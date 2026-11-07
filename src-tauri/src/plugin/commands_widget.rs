//! 挂件（widget）相关的 Tauri 命令。
//!
//! 视图是**声明式节点树**（见 `plugin::widget`），前端只渲染白名单类型；
//! 用户的点击通过 `widget_action` 回传给插件，插件可返回新视图。

use std::sync::Arc;

use serde_json::Value;

use crate::plugin::manager::PluginManager;

/// 挂载一个挂件，返回校验过的视图 `{ title, subtitle, nodes }`。
///
/// 失败语义刻意区分：插件未连接 → `unavailable`；插件返回的视图非法 →
/// `bad_request`。界面据此提示"插件没起来"还是"插件写错了"，而不是笼统的"加载失败"。
#[tauri::command]
pub(crate) async fn widget_mount(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    widget_id: String,
) -> Result<Value, String> {
    manager
        .widget_mount(plugin_id.trim(), widget_id.trim())
        .await
        .map_err(|e| format!("{}（{}）", e.message, e.code))
}

/// 把用户在挂件上的动作回传，返回新视图；`null` 表示插件决定不刷新。
#[tauri::command]
pub(crate) async fn widget_action(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    widget_id: String,
    action: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    let view = manager
        .widget_action(
            plugin_id.trim(),
            widget_id.trim(),
            &action,
            payload.unwrap_or(Value::Null),
        )
        .await
        .map_err(|e| format!("{}（{}）", e.message, e.code))?;
    Ok(view.unwrap_or(Value::Null))
}

/// 卸载挂件。插件侧的清理失败不影响卸载结果。
#[tauri::command]
pub(crate) async fn widget_unmount(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    widget_id: String,
) -> Result<(), String> {
    manager
        .widget_unmount(plugin_id.trim(), widget_id.trim())
        .await
        .map_err(|e| format!("{}（{}）", e.message, e.code))
}
