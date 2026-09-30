//! OOBE（首次启动引导）与本地化相关的 Tauri 命令。
//!
//! 命令层只做参数转交与结果映射；校验与落盘在 [`crate::setup`]。

use std::sync::Arc;

use crate::mgr::AppMgr;
use crate::setup::SetupState;

/// 读取引导状态：是否已完成 + 当前生效/系统检测到的语言与地区 + 可选值清单。
///
/// 前端**不应硬编码**可选语言与地区，一律以本命令返回的清单为准。
#[tauri::command]
pub(crate) fn get_setup_state_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<SetupState, String> {
    mgr.read().setup_state()
}

/// 完成首次引导：保存语言与地区，并标记为已完成。
///
/// 传了不支持的语言/地区会直接报错（不会写入半个选择）。
#[tauri::command]
pub(crate) fn complete_setup_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    language: String,
    region: String,
) -> Result<SetupState, String> {
    mgr.write().complete_setup(&language, &region)?;
    mgr.read().setup_state()
}

/// 修改语言/地区（引导之后在设置里使用，不会改动"已完成"标记）。
#[tauri::command]
pub(crate) fn update_setup_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    language: String,
    region: String,
) -> Result<SetupState, String> {
    mgr.write().update_setup(&language, &region)?;
    mgr.read().setup_state()
}

/// 重置引导状态，下次启动重新走一遍 OOBE（开发调试 OOBE 界面时常用）。
#[tauri::command]
pub(crate) fn reset_setup_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<SetupState, String> {
    mgr.write().reset_setup()?;
    mgr.read().setup_state()
}
