//! 插件市场（B7）：清单拉取 → 下载校验 → 解压安装。
//!
//! # 为什么包格式是 `.tar.gz`
//!
//! 仓库已有 `tar` + `flate2`（适配器安装用的就是这一套），不引入 zip 依赖；
//! 解压前的"展开体积 / 条目数"防护也沿用适配器安装的口径，避免解压炸弹。
//!
//! # fail-closed
//!
//! 清单拉不到就报错，**不返回空清单**——空清单会把网络故障伪装成"市场没有插件"。
//! 没有 `sha256` 的条目一律拒绝：包会落进插件目录并被加载，必须强制校验。

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::asset_server::{assets_server_url, join};
use crate::downloader::verified::{download_verified, RemoteFile};
use crate::plugin::manifest::validate_plugin_id;

/// 清单拉取超时。
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(10);
/// 客户端支持的清单版本上限。
const MANIFEST_VERSION: u32 = 1;

/// 市场中一个插件的条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    /// 下载地址，按顺序尝试；为空时按资源服务器推导 `plugins/<file>`。
    #[serde(default)]
    pub urls: Vec<String>,
    /// **必填**：包会被加载，缺失即拒绝安装。
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
}

/// 市场清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketManifest {
    #[serde(default = "default_manifest_version")]
    pub manifest_version: u32,
    #[serde(default)]
    pub plugins: Vec<MarketEntry>,
}

fn default_manifest_version() -> u32 {
    MANIFEST_VERSION
}

fn manifest_url(data_dir: &Path) -> String {
    join(&assets_server_url(data_dir), "plugins/market.json")
}

/// 从资源服务器拉取市场清单。
pub(crate) async fn fetch_market(data_dir: &Path) -> Result<MarketManifest, String> {
    let url = manifest_url(data_dir);
    let client = reqwest::Client::builder()
        .timeout(MANIFEST_TIMEOUT)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("拉取插件市场清单失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("插件市场清单返回 {}（{url}）", resp.status()));
    }
    let manifest: MarketManifest = resp
        .json()
        .await
        .map_err(|e| format!("解析插件市场清单失败: {e}"))?;
    if manifest.manifest_version > MANIFEST_VERSION {
        return Err(format!(
            "插件市场清单版本 {} 高于客户端支持的 {}，请更新客户端",
            manifest.manifest_version, MANIFEST_VERSION
        ));
    }
    Ok(manifest)
}

/// 安装一个市场插件：下载 → SHA256 校验 → 解压到 `Plugins/<id>/`。
///
/// 返回插件 id；调用方随后 `PluginManager::reload()` 让注册表感知新目录。
pub(crate) async fn install(data_dir: &Path, entry: &MarketEntry) -> Result<String, String> {
    validate_plugin_id(&entry.id)?;
    if entry.sha256.trim().is_empty() {
        return Err("市场清单缺少 sha256，拒绝安装".to_string());
    }

    let plugins_dir = data_dir.join("Plugins");
    let cache_dir = plugins_dir.join(".market-cache");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("创建市场缓存目录失败: {e}"))?;

    let file = format!("{}-{}.tar.gz", entry.id, sanitize(&entry.version));
    let urls = if entry.urls.is_empty() {
        vec![join(
            &assets_server_url(data_dir),
            &format!("plugins/{file}"),
        )]
    } else {
        entry.urls.clone()
    };

    let remote = RemoteFile {
        file: &file,
        urls: &urls,
        sha256: &entry.sha256,
        size: entry.size,
    };
    let client = reqwest::Client::new();
    let archive_path = download_verified(&cache_dir, &client, &remote, |_, _| {}).await?;

    let target = plugins_dir.join(&entry.id);
    // 解包与形态校验统一走 `package`，避免"网络装的走一套、本地拖的走另一套"
    crate::plugin::package::extract_tar_gz(&archive_path, &target)?;
    let _ = std::fs::remove_file(&archive_path);
    Ok(entry.id.clone())
}

/// 版本号只用于拼缓存文件名，去掉路径分隔符等不安全字符。
fn sanitize(version: &str) -> String {
    version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}
