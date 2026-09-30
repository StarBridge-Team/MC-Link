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
