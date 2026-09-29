pub mod pull;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::cache::{cache_path, ensure_cache_dir};
use crate::datadir::assets_dir;
use crate::mgr::AppMgr;

/// 资源缓存根目录下的相对路径转换为完整路径。
pub(crate) fn asset_cache_path(data_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let dir = assets_dir(data_dir)?;
    Ok(cache_path(&dir, relative))
}

/// 获取已存在资源的完整路径。
pub(crate) fn get_asset_path(data_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = asset_cache_path(data_dir, relative)?;
    if !path.exists() {
        return Err(format!("资源不存在: {}", relative));
    }
    Ok(path)
}

/// 确保资源缓存目录存在。
pub(crate) fn ensure_asset_cache_dir(data_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    let dir = asset_cache_path(data_dir, relative)?;
    ensure_cache_dir(&dir)?;
    Ok(dir)
}

/// 获取 Assets 资源本地路径（前端通过 convertFileSrc 转为 webview 可访问 URL）。
#[tauri::command]
pub(crate) fn get_asset_url(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    path: String,
) -> Result<String, String> {
    mgr.read().asset_path(&path)
}

/// 获取当前配置的资产服务器地址。
#[tauri::command]
pub(crate) fn get_assets_server_url(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> String {
    crate::asset_server::assets_server_url(mgr.data_dir())
}
