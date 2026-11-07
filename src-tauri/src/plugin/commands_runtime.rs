//! 共享运行时（Bun）相关的 Tauri 命令。
//!
//! 单独成文件而不是塞进 `commands.rs`：那边是"插件清单/路由/授权"的门面，
//! 与"下载一个 90 MB 的解释器"是两件不同的事，混在一起会同时撑破行数上限
//! 与职责边界。

use std::sync::Arc;

use crate::plugin::manager::PluginManager;
use crate::plugin::runtime::RuntimeStatus;

/// 共享 Node 运行时（Bun）的当前状态。
///
/// 界面用它决定"Node 插件能不能跑"以及要不要展示"安装运行时"按钮。
/// 不发网络请求、不触发安装；锁定版本也在这个返回值里（`pinnedVersion`）。
#[tauri::command]
pub(crate) fn plugin_runtime_status(
    manager: tauri::State<'_, Arc<PluginManager>>,
) -> RuntimeStatus {
    crate::plugin::runtime::status(manager.data_dir())
}

/// 安装/修复共享运行时，返回实际安装的版本。
///
/// `async`：内部要发网络请求（下载 90 MB 级产物），同步命令会占住主线程。
#[tauri::command]
pub(crate) async fn plugin_runtime_install(
    manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<String, String> {
    crate::plugin::runtime::ensure(manager.data_dir()).await
}
