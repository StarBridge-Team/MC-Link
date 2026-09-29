pub mod pull;

use std::path::{PathBuf};
use std::sync::Arc;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::cache::cache_path;
use crate::mgr::AppMgr;

pub(crate) const PAGE_CACHE_TTL: Duration = Duration::from_secs(3600);

/// 页面清单条目。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PageEntry {
    pub name: String,
    pub path: String,
    pub sha256: Option<String>,
}

/// 页面清单。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PageManifest {
    pub base_url: Option<String>,
    pub pages: Vec<PageEntry>,
}

/// 清理页面名称，仅保留安全字符。
pub(crate) fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

/// 页面缓存目录。
pub(crate) fn page_cache_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("Cache").join("Pages")
}

/// 清单缓存文件路径。
pub(crate) fn manifest_cache_file(data_dir: &std::path::Path) -> PathBuf {
    cache_path(&page_cache_dir(data_dir), "manifest.json")
}

/// 读取本地缓存的清单。
pub(crate) fn read_cached_manifest(data_dir: &std::path::Path) -> Result<PageManifest, String> {
    let path = manifest_cache_file(data_dir);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取页面清单缓存失败: {}", e))?;
    serde_json::from_str(&text)
        .map_err(|e| format!("解析页面清单缓存失败: {}", e))
}

/// 清理页面缓存。
pub(crate) fn clear_page_cache(data_dir: &std::path::Path) -> Result<(), String> {
    let dir = page_cache_dir(data_dir);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir)
        .map_err(|e| format!("清理页面缓存失败: {}", e))
}

#[tauri::command]
pub(crate) async fn get_page_manifest(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<PageManifest, String> {
    mgr.pull().page_manifest().await
}

#[tauri::command]
pub(crate) async fn get_page_content(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    name: String,
) -> Result<String, String> {
    mgr.pull().page_content(&name).await
}

#[tauri::command]
pub(crate) fn clear_page_cache_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<(), String> {
    mgr.write().clear_page_cache()
}
