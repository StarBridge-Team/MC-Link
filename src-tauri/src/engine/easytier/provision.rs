//! 引擎的获取与安装：下载 → 校验 → 解压 → 就位。
//!
//! # 三条硬性约束
//!
//! 1. **只有一个下载实现**：传输层复用 [`crate::downloader::verified`]（多镜像、
//!    断点续传、SHA256 强制校验），本模块只负责"清单字段映射 + 解压 + 就位"。
//! 2. **失败必须可重试且不留下半成品**：解压写在 `<version>.tmp`，校验通过后才
//!    改名成正式目录；中途失败只留一个临时目录，下次启动会先清掉它。
//! 3. **并发只下载一次**：界面可能在多个窗口同时触发 `adapter.init`，用一把锁
//!    加双检把它们收敛成一次下载，其余调用等结果。
//!
//! # 为什么是阻塞接口
//!
//! 适配器（`plugin/builtin/*`）跑在 `spawn_blocking` 上，本来就是同步语义；
//! 这里内部临时起一个单线程 runtime 去跑 `download_verified`，而不是把 async
//! 传染给整个适配器层。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::artifact::{self, Artifact};
use super::binary::{self, Binaries};

/// 引擎安装状态（供 UI 显示"首次使用需要下载引擎"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionState {
    /// 本机还没有引擎，且当前没有下载在进行。
    Missing,
    /// 正在下载/解压。
    Downloading,
    /// 已就位。
    Ready,
}

/// 安装过程串行化：同一时刻只允许一个线程真正下载。
static INSTALL_LOCK: Mutex<()> = Mutex::new(());
/// 是否有下载在进行（[`ProvisionState::Downloading`] 的唯一来源）。
static BUSY: AtomicBool = AtomicBool::new(false);

/// 下载超时（连接级）。传输空闲超时由 `downloader::verified` 按设置项控制。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// 解压上限：挡住"哈希正确、但展开后撑爆磁盘"的包（防御纵深，正常包远小于此）。
const MAX_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
/// 解压条目上限。
const MAX_ENTRIES: usize = 512;

/// 引擎根目录：`<data_dir>/Engine/EasyTier`。
fn engine_root(data_dir: &Path) -> PathBuf {
    data_dir.join("Engine").join("EasyTier")
}

/// 某个版本的安装目录：`<data_dir>/Engine/EasyTier/<version>`。
pub fn install_dir(data_dir: &Path) -> PathBuf {
    engine_root(data_dir).join(artifact::VERSION)
}

/// 当前状态。
pub fn state(data_dir: &Path) -> ProvisionState {
    if installed(data_dir).is_some() {
        ProvisionState::Ready
    } else if BUSY.load(Ordering::SeqCst) {
        ProvisionState::Downloading
    } else {
        ProvisionState::Missing
    }
}

/// 已安装则返回可执行文件（顺带补齐可执行位）。
pub fn installed(data_dir: &Path) -> Option<Binaries> {
    let found = binary::locate(&install_dir(data_dir))?;
    // 每次取用都补一次：幂等、廉价，且能救回"解压丢了权限位"的老安装
    let _ = binary::ensure_executable(&found.core);
    let _ = binary::ensure_executable(&found.cli);
    Some(found)
}

/// 确保引擎就位；不在就下载。可重入、并发安全。
///
/// `on_progress` 收到 0–100 的百分比（总长未知时上报 0）。
pub fn ensure_blocking(
    data_dir: &Path,
    mut on_progress: impl FnMut(u8) + Send,
) -> Result<Binaries, String> {
    if let Some(found) = installed(data_dir) {
        return Ok(found);
    }

    // 锁中毒也要继续：这里没有"必须恢复的共享不变量"，重试一次比整机卡死强
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // 双检：等锁期间别人可能已经装好了
    if let Some(found) = installed(data_dir) {
        return Ok(found);
    }

    BUSY.store(true, Ordering::SeqCst);
    let result = install(data_dir, &mut on_progress);
    BUSY.store(false, Ordering::SeqCst);
    result
}

