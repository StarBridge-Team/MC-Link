//! 用 Tauri **官方更新插件**落地更新（安装版与 Linux/macOS）。
//!
//! # 与自研路径的分工
//!
//! | 场景 | 谁来落地 | 为什么 |
//! |---|---|---|
//! | Windows 便携版 | 自研（[`super::install`]，替换 exe） | 便携版没有安装记录，官方插件不覆盖这种形态 |
//! | Windows 安装版 | 官方插件（NSIS，passive 安装后自动重启） | 系统目录与注册表由安装器管理，交给官方实现更可靠 |
//! | Linux | 官方插件（**仅 AppImage** 支持自动安装，deb/rpm 不支持） | 自行替换会破坏包管理器的记录 |
//! | macOS | 官方插件（`.app.tar.gz`） | 需要签名与 Gatekeeper 配合，不宜自行替换 |
//!
//! # 为什么用 `pubkey` 作为开关
//!
//! 官方插件的签名校验**不可关闭**，`plugins.updater.pubkey` 是必填项。
//! 也就是说：没有公钥 → 插件根本无法完成校验 → 这条路径整体不可用。
//! 因此这里以"配置里有没有填公钥"作为启用条件，没填就**不注册插件**，
//! 让调用方安静地回退到自研路径（Windows）或提示手动下载（其他平台）。
//!
//! # 发布侧的前置条件
//!
//! 需要 `TAURI_SIGNING_PRIVATE_KEY`（Tauri CLI 生成的 minisign 私钥）：
//! `scripts/make-update.mjs` 用它给产物签名，并生成插件要读的清单 `update/tauri.json`。
//! 没有私钥时脚本会跳过这一步并打印提示——此时插件路径始终不可用，
//! 但 Windows 安装版仍能通过自研路径正常更新。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

/// 配置里是否填了公钥。
///
/// 存成静态量是因为"是否可用"必须在**没有 `AppHandle` 的地方**也能回答：
/// 选资产（`update::model`）发生在拉清单阶段，那里只拿得到数据目录与 HTTP 客户端；
/// 而这个值在启动时读一次就不会再变（插件注册与否在启动时已经定下）。
static CONFIGURED: AtomicBool = AtomicBool::new(false);

/// 启动时记录一次"配置里是否填了更新公钥"（由 `lib.rs` 在注册插件处调用）。
pub(crate) fn record_configured(configured: bool) {
    CONFIGURED.store(configured, Ordering::Relaxed);
}

/// 官方插件能自动安装的平台。其余平台（如移动端）没有可用的安装流程。
pub(crate) fn platform_supported() -> bool {
    cfg!(any(windows, target_os = "linux", target_os = "macos"))
}

/// 是否应当**优先**走官方插件。
///
/// 便携版必须用自研路径：官方插件只会把 NSIS/AppImage 装回去，
/// 对"exe 同目录即全部"的便携形态无能为力。
pub(crate) fn preferred() -> bool {
    if !platform_supported() {
        return false;
    }
    !(cfg!(windows) && crate::datadir::install_mode() == crate::datadir::InstallMode::Portable)
}

/// 配置里是否填了可用的更新公钥（纯函数，便于单测）。
fn pubkey_value_is_usable(updater_config: Option<&serde_json::Value>) -> bool {
    updater_config
        .and_then(|cfg| cfg.get("pubkey"))
        .and_then(|key| key.as_str())
        .map(|key| {
            let key = key.trim();
            // 占位符也算"没填"：留空或没替换的模板都不该被当成已配置
            !key.is_empty() && !key.contains("REPLACE_ME")
        })
        .unwrap_or(false)
}

/// 插件是否已配置（即 `plugins.updater.pubkey` 已填）。
pub(crate) fn configured(plugins: &tauri::utils::config::PluginConfig) -> bool {
    pubkey_value_is_usable(plugins.0.get("updater"))
}

/// 当前进程能否走官方插件路径：已配置公钥 **且** 平台支持。
///
/// 选资产与决定"能否自动安装"都以此为准。
pub(crate) fn available() -> bool {
    CONFIGURED.load(Ordering::Relaxed) && platform_supported()
}

/// 通过官方插件安装更新。
///
/// 返回被安装的版本号。**Windows 上插件会在安装前让应用自行退出**，
/// 因此调用方拿到返回值后应尽快展示提示；其他平台由调用方负责重启。
pub(crate) async fn install_via_plugin(app: &AppHandle) -> Result<String, String> {
    let updater = app
        .updater()
        .map_err(|e| format!("初始化官方更新器失败: {}", e))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("官方更新器检查失败: {}", e))?
        .ok_or_else(|| "官方更新器未发现可用更新（清单可能缺少签名）".to_string())?;

    let version = update.version.clone();

    // 插件按"每块"回调，这里累加成前端约定的累计字节数
    let downloaded = Arc::new(AtomicU64::new(0));
    let progress = downloaded.clone();
    let handle = app.clone();

    update
        .download_and_install(
            move |chunk_length, content_length| {
                let done = progress.fetch_add(chunk_length as u64, Ordering::Relaxed)
                    + chunk_length as u64;
                let _ = handle.emit(
                    "update-progress",
                    json!({ "downloaded": done, "total": content_length.unwrap_or(0) }),
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("官方更新器下载或安装失败: {}", e))?;

    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_placeholder_pubkey_means_not_configured() {
        assert!(!pubkey_value_is_usable(None));
        assert!(!pubkey_value_is_usable(Some(&json!({}))));
        assert!(!pubkey_value_is_usable(Some(&json!({ "pubkey": "" }))));
        assert!(!pubkey_value_is_usable(Some(&json!({ "pubkey": "   " }))));
        assert!(!pubkey_value_is_usable(Some(&json!({
            "pubkey": "REPLACE_ME_WITH_TAURI_SIGNER_PUBLIC_KEY"
        }))));
        assert!(!pubkey_value_is_usable(Some(&json!({ "pubkey": 123 }))));
    }

    #[test]
    fn real_pubkey_means_configured() {
        assert!(pubkey_value_is_usable(Some(&json!({
            "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6"
        }))));
    }

    #[test]
    fn portable_mode_never_prefers_the_plugin() {
        // 便携版必须走自研替换路径，插件只会把安装包装回去
        if cfg!(windows) && crate::datadir::install_mode() == crate::datadir::InstallMode::Portable {
            assert!(!preferred());
        }
    }
}
