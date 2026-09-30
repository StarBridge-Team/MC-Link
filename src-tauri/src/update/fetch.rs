//! 更新清单的拉取与"是否需要更新"的判断。
//!
//! 清单**不做本地缓存**：更新信息的价值在于"当下是否是最新"，缓存只会让新版本
//! 延迟可见，而清单本身只有几 KB。

use std::path::Path;
use std::time::Duration;

use crate::asset_server::{assets_server_url, join};
use crate::build_channel::BuildChannel;
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

/// 检查更新：判定渠道 → 拉清单 → 比版本 → 按平台与安装形态选出资产。
///
/// # 渠道闸门
///
/// 只有**官方发布构建**允许自动更新（见 [`crate::build_channel`]）：
///
/// - **开发构建**：直接返回"无可用更新"且**不发起任何网络请求**。它随时在变，
///   "是否是最新版本"对它没有意义，跳过请求也免得开发者每次打开关于页都打一次网络。
///   调试更新流程本身时可用 `MC_LINK_UPDATE_OVERRIDE=1` 强制开启。
/// - **自行构建**：照常检查并告知"官方已发布新版本"，但 `asset` 恒为 `None`，
///   只给手动下载链接——绝不静默替换别人的构建成果。
pub(crate) async fn check_update(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<CheckUpdateResult, String> {
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let channel = crate::build_channel::channel();
    let allowed = crate::build_channel::update_allowed();

    let platform = crate::assets::adapter::current_platform();
    let mode = install_mode();

    if channel == BuildChannel::Dev && !allowed {
        return Ok(CheckUpdateResult {
            has_update: false,
            current_version,
            install_mode: mode.as_str().to_string(),
            platform,
            build_channel: channel.as_str().to_string(),
            update_allowed: false,
            latest: None,
        });
    }

    let manifest = fetch_manifest(data_dir, client).await?;
    let has_update = version_greater(&manifest.version, &current_version);

    // 渠道不允许自更新时，连同"平台是否支持"一起收敛为 false：
    // 选不到资产，界面就只能走手动下载这条路
    let auto_supported = is_auto_install_supported() && allowed;

    Ok(CheckUpdateResult {
        has_update,
        current_version,
        install_mode: mode.as_str().to_string(),
        platform: platform.clone(),
        build_channel: channel.as_str().to_string(),
        update_allowed: allowed,
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
