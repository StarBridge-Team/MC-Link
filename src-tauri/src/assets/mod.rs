pub mod pull;
pub(crate) mod adapter;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::cache::cache_path;
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

/// 获取 Assets 资源本地路径（前端通过 convertFileSrc 转为 webview 可访问 URL）。
#[tauri::command]
pub(crate) fn get_asset_url(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    path: String,
) -> Result<String, String> {
    mgr.read().asset_path(&path)
}

// 这里曾有一个 `get_assets_server_url` 命令，用于让前端在 dev 模式下直接连资源服务器。
// 资源加载已统一为"后端同步到本地缓存 → 前端注入 asset:// 本地文件"，
// dev 与 prod 走同一条路径，前端不再需要知道服务器地址，命令随之删除。

/// 单个文本资源允许读取的上限（当前最大的 CSS 约 100 KB，留足余量）。
const MAX_TEXT_ASSET_BYTES: u64 = 2 * 1024 * 1024;

/// 读取 Assets 目录内某个文本资源的内容。
///
/// # 为什么需要这个命令
///
/// 通过 `asset:` 协议加载的 CSS，其内部的**相对 `url()` 无法正确解析**：
/// `convertFileSrc` 会把整条绝对路径百分号编码成**单个路径段**（连 `/` 都编码成
/// `%2F`），浏览器从 URL 角度看"目录"就是 asset 根，于是
/// `url("./bootstrap-icons.woff2")` 被解析成 `http://asset.localhost/bootstrap-icons.woff2`
/// → 404 → 字体不加载 → 图标全是豆腐块。
///
/// 前端因此改为：取到 CSS 原文 → 把相对 `url()` 重写成绝对的 `convertFileSrc` 地址
/// → 以 `<style>` 注入。既解决相对解析，又保持"清单驱动、不硬编码资源名"。
#[tauri::command]
pub(crate) fn read_asset_text(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    path: String,
) -> Result<String, String> {
    let data_dir = mgr.data_dir();

    // 只接受不越界的相对路径。这个命令会把文件内容交给前端，
    // 若放任 `..`，等于给前端开了一条任意文件读取通道。
    if !pull::is_safe_relative(&path) {
        return Err(format!("非法资源路径: {}", path));
    }

    let root = assets_dir(data_dir)?;
    let full = get_asset_path(data_dir, &path)?;

    let meta = std::fs::metadata(&full).map_err(|e| format!("读取资源失败: {}", e))?;
    if meta.len() > MAX_TEXT_ASSET_BYTES {
        return Err(format!("资源过大，拒绝读取: {} 字节", meta.len()));
    }

    // 纵深防御：即使路径校验被绕过，也不允许读到 Assets 之外。
    let root = root.canonicalize().unwrap_or(root);
    let full = full.canonicalize().unwrap_or(full);
    if !full.starts_with(&root) {
        return Err("拒绝读取资源目录之外的文件".to_string());
    }

    std::fs::read_to_string(&full).map_err(|e| format!("资源不是合法 UTF-8 文本: {}", e))
}
