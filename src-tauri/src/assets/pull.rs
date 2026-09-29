use std::path::{Path, PathBuf};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use crate::cache::{cache_path, is_cached};
use crate::datadir::assets_dir;
use crate::asset_server::{assets_server_url, join};
use super::{ensure_asset_cache_dir, get_asset_path};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(10);
const LOCAL_MANIFEST_FILENAME: &str = "manifest.json";

/// 资产清单：服务器与客户端共享同一结构，version 不一致即视为需要重新下载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AssetsManifest {
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub assets: Vec<AssetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AssetEntry {
    pub path: String,
}

/// sync_assets 的结果：三个 ready 字段对应前端 PrepareAppData。
#[derive(Debug, Clone, Default)]
pub(crate) struct SyncOutcome {
    pub bi_ready: bool,
    pub fonts_ready: bool,
    pub icon_ready: bool,
}

fn local_manifest_path(data_dir: &Path) -> Result<PathBuf, String> {
    let dir = assets_dir(data_dir)?;
    Ok(cache_path(&dir, LOCAL_MANIFEST_FILENAME))
}

fn read_local_manifest(data_dir: &Path) -> Option<AssetsManifest> {
    let path = local_manifest_path(data_dir).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_local_manifest(data_dir: &Path, manifest: &AssetsManifest) -> Result<(), String> {
    let path = local_manifest_path(data_dir)?;
    let text = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("序列化 manifest 失败: {}", e))?;
    std::fs::write(&path, text)
        .map_err(|e| format!("写入本地 manifest 失败: {}", e))?;
    Ok(())
}