/// 在后台线程里准备引擎（不等待结果）。
///
/// 供 `adapter.init` 调用：联机页一打开就开始下载，用户点"开始联机"时通常已经装好。
pub fn ensure_background(data_dir: &Path) {
    if installed(data_dir).is_some() || BUSY.load(Ordering::SeqCst) {
        return;
    }
    let dir = data_dir.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("easytier-provision".to_string())
        .spawn(move || match ensure_blocking(&dir, |_| {}) {
            Ok(_) => println!("[引擎] EasyTier 已就绪"),
            Err(e) => eprintln!("[引擎] EasyTier 准备失败: {}", e),
        });
    if let Err(e) = spawned {
        // 线程都起不来时不能再吞掉：否则状态永远停在 Missing 且没有线索
        eprintln!("[引擎] 无法启动下载线程: {}", e);
    }
}

/// 完整安装流程。
fn install(
    data_dir: &Path,
    on_progress: &mut (impl FnMut(u8) + Send),
) -> Result<Binaries, String> {
    let artifact = artifact::current().ok_or_else(|| {
        format!(
            "当前平台（{}）没有可用的 EasyTier 引擎产物。\
             内置引擎只随发布版本提供已登记的平台，请换用受支持的平台或在插件市场安装第三方适配器。",
            artifact::platform_label()
        )
    })?;

    let staging = engine_root(data_dir).join(format!("{}.tmp", artifact::VERSION));
    // 上一轮留下的临时目录一律先清掉：里面可能是校验失败或解压到一半的内容
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("创建引擎目录失败（{}）: {}", staging.display(), e))?;

    let result = (|| {
        println!("[引擎] 开始获取 EasyTier {}（{}）", artifact::VERSION, artifact.file);
        let archive = download(&staging, artifact, on_progress)?;
        println!("[引擎] 校验通过，开始解压");
        extract(&archive, &staging)?;
        let _ = std::fs::remove_file(&archive);
        Ok::<(), String>(())
    })();

    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    // 解压结果必须先自检：没有可执行文件的目录装上去也只是一个永远起不来的引擎
    if binary::locate(&staging).is_none() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "引擎包 {} 解压后没有找到 easytier-core / easytier-cli，已放弃安装",
            artifact.file
        ));
    }

    let final_dir = install_dir(data_dir);
    let _ = std::fs::remove_dir_all(&final_dir);
    if let Some(parent) = final_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建引擎目录失败（{}）: {}", parent.display(), e))?;
    }
    std::fs::rename(&staging, &final_dir).map_err(|e| {
        format!(
            "引擎就位失败（{} → {}）: {}",
            staging.display(),
            final_dir.display(),
            e
        )
    })?;

    installed(data_dir).ok_or_else(|| "引擎安装后仍无法定位可执行文件".to_string())
}

/// 下载发行包到 `dir`，返回落地路径。传输层复用唯一实现。
fn download(
    dir: &Path,
    artifact: &Artifact,
    on_progress: &mut (impl FnMut(u8) + Send),
) -> Result<PathBuf, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建下载运行时失败: {}", e))?;

    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

        let origin = artifact::origin_url(artifact);
        // 直连 + 代理择优；全部探测失败时退回"直连 + 全部代理"，让下载层自己去撞
        let mut urls = crate::domain::proxy::ordered_channels(&client, &origin).await;
        if urls.is_empty() {
            urls = fallback_channels(&origin);
        }

        let remote = crate::downloader::verified::RemoteFile {
            file: artifact.file,
            urls: &urls,
            sha256: artifact.sha256,
            size: Some(artifact.size),
        };
        let report = on_progress;
        crate::downloader::verified::download_verified(dir, &client, &remote, |done, total| {
            if total > 0 {
                let pct = (done as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
                report(pct as u8);
            } else {
                report(0);
            }
        })
        .await
    })
}

/// 探测全部失败时的兜底候选：直连排首，其后是代理池原样套前缀。
fn fallback_channels(origin: &str) -> Vec<String> {
    let mut out = vec![origin.to_string()];
    for proxy in crate::domain::proxy::pool::snapshot() {
        out.push(crate::domain::proxy::probe::proxied_url(&proxy, origin));
    }
    out
}

