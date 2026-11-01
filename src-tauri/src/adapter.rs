//! 内置适配器的运行载体：管理陶瓦 exe 的生命周期、端口发现与状态落盘。
//!
//! 本模块**不再直接对外暴露 Tauri 命令**。它现在只服务于
//! [`crate::plugin::builtin::terracotta::TerracottaProvider`]，由后者把它包装成
//! 标准的 `adapter.*` 能力。对外接口统一走插件管理器。

use crate::utils::lock_or_recover;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 适配器状态。
///
/// 字段名与磁盘上的 `Terracotta.json` 以及前端契约保持一致，请勿重命名。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AdapterStatus {
    pub installed: bool,
    pub running: bool,
    pub starting: bool,
    pub port: Option<u16>,
    /// 落地目录里实际找到的陶瓦可执行文件名（不含路径）。
    ///
    /// 上游包内的可执行文件名随平台/版本变化（Windows 形如
    /// `terracotta-0.4.2-windows-x86_64.exe`，Unix 形如
    /// `terracotta-0.4.2-linux-x86_64`），因此客户端**不硬编码**文件名，
    /// 而是在落地目录里发现，并把结果记在这里供下次启动直接复用。
    ///
    /// 老状态文件没有这个字段时按 `#[serde(default)]` 视为 `None`，
    /// 此时会重新扫描目录，不影响升级。
    #[serde(default)]
    pub exe_name: Option<String>,
}

pub struct AdapterManager {
    pub adapter_dir: PathBuf,
    pub terracotta_port: Arc<Mutex<Option<u16>>>,
}

