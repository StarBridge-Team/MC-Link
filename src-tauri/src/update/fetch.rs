//! 更新清单的拉取与"是否需要更新"的判断。
//!
//! 清单**不做本地缓存**：更新信息的价值在于"当下是否是最新"，缓存只会让新版本
//! 延迟可见，而清单本身只有几 KB。

use std::path::Path;
use std::time::Duration;

use crate::asset_server::{assets_server_url, join};
use crate::datadir::install_mode;

use super::install::is_auto_install_supported;
use super::model::{
    build_info, ensure_supported, version_greater, CheckUpdateResult, UpdateManifest,
};

/// 清单拉取超时。
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(15);

/// 资源服务器上的更新清单地址。
fn manifest_url(data_dir: &Path) -> String {
    join(&assets_server_url(data_dir), "update/latest.json")
}

/// 拉取更新清单。
pub(crate) async fn fetch_manifest(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<UpdateManifest, String> {
    let url = manifest_url(data_dir);

    let response = client
        .get(&url)
        .timeout(MANIFEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("无法获取更新清单（{}）: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "更新清单不可用（HTTP {}）：{}。请确认资源服务器已发布 /update/latest.json",
            response.status(),
            url
        ));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("读取更新清单失败: {}", e))?;

    let manifest: UpdateManifest =
        serde_json::from_str(&text).map_err(|e| format!("解析更新清单失败: {}", e))?;

    ensure_supported(&manifest)?;
    Ok(manifest)
}

/// 检查更新：拉清单 → 比版本 → 按当前平台与安装形态选出资产。
pub(crate) async fn check_update(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<CheckUpdateResult, String> {
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let manifest = fetch_manifest(data_dir, client).await?;

    let platform = crate::assets::adapter::current_platform();
    let mode = install_mode();
    let auto_supported = is_auto_install_supported();
    let has_update = version_greater(&manifest.version, &current_version);

    Ok(CheckUpdateResult {
        has_update,
        current_version,
        install_mode: mode.as_str().to_string(),
        platform: platform.clone(),
        latest: if has_update {
            Some(build_info(
                &manifest,
                &platform,
                mode,
                auto_supported,
                &assets_server_url(data_dir),
            ))
        } else {
            None
        },
    })
}
