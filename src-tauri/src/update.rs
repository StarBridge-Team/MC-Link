use std::path::{Path, PathBuf};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::cache::{cache_path, ensure_cache_dir};
use crate::downloader::Downloader;
use crate::asset_server::{assets_server_url, join};

const UPDATE_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// 更新信息。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
    pub release_notes: String,
    pub release_date: String,
    pub mandatory: bool,
    pub sha256: Option<String>,
}

/// 更新检查结果。
#[derive(Serialize, Clone, Debug)]
pub struct CheckUpdateResult {
    pub has_update: bool,
    pub current_version: String,
    pub latest: Option<UpdateInfo>,
}

/// 下载更新结果。
#[derive(Serialize, Clone, Debug)]
pub struct DownloadUpdateResult {
    pub success: bool,
    pub path: Option<String>,
    pub message: String,
}

fn update_server_url(data_dir: &Path) -> String {
    assets_server_url(data_dir)
}

fn update_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("Cache").join("Updates")
}

/// 比较两个语义化版本字符串，仅支持 `x.y.z` 格式。
fn version_greater(left: &str, right: &str) -> bool {
    let parse = |s: &str| {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect::<Vec<_>>()
    };
    let a = parse(left);
    let b = parse(right);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        if av != bv {
            return av > bv;
        }
    }
    false
}

/// 从远程服务器检查更新。
pub async fn check_update(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<CheckUpdateResult, String> {
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let url = join(&update_server_url(data_dir), "update/latest.json");

    let info: UpdateInfo = client
        .get(&url)
        .timeout(UPDATE_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("检查更新失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析更新信息失败: {}", e))?;

    let has_update = version_greater(&info.version, &current_version);

    Ok(CheckUpdateResult {
        has_update,
        current_version,
        latest: if has_update { Some(info) } else { None },
    })
}

/// 下载最新版本安装包到本地缓存。
pub async fn download_update(
    data_dir: &Path,
    info: UpdateInfo,
    client: &reqwest::Client,
) -> Result<DownloadUpdateResult, String> {
    let cache_dir = update_cache_dir(data_dir);
    ensure_cache_dir(&cache_dir)?;

    let filename = info
        .download_url
        .split('/')
        .last()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("mc-link-update-{}", info.version));
    let dest = cache_path(&cache_dir, &filename);

    let downloader = Downloader::new(client.clone()).with_timeout(DOWNLOAD_TIMEOUT);
    downloader
        .download(&info.download_url, &dest, info.sha256.as_deref())
        .await
        .map_err(|e| format!("下载更新包失败: {}", e))?;

    Ok(DownloadUpdateResult {
        success: true,
        path: Some(dest.to_string_lossy().to_string()),
        message: "下载完成".to_string(),
    })
}

/// 清理本地更新缓存。
pub fn clear_update_cache(data_dir: &Path) -> Result<(), String> {
    let dir = update_cache_dir(data_dir);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir)
        .map_err(|e| format!("清理更新缓存失败: {}", e))
}

#[tauri::command]
pub(crate) async fn check_update_command(
    mgr: tauri::State<'_, std::sync::Arc<crate::mgr::AppMgr>>,
) -> Result<CheckUpdateResult, String> {
    mgr.pull().check_update().await
}

#[tauri::command]
pub(crate) async fn download_update_command(
    mgr: tauri::State<'_, std::sync::Arc<crate::mgr::AppMgr>>,
    info: UpdateInfo,
) -> Result<DownloadUpdateResult, String> {
    mgr.pull().download_update(info).await
}

#[tauri::command]
pub(crate) fn clear_update_cache_command(
    mgr: tauri::State<'_, std::sync::Arc<crate::mgr::AppMgr>>,
) -> Result<(), String> {
    mgr.write().clear_update_cache()
}
