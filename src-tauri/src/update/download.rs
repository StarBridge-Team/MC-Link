//! 更新包的下载与缓存。
//!
//! 下载本身走 [`crate::downloader::verified`]——**与适配器共用同一套校验链路**
//! （多镜像、体积上限、分片超时、SHA256 强制校验）。更新包会被直接执行，
//! 校验强度不能低于第三方二进制。

use std::path::{Path, PathBuf};

use crate::asset_server::assets_server_url;
use crate::cache::ensure_cache_dir;

use super::model::{asset_urls, DownloadUpdateResult, UpdateAsset};

/// 更新包缓存目录。
pub(crate) fn update_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("Cache").join("Updates")
}

/// 下载更新包到本地缓存。
///
/// 已存在且哈希一致时直接复用，因此"下载 → 安装失败 → 重试"不会重复下载。
pub(crate) async fn download_asset(
    data_dir: &Path,
    client: &reqwest::Client,
    asset: &UpdateAsset,
    on_progress: impl FnMut(u64, u64) + Send,
) -> Result<DownloadUpdateResult, String> {
    let dir = update_cache_dir(data_dir);
    ensure_cache_dir(&dir)?;

    let urls = asset_urls(asset, &assets_server_url(data_dir));
    let remote = crate::downloader::verified::RemoteFile {
        file: &asset.file,
        urls: &urls,
        sha256: &asset.sha256,
        size: asset.size,
    };

    let path =
        crate::downloader::verified::download_verified(&dir, client, &remote, on_progress).await?;
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    Ok(DownloadUpdateResult {
        success: true,
        file: asset.file.clone(),
        size,
        message: format!("更新包已就绪（{} 字节）", size),
    })
}

/// 清理更新缓存（安装包与落地日志一并删除）。
pub(crate) fn clear_update_cache(data_dir: &Path) -> Result<(), String> {
    let dir = update_cache_dir(data_dir);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("清理更新缓存失败: {}", e))
}

/// 清理缓存里**已经用完**的更新包与下载残留（启动时调用）。
///
/// 判定规则：文件名形如 `MC-Link-<版本>-<平台>-<类型>.<扩展名>` 时，
/// **版本不高于当前应用版本**的包已经用完（装过了，或已被替换掉）——
/// 而比当前版本新的包必须留着：用户可能下载完选择了"稍后安装"。
///
/// 续传产生的 `*.part` / `*.progress` 同样是 `MC-Link-<版本>-` 前缀，
/// 因此也按同一版本规则处理：旧版本的半成品删掉，新版本的留着供续传。
///
/// 认不出格式的文件（例如 `install.log`）一律保留：宁可多留一个文件，
/// 也不要删掉可能还有用的东西。
pub(crate) fn prune_cache(data_dir: &Path) {
    let dir = update_cache_dir(data_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let current = env!("CARGO_PKG_VERSION");

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();

        // 下载中断的残留（`.tmp` 是 verified 下载器的中间文件）
        if name.ends_with(".tmp") {
            let _ = std::fs::remove_file(&path);
            continue;
        }

        if let Some(version) = package_version(&name) {
            // 不比当前版本新 → 已经用完，可删
            if !super::model::version_greater(&version, current) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// 从包文件名解析版本号：`MC-Link-0.4.1-windows-x86_64-portable.exe` → `0.4.1`。
///
/// 只接受严格的 `x.y.z`，其余一律返回 `None`（表示"认不出，别动它"）。
fn package_version(file_name: &str) -> Option<String> {
    let rest = file_name.strip_prefix("MC-Link-")?;
    let version = rest.split('-').next()?;
    let parts: Vec<&str> = version.split('.').collect();
    let well_formed = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    well_formed.then(|| version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_from_package_name() {
        assert_eq!(
            package_version("MC-Link-0.4.1-windows-x86_64-portable.exe").as_deref(),
            Some("0.4.1")
        );
        assert_eq!(
            package_version("MC-Link-0.10.0-windows-x86_64-setup.exe").as_deref(),
            Some("0.10.0")
        );
    }

    #[test]
    fn ignores_names_it_does_not_understand() {
        // 认不出就返回 None —— 调用方据此保留文件，避免误删
        assert_eq!(package_version("install.log"), None);
        assert_eq!(package_version("MC-Link-0.4-windows-x64.exe"), None);
        assert_eq!(package_version("MC-Link-dev-windows-x64.exe"), None);
        assert_eq!(package_version("other-1.2.3-x.exe"), None);
    }

    #[test]
    fn prune_removes_consumed_packages_and_tmp_only() {
        let dir = update_cache_dir(&std::env::temp_dir().join("mc-link-prune-test"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let write = |name: &str| std::fs::write(dir.join(name), b"x").unwrap();
        // 用当前版本号构造"同版本"用例，避免版本自增后测试失效
        let current = env!("CARGO_PKG_VERSION");
        write("MC-Link-0.0.1-windows-x86_64-portable.exe"); // 旧版本 → 删
        write(&format!("MC-Link-{current}-windows-x86_64-setup.exe")); // 与当前同版本 → 删
        write("MC-Link-99.0.0-windows-x86_64-portable.exe"); // 更新 → 留
        write("download.exe.tmp"); // 残留 → 删
        write("install.log"); // 认不出 → 留

        prune_cache(&std::env::temp_dir().join("mc-link-prune-test"));

        let left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        assert!(left.contains(&"MC-Link-99.0.0-windows-x86_64-portable.exe".to_string()));
        assert!(left.contains(&"install.log".to_string()));
        assert!(!left.contains(&"MC-Link-0.0.1-windows-x86_64-portable.exe".to_string()));
        assert!(!left.contains(&"download.exe.tmp".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