/// 解压到 `dest`。
///
/// 用 `enclosed_name` 剔除绝对路径与 `..`：包虽然先过 SHA256，但"校验过的包"
/// 仍可能被上游打进畸形条目，解压这一层不该成为写任意路径的机会。
fn extract(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive)
        .map_err(|e| format!("打开安装包失败（{}）: {}", archive.display(), e))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("解析安装包失败: {}", e))?;

    if zip.len() > MAX_ENTRIES {
        return Err(format!(
            "安装包条目过多（{} 项，上限 {}），已中止",
            zip.len(),
            MAX_ENTRIES
        ));
    }

    let mut unpacked: u64 = 0;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| format!("读取安装包条目失败: {}", e))?;

        let Some(relative) = entry.enclosed_name() else {
            // 绝对路径或含 `..`：跳过并留痕，不中断整个安装（其余条目仍然有效）
            eprintln!(
                "[引擎] 跳过可疑条目（路径越界）: {}",
                entry.name().to_string()
            );
            continue;
        };
        let target = dest.join(&relative);

        if entry.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|e| format!("创建目录失败（{}）: {}", target.display(), e))?;
            continue;
        }

        // 符号链接不落地：跨平台语义不一致，且引擎包不需要它
        if entry.unix_mode().map(|m| m & 0o170000 == 0o120000).unwrap_or(false) {
            eprintln!("[引擎] 跳过符号链接: {}", entry.name());
            continue;
        }

        unpacked = unpacked.saturating_add(entry.size());
        if unpacked > MAX_UNPACKED_BYTES {
            return Err(format!(
                "安装包展开后过大（超过 {} MB），已中止",
                MAX_UNPACKED_BYTES / 1024 / 1024
            ));
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录失败（{}）: {}", parent.display(), e))?;
        }
        let mut out = std::fs::File::create(&target)
            .map_err(|e| format!("写入失败（{}）: {}", target.display(), e))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("解压失败（{}）: {}", target.display(), e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclink-engine-prov-{}-{}",
            name,
            crate::plugin::crypto::random_hex(4)
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// 造一个含指定条目的 zip。
    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn state_is_missing_without_install_dir() {
        let dir = temp_dir("missing");
        assert_eq!(state(&dir), ProvisionState::Missing);
        assert!(installed(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_writes_nested_entries() {
        let dir = temp_dir("extract");
        let archive = dir.join("pkg.zip");
        make_zip(
            &archive,
            &[
                ("easytier-x86/easytier-core.exe", b"core"),
                ("easytier-x86/wintun.dll", b"dll"),
            ],
        );
        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        extract(&archive, &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("easytier-x86").join("wintun.dll")).unwrap(),
            b"dll"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_refuses_to_escape_destination() {
        let dir = temp_dir("traversal");
        let archive = dir.join("evil.zip");
        make_zip(&archive, &[("../escaped.txt", b"boom")]);

        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        extract(&archive, &dest).expect("越界条目应被跳过而不是整体失败");

        assert!(
            !dir.join("escaped.txt").exists(),
            "越界条目不得写到目标目录之外"
        );
        assert!(!dest.join("..").join("escaped.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fallback_channels_start_with_direct_and_include_proxies() {
        let origin = "https://github.com/o/r/releases/download/v1/a.zip";
        let channels = fallback_channels(origin);
        assert_eq!(channels[0], origin);
        assert!(
            channels.len() > 1 && channels[1].starts_with("https://"),
            "代理候选应排在直连之后: {:?}",
            channels
        );
    }

    #[test]
    fn installed_dir_is_versioned() {
        let dir = temp_dir("layout");
        assert!(install_dir(&dir).ends_with(artifact::VERSION));
        assert!(install_dir(&dir).starts_with(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_executable_sets_bits_on_extracted_binary() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("exec");
        let archive = dir.join("pkg.zip");
        make_zip(&archive, &[("bin/easytier-core", b"core")]);
        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        extract(&archive, &dest).unwrap();

        let exe = dest.join("bin").join("easytier-core");
        binary::ensure_executable(&exe).unwrap();
        let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "应补齐可执行位，实际 {:o}", mode);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