impl AdapterManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let adapter_dir = data_dir.join("Adapter");

        AdapterManager {
            adapter_dir,
            terracotta_port: Arc::new(Mutex::new(None)),
        }
    }

    fn status_path(&self) -> PathBuf {
        self.adapter_dir.join("Terracotta.json")
    }

    fn read_status(&self) -> AdapterStatus {
        // 文件损坏时由 persist 隔离原文件并返回默认值
        crate::persist::load_json::<AdapterStatus>(&self.status_path())
            .map(|loaded| loaded.value)
            .unwrap_or_default()
    }

    fn write_status(&self, status: &AdapterStatus) {
        // 旧实现用 `let _ =` 吞掉落盘错误，导致"状态改了但没保存"无从察觉
        if let Err(e) = crate::persist::save_json(&self.status_path(), status) {
            eprintln!("[适配器] 状态落盘失败: {}", e);
        }
    }

    fn is_port_alive(port: u16) -> bool {
        std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(2),
        )
        .is_ok()
    }

    pub fn get_status(&self) -> AdapterStatus {
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        if let Some(p) = port {
            if !Self::is_port_alive(p) {
                *lock_or_recover(&self.terracotta_port, "terracotta_port") = None;
            }
        }

        let mut status = self.read_status();
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        status.running = port.is_some();
        status.port = port;
        self.write_status(&status);
        status
    }

    pub fn launch_all(&self) {
        if lock_or_recover(&self.terracotta_port, "terracotta_port").is_some() {
            return;
        }
        let status = self.read_status();
        if status.installed {
            self.spawn_terracotta_process();
            self.poll_terracotta_port();
        } else {
            println!("[适配器] 未安装，跳过阻塞下载（将由前端触发下载）");
        }
    }

    fn spawn_terracotta_process(&self) {
        let terracotta_dir = self.adapter_dir.join("Terracotta");
        let platform = crate::assets::adapter::current_platform();

        let Some(exe_path) = self.resolve_terracotta_exe(&terracotta_dir, &platform) else {
            let mut status = self.read_status();
            status.installed = false;
            status.exe_name = None;
            self.write_status(&status);
            eprintln!(
                "[适配器] 在 {} 里没有找到陶瓦可执行文件（该目录下的内容：{}），无法启动",
                terracotta_dir.display(),
                describe_dir(&terracotta_dir)
            );
            return;
        };

        // Unix 上 tar 解压可能丢可执行位：尽力补上，补不上就跳过启动（不 panic）
        #[cfg(unix)]
        if !is_executable_file(&exe_path) {
            if let Err(e) = make_executable(&exe_path) {
                eprintln!(
                    "[适配器] 无法为 {} 补上可执行位（{}），已跳过启动",
                    exe_path.display(),
                    e
                );
                return;
            }
        }

        let port_dir = self.adapter_dir.join("Terracotta.temp");
        let _ = std::fs::create_dir_all(&port_dir);
        let hmcl_file = port_dir.join("terracotta-port.json");
        let hmcl_str = hmcl_file.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&hmcl_file);

        if let Err(e) = std::process::Command::new(&exe_path)
            .args(["--hmcl", &hmcl_str])
            .current_dir(&terracotta_dir)
            .spawn()
        {
            eprintln!("[适配器] 启动陶瓦进程失败（{}）: {}", exe_path.display(), e);
        }

        let status = self.read_status();
        let mut new_status = status.clone();
        new_status.installed = true;
        new_status.exe_name = exe_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        self.write_status(&new_status);
    }

    /// 定位落地目录里的陶瓦可执行文件。
    ///
    /// 先用状态文件里记住的文件名（省一次目录扫描），找不到再按规则发现。
    fn resolve_terracotta_exe(&self, dir: &Path, platform: &str) -> Option<PathBuf> {
        let status = self.read_status();
        if let Some(name) = status.exe_name.as_deref() {
            // 只接受单层文件名，避免状态文件被篡改后越出落地目录
            if Path::new(name).components().count() == 1 {
                let path = dir.join(name);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        find_terracotta_exe(dir, platform)
    }

    fn poll_terracotta_port(&self) {
        let port_dir = self.adapter_dir.join("Terracotta.temp");
        let hmcl_file = port_dir.join("terracotta-port.json");

        let start = Instant::now();
        let timeout = Duration::from_secs(12);
        loop {
            if start.elapsed() > timeout {
                break;
            }
            if let Ok(content) = std::fs::read_to_string(&hmcl_file) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(port) = json.get("port").and_then(|v| v.as_u64()) {
                        let port = port as u16;
                        if Self::is_port_alive(port) {
                            *lock_or_recover(&self.terracotta_port, "terracotta_port") = Some(port);
                            println!("[适配器] 陶瓦联机端口: {}", port);
                            let _ = std::fs::remove_file(&hmcl_file);
                            break;
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    pub fn ensure_running(&self) -> Result<u16, String> {
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        if let Some(p) = port {
            return Ok(p);
        }

        let status = self.read_status();
        if !status.installed {
            return Err("陶瓦联机未安装".to_string());
        }

        self.spawn_terracotta_process();
        self.poll_terracotta_port();

        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        match port {
            Some(p) => {
                let mut s = self.read_status();
                s.starting = false;
                s.running = true;
                s.port = Some(p);
                self.write_status(&s);
                Ok(p)
            }
            None => {
                let mut s = self.read_status();
                s.starting = false;
                self.write_status(&s);
                Err("陶瓦联机自动启动超时".to_string())
            }
        }
    }

    pub fn mark_installed(&self) {
        let mut status = self.read_status();
        status.installed = true;
        self.write_status(&status);
    }

    pub fn launch_terracotta_after_download(&self) {
        self.mark_installed();
        self.spawn_terracotta_process();
        self.poll_terracotta_port();
    }

    pub fn shutdown_all(&self) {
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        if let Some(p) = port {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .ok();
            if let Some(client) = client {
                let url = format!("http://127.0.0.1:{}/panic?peaceful=true", p);
                let _ = client.get(&url).send();
            }
            *lock_or_recover(&self.terracotta_port, "terracotta_port") = None;
        }

        let mut status = self.read_status();
        status.running = false;
        status.starting = false;
        status.port = None;
        self.write_status(&status);
        println!("[适配器] 所有适配器已关闭");
    }
}

/// 在落地目录里找出陶瓦可执行文件。
///
/// 上游包内的可执行文件名**随平台与版本变化**（Windows 形如
/// `terracotta-0.4.2-windows-x86_64.exe`，Unix 形如
/// `terracotta-0.4.2-linux-x86_64`），因此不能写死文件名，只能在这里发现。
///
/// 判定规则：
/// - 名称（忽略大小写）以 `terracotta` 开头；
/// - Windows：扩展名必须是 `exe`；Unix：没有扩展名；
/// - Unix 上还要有可执行位（`PermissionsExt`）；
/// - 名称里**含当前平台标签**的优先，没有就取第一个候选。
///
/// 找不到返回 `None`。
fn find_terracotta_exe(dir: &Path, platform: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let platform_lower = platform.to_ascii_lowercase();

    let mut best: Option<(u8, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().starts_with("terracotta") {
            continue;
        }
        if !has_executable_name(name) {
            continue;
        }
        // 评分：平台标签命中 +2，有可执行位 +1。得分相同按名称排序，保证结果稳定。
        let mut score = 0u8;
        if !platform_lower.is_empty() && name.to_ascii_lowercase().contains(&platform_lower) {
            score += 2;
        }
        if is_executable_file(&path) {
            score += 1;
        }
        let replace = match &best {
            Some((best_score, best_path)) => {
                score > *best_score
                    || (score == *best_score
                        && path.file_name() < best_path.file_name())
            }
            None => true,
        };
        if replace {
            best = Some((score, path));
        }
    }
    best.map(|(_, path)| path)
}

/// 名称是否像"可执行文件"：Windows 必须是 `.exe`，Unix 不能有扩展名。
///
/// 只把**纯字母**的尾段当作扩展名：上游文件名里带版本号
/// （`terracotta-0.4.2-linux-x86_64`），按最后一个 `.` 生切会把 `0.4.2`
/// 的片段误判成扩展名，那样 Unix 上的可执行文件就永远选不出来。
fn has_executable_name(name: &str) -> bool {
    has_executable_name_for(name, cfg!(windows))
}

/// [`has_executable_name`] 的可测试版本：`windows` 显式传入。
fn has_executable_name_for(name: &str, windows: bool) -> bool {
    let ext = alpha_extension(name);
    if windows {
        ext.map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false)
    } else {
        ext.is_none()
    }
}

/// 取"纯字母型"扩展名；不是扩展名（如版本号片段）返回 `None`。
fn alpha_extension(name: &str) -> Option<&str> {
    let (_, ext) = name.rsplit_once('.')?;
    if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(ext)
}

/// 文件是否带可执行位。Windows 上没有这个概念，一律视为可执行。
#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(_path: &Path) -> bool {
    true
}

/// 尽力给文件补上可执行位（tar 解压可能丢权限位）。
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mut perm = meta.permissions();
    let mode = perm.mode();
    perm.set_mode(mode | 0o111);
    std::fs::set_permissions(path, perm).map_err(|e| e.to_string())
}

/// 列出目录下的文件名，用于"没找到可执行文件"时的日志。
fn describe_dir(dir: &Path) -> String {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            if names.is_empty() {
                "（空目录）".to_string()
            } else {
                names.join(", ")
            }
        }
        Err(e) => format!("（无法读取目录: {}）", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每次调用都用唯一目录，避免并行测试互相看到对方的残留文件。
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mc-link-adapter-exe-{}-{}",
            name,
            crate::plugin::crypto::random_hex(4)
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// 按当前平台的命名规则造一个候选可执行文件。
    fn write_candidate(dir: &Path, base: &str) -> PathBuf {
        let name = if cfg!(windows) {
            format!("{}.exe", base)
        } else {
            base.to_string()
        };
        let path = dir.join(name);
        std::fs::write(&path, b"stub").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&path).unwrap().permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&path, perm).unwrap();
        }
        path
    }

    #[test]
    fn no_candidate_returns_none() {
        let dir = temp_dir("none");
        std::fs::write(dir.join("readme.txt"), b"x").unwrap();
        assert!(
            find_terracotta_exe(&dir, "windows-x86_64").is_none(),
            "目录里没有陶瓦可执行文件时应返回 None"
        );
    }

    #[test]
    fn candidate_matching_platform_tag_wins() {
        let dir = temp_dir("platform");
        write_candidate(&dir, "terracotta-0.4.2-linux-x86_64");
        let preferred = write_candidate(&dir, "terracotta-0.4.2-windows-x86_64");

        let found = find_terracotta_exe(&dir, "windows-x86_64")
            .expect("应能找到候选可执行文件");
        assert_eq!(
            found.file_name(),
            preferred.file_name(),
            "应优先选文件名含当前平台标签的那个"
        );
    }

    #[test]
    fn extra_terracotta_named_files_are_not_picked() {
        // 上游 macOS 包里同时有 `terracotta-0.4.2-macos-arm64` 与
        // `terracotta-0.4.2-macos-arm64.pkg`（后者是安装包，不是可执行文件）
        let dir = temp_dir("extra");
        let exe = write_candidate(&dir, "terracotta-0.4.2-macos-arm64");
        std::fs::write(dir.join("terracotta-0.4.2-macos-arm64.pkg"), b"stub").unwrap();

        let found = find_terracotta_exe(&dir, "macos-arm64")
            .expect("应能找到候选可执行文件");
        assert_eq!(
            found.file_name(),
            exe.file_name(),
            "应选中真正的可执行文件，而不是同名的 .pkg"
        );
    }

    #[test]
    fn versioned_name_without_extension_is_accepted() {
        // 上游文件名带版本号（0.4.2），Unix 上不能把它误判成"有扩展名"
        assert!(has_executable_name_for(
            "terracotta-0.4.2-linux-x86_64",
            false
        ));
        assert!(!has_executable_name_for(
            "terracotta-0.4.2-linux-x86_64.exe",
            false
        ));
        // Windows 上必须有 .exe
        assert!(has_executable_name_for(
            "terracotta-0.4.2-windows-x86_64.exe",
            true
        ));
        assert!(!has_executable_name_for(
            "terracotta-0.4.2-windows-x86_64",
            true
        ));
    }

    #[cfg(unix)]
    #[test]
    fn file_without_executable_bit_is_still_discovered() {
        // tar 解压可能丢权限位：文件仍需能被找到，随后由 make_executable 补权限
        let dir = temp_dir("noexec");
        let path = dir.join("terracotta-0.4.2-linux-x86_64");
        std::fs::write(&path, b"stub").unwrap();
        assert!(find_terracotta_exe(&dir, "linux-x86_64").is_some());
    }
}
