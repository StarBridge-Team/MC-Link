use std::path::Path;
use std::time::Duration;
use crate::cache::{cache_path, ensure_cache_dir, is_cached};
use crate::asset_server::{assets_server_url, join};
use super::{PageManifest, PAGE_CACHE_TTL, page_cache_dir, read_cached_manifest, sanitize_name};

const PAGE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

/// 获取页面清单，优先使用本地缓存，过期后从远程拉取。
pub async fn fetch_page_manifest(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<PageManifest, String> {
    let cache_dir = page_cache_dir(data_dir);
    ensure_cache_dir(&cache_dir)?;
    let cache_file = cache_path(&cache_dir, "manifest.json");

    let url = join(&assets_server_url(data_dir), "pages/manifest.json");

    if is_cached(&cache_file, Some(PAGE_CACHE_TTL)) {
        return read_cached_manifest(data_dir);
    }

    let text = client
        .get(&url)
        .timeout(PAGE_DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("下载页面清单失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取页面清单失败: {}", e))?;

    let manifest: PageManifest = serde_json::from_str(&text)
        .map_err(|e| format!("解析页面清单失败: {}", e))?;

    std::fs::write(&cache_file, &text)
        .map_err(|e| format!("保存页面清单缓存失败: {}", e))?;

    Ok(manifest)
}

/// 获取单个页面内容，优先读取本地缓存。
pub async fn fetch_page_content(
    data_dir: &Path,
    name: &str,
    client: &reqwest::Client,
) -> Result<String, String> {
    let cache_dir = page_cache_dir(data_dir);
    ensure_cache_dir(&cache_dir)?;
    let safe_name = sanitize_name(name);
    let cache_file = cache_path(&cache_dir, &format!("{}.html", safe_name));

    let manifest = fetch_page_manifest(data_dir, client).await?;
    let entry = manifest
        .pages
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("页面 '{}' 不在清单中", name))?;

    if is_cached(&cache_file, Some(PAGE_CACHE_TTL)) {
        return std::fs::read_to_string(&cache_file)
            .map_err(|e| format!("读取页面缓存失败: {}", e));
    }

    // 路径优先级：
    // 1. manifest.base_url 显式提供 → 直接拼 base_url/path（路径自带前缀）
    // 2. 否则 → 走资产服务器 /pages/<path>
    let path = entry.path.trim_start_matches('/');
    let url = match manifest.base_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(base) => join(base, path),
        None => join(&assets_server_url(data_dir), &format!("pages/{}", path)),
    };

    let text = client
        .get(&url)
        .timeout(PAGE_DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("下载页面 '{}' 失败: {}", name, e))?
        .text()
        .await
        .map_err(|e| format!("读取页面 '{}' 失败: {}", name, e))?;

    std::fs::write(&cache_file, &text)
        .map_err(|e| format!("保存页面缓存失败: {}", e))?;

    Ok(text)
}
