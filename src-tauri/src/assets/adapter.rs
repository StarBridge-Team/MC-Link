//! 适配器安装包的获取与完整性校验。
//!
//! # 为什么需要这个模块
//!
//! 适配器包是**第三方可执行文件**，安装后会被直接拉起。此前客户端从上游地址
//! 下载后未经任何校验就解压并启动，等于把"发布资产被替换"直接变成任意代码执行。
//!
//! # 信任模型
//!
//! 校验清单由**我们自己的资源服务器**提供（`<assets_server>/adapter/manifest.json`），
//! 包本身仍从清单给出的地址下载（当前为上游 gitee 发布页，可由清单追加镜像）。
//! 因此：
//! - 清单是信任锚点，**拉不到清单即拒绝安装**（fail-closed），不会退回无校验下载；
//! - 升级适配器版本只需更新资源服务器上的清单，不必改客户端代码；
//! - 校验不通过的文件会被删除，绝不进入解压与启动流程。
//!
//! 若将来适配器改为随应用内嵌发布，应在 [`fetch_manifest`] 中增加"本地常量清单"分支
//! （当前没有内嵌包，故不实现该分支，避免留下永不执行的分支代码）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::asset_server::{assets_server_url, join};

/// 清单拉取超时。
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(10);

/// 资源服务器上的适配器清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdapterManifest {
    pub version: String,
    #[serde(default)]
    pub adapters: Vec<AdapterEntry>,
}

/// 单个平台对应的适配器包。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdapterEntry {
    /// 平台标识，形如 `windows-x86_64`（与 [`current_platform`] 对应）。
    pub platform: String,
    /// 包的版本号（来自上游发布）。
    pub version: String,
    /// 包文件名。
    pub file: String,
    /// 小写十六进制 SHA256。
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
    /// 候选下载地址，按顺序尝试。
    #[serde(default)]
    pub urls: Vec<String>,
}

/// 当前平台标识，与清单中的 `platform` 字段对应。
pub(crate) fn current_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn manifest_url(data_dir: &Path) -> String {
    join(&assets_server_url(data_dir), "adapter/manifest.json")
}

/// 从资源服务器拉取适配器清单。
///
/// 拉取或解析失败一律返回错误：没有清单就不允许安装任何适配器包。
pub(crate) async fn fetch_manifest(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<AdapterManifest, String> {
    let url = manifest_url(data_dir);
    let text = client
        .get(&url)
        .timeout(MANIFEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| {
            format!(
                "无法从资源服务器获取适配器校验清单（{}）: {}。\
                 为避免安装未经验证的二进制，安装已中止；\
                 请确认资源服务器已发布 /adapter/manifest.json",
                url, e
            )
        })?
        .text()
        .await
        .map_err(|e| format!("读取适配器校验清单失败: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("解析适配器校验清单失败: {}", e))
}

/// 选出当前平台对应的条目。
pub(crate) fn pick_entry<'a>(
    manifest: &'a AdapterManifest,
    platform: &str,
) -> Result<&'a AdapterEntry, String> {
    manifest.adapters.iter().find(|e| e.platform == platform).ok_or_else(|| {
        let available = if manifest.adapters.is_empty() {
            "无".to_string()
        } else {
            manifest
                .adapters
                .iter()
                .map(|e| e.platform.clone())
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "资源服务器未提供当前平台（{}）的适配器包，清单中的平台：{}",
            platform, available
        )
    })
}

/// 下载适配器包并校验 SHA256，返回落地的压缩包路径。
///
/// 校验失败会删除已下载文件并返回错误，绝不把未校验的文件交给解压/启动流程。
///
/// 下载与校验的具体链路在 [`crate::downloader::verified`]——**与更新包共用同一实现**，
/// 避免"适配器校验了、更新包忘了校验"这类两边漂移。本函数只做清单字段的映射。
pub(crate) async fn download_verified_package(
    dir: &Path,
    client: &reqwest::Client,
    entry: &AdapterEntry,
    mut on_progress: impl FnMut(u8),
) -> Result<PathBuf, String> {
    let remote = crate::downloader::verified::RemoteFile {
        file: &entry.file,
        urls: &entry.urls,
        sha256: &entry.sha256,
        size: entry.size,
    };
    crate::downloader::verified::download_verified(dir, client, &remote, |done, total| {
        if total > 0 {
            on_progress(((done as f64 / total as f64) * 100.0).min(100.0) as u8);
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(entries: Vec<AdapterEntry>) -> AdapterManifest {
        AdapterManifest { version: "1".to_string(), adapters: entries }
    }

    fn entry(platform: &str, sha256: &str) -> AdapterEntry {
        AdapterEntry {
            platform: platform.to_string(),
            version: "0.4.2".to_string(),
            file: "pkg.tar.gz".to_string(),
            sha256: sha256.to_string(),
            size: None,
            urls: vec!["https://example.invalid/pkg.tar.gz".to_string()],
        }
    }

    #[test]
    fn platform_matches_current_target() {
        // 形如 windows-x86_64 / linux-x86_64 / macos-aarch64
        let p = current_platform();
        assert!(p.contains('-'), "平台标识应形如 <os>-<arch>，实际: {}", p);
    }

    #[test]
    fn pick_entry_returns_matching_platform() {
        let m = manifest_with(vec![entry("windows-x86_64", &"a".repeat(64))]);
        assert!(pick_entry(&m, "windows-x86_64").is_ok());
    }

    #[test]
    fn pick_entry_reports_missing_platform_with_available_list() {
        let m = manifest_with(vec![entry("linux-x86_64", &"a".repeat(64))]);
        let err = pick_entry(&m, "windows-x86_64").unwrap_err();
        assert!(err.contains("windows-x86_64"), "错误应包含请求的平台: {}", err);
        assert!(err.contains("linux-x86_64"), "错误应列出可用平台: {}", err);
    }

    #[tokio::test]
    async fn rejects_manifest_with_illegal_sha256() {
        let dir = std::env::temp_dir().join("mc-link-adapter-test");
        let _ = std::fs::create_dir_all(&dir);
        let client = reqwest::Client::new();
        let bad = entry("windows-x86_64", "not-a-hash");
        let err = download_verified_package(&dir, &client, &bad, |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("sha256"), "应在下载前拒绝非法哈希: {}", err);
    }

    #[tokio::test]
    async fn rejects_manifest_without_urls() {
        let dir = std::env::temp_dir().join("mc-link-adapter-test");
        let _ = std::fs::create_dir_all(&dir);
        let client = reqwest::Client::new();
        let mut e = entry("windows-x86_64", &"a".repeat(64));
        e.urls.clear();
        let err = download_verified_package(&dir, &client, &e, |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("下载地址"), "应在无地址时报错: {}", err);
    }
}