async fn fetch_remote_manifest(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<AssetsManifest, String> {
    let base = assets_server_url(data_dir);
    let url = join(&base, "manifest.json");
    let text = client
        .get(&url)
        .timeout(MANIFEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("拉取远程 manifest 失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取远程 manifest 失败: {}", e))?;
    serde_json::from_str(&text)
        .map_err(|e| format!("解析远程 manifest 失败: {}", e))
}

/// 同步资产：版本一致且文件齐全则跳过；不一致则清理 Assets/ 并重新下载全部资源。
///
/// 返回 SyncOutcome 指示每个资源是否就绪，前端据此判断是否可加载 CSS/图标。
pub(crate) async fn sync_assets(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<SyncOutcome, String> {
    let remote = fetch_remote_manifest(data_dir, client).await?;
    let local_version = read_local_manifest(data_dir).map(|m| m.version);
    let need_redownload = local_version.as_deref() != Some(remote.version.as_str());
    eprintln!("[sync_assets] remote_version={} local_version={:?} need_redownload={}", remote.version, local_version, need_redownload);

    if need_redownload {
        // 版本不匹配：先清理 Assets/ 下所有旧文件
        let assets = assets_dir(data_dir)?;
        crate::cache::clear_cache(&assets)?;

        // 重新下载全部资源（各下载函数内部已确保目录存在）
        let bi = match download_bootstrap_icons(data_dir, client).await {
            Ok(_) => true,
            Err(e) => { eprintln!("[sync_assets] bootstrap-icons 下载失败: {}", e); false }
        };
        let fonts = match download_google_fonts(data_dir, client).await {
            Ok(_) => true,
            Err(e) => { eprintln!("[sync_assets] poppins 字体下载失败: {}", e); false }
        };
        let icon = match download_app_icon(data_dir, client).await {
            Ok(_) => true,
            Err(e) => { eprintln!("[sync_assets] 图标下载失败: {}", e); false }
        };

        // 仅在全部成功时写入 manifest，避免下次启动因版本一致而跳过缺失文件
        if bi && fonts && icon {
            write_local_manifest(data_dir, &remote)?;
        }

        return Ok(SyncOutcome { bi_ready: bi, fonts_ready: fonts, icon_ready: icon });
    }

    // 版本一致：仍校验关键文件是否齐全（防本地被误删）
    let bi_ready = get_asset_path(data_dir, "bootstrap-icons/bootstrap-icons.css").is_ok();
    let fonts_ready = get_asset_path(data_dir, "fonts/poppins.css").is_ok();
    let icon_ready = get_asset_path(data_dir, "icons/mc-link-icon.png").is_ok();

    // 任一关键文件缺失：强制重新下载
    if !bi_ready || !fonts_ready || !icon_ready {
        eprintln!("[sync_assets] 版本一致但文件缺失 bi={} fonts={} icon={}，触发重下", bi_ready, fonts_ready, icon_ready);
        let assets = assets_dir(data_dir)?;
        crate::cache::clear_cache(&assets)?;

        let bi = match download_bootstrap_icons(data_dir, client).await {
            Ok(_) => true,
            Err(e) => { eprintln!("[sync_assets] bootstrap-icons 下载失败: {}", e); false }
        };
        let fonts = match download_google_fonts(data_dir, client).await {
            Ok(_) => true,
            Err(e) => { eprintln!("[sync_assets] poppins 字体下载失败: {}", e); false }
        };
        let icon = match download_app_icon(data_dir, client).await {
            Ok(_) => true,
            Err(e) => { eprintln!("[sync_assets] 图标下载失败: {}", e); false }
        };

        if bi && fonts && icon {
            write_local_manifest(data_dir, &remote)?;
        }
        return Ok(SyncOutcome { bi_ready: bi, fonts_ready: fonts, icon_ready: icon });
    }

    Ok(SyncOutcome { bi_ready, fonts_ready, icon_ready })
}

/// 下载 Bootstrap Icons CSS 与字体到 Assets/bootstrap-icons/，已存在则跳过。
pub(crate) async fn download_bootstrap_icons(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<(), String> {
    let assets = assets_dir(data_dir)?;
    let bi_dir = cache_path(&assets, "bootstrap-icons");
    ensure_asset_cache_dir(data_dir, "bootstrap-icons")?;

    let css_path = cache_path(&bi_dir, "bootstrap-icons.css");
    if is_cached(&css_path, None) {
        return Ok(());
    }

    let base = assets_server_url(data_dir);
    let css_url = join(&base, "bootstrap-icons/bootstrap-icons.css");
    let css = client
        .get(&css_url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("下载 Bootstrap Icons CSS 失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取 Bootstrap Icons CSS 失败: {}", e))?;

    let font_filename = "bootstrap-icons.woff2";
    let font_path = cache_path(&bi_dir, font_filename);
    if !is_cached(&font_path, None) {
        let font_url = join(&base, "bootstrap-icons/bootstrap-icons.woff2");
        let font_bytes = client
            .get(&font_url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|e| format!("下载 Bootstrap Icons 字体失败: {}", e))?
            .bytes()
            .await
            .map_err(|e| format!("读取 Bootstrap Icons 字体失败: {}", e))?;
        std::fs::write(&font_path, &font_bytes)
            .map_err(|e| format!("保存 Bootstrap Icons 字体失败: {}", e))?;
    }

    std::fs::write(&css_path, &css)
        .map_err(|e| format!("保存 Bootstrap Icons CSS 失败: {}", e))?;

    Ok(())
}

/// 下载 Poppins 字体 CSS 与字体文件到 Assets/fonts/，已存在则跳过。
pub(crate) async fn download_google_fonts(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<(), String> {
    let fonts_dir = ensure_asset_cache_dir(data_dir, "fonts")?;

    let css_path = cache_path(&fonts_dir, "poppins.css");
    if is_cached(&css_path, None) {
        return Ok(());
    }

    let base = assets_server_url(data_dir);
    let css_url = join(&base, "fonts/poppins.css");
    let css = client
        .get(&css_url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("下载 Poppins CSS 失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取 Poppins CSS 失败: {}", e))?;

    // 下载 CSS 中引用的所有 url(...) 字体文件，并重写为相对路径
    let mut rewritten = css.clone();
    let mut index = 0;
    let mut counter = 0;

    while let Some(pos) = css[index..].find("url(") {
        let start = index + pos + "url(".len();
        let end = css[start..].find(')').ok_or("解析 Poppins 字体 URL 失败")?;
        let raw = &css[start..start + end];
        let url = raw.trim_matches('\'').trim_matches('"');

        if url.starts_with("http") {
            counter += 1;
            let ext = if url.contains(".woff2") {
                "woff2"
            } else if url.contains(".woff") {
                "woff"
            } else {
                "ttf"
            };
            let filename = format!("poppins-{}.{}", counter, ext);

            let font_bytes = client
                .get(url)
                .timeout(DOWNLOAD_TIMEOUT)
                .send()
                .await
                .map_err(|e| format!("下载 Poppins 字体失败 {}: {}", url, e))?
                .bytes()
                .await
                .map_err(|e| format!("读取 Poppins 字体失败 {}: {}", url, e))?;

            std::fs::write(cache_path(&fonts_dir, &filename), &font_bytes)
                .map_err(|e| format!("保存 Poppins 字体失败: {}", e))?;

            rewritten = rewritten.replace(url, &format!("./{filename}"));
        }

        index = start + end + 1;
    }

    std::fs::write(&css_path, rewritten)
        .map_err(|e| format!("保存 Poppins CSS 失败: {}", e))?;

    Ok(())
}

/// 下载 MC Link 图标到 Assets/icons/，已存在则跳过。
pub(crate) async fn download_app_icon(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<(), String> {
    let icons_dir = ensure_asset_cache_dir(data_dir, "icons")?;
    let icon_path = cache_path(&icons_dir, "mc-link-icon.png");
    if is_cached(&icon_path, None) {
        return Ok(());
    }

    let base = assets_server_url(data_dir);
    let url = join(&base, "icons/mc-link-icon.png");
    let bytes = client
        .get(&url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("下载 MC Link 图标失败: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("读取 MC Link 图标失败: {}", e))?;

    std::fs::write(&icon_path, &bytes)
        .map_err(|e| format!("保存 MC Link 图标失败: {}", e))?;

    Ok(())
}
