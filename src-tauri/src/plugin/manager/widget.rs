//! `widget` 类插件的挂载、更新与动作转发。
//!
//! 三件事：
//!
//! | 动作 | 核心 → 插件 | 插件 → 核心 |
//! |---|---|---|
//! | 挂载 | `widget.mount { widget: { id } }` | 声明式视图（见 [`crate::plugin::widget`]） |
//! | 动作 | `widget.update { widget, action, payload }` | 新的视图，或 `null`（不变） |
//! | 卸载 | `widget.unmount { widget }` | 任意（忽略） |
//!
//! 视图一律经过 [`crate::plugin::widget::validate_view`]：插件交什么都不能让前端
//! 渲染出白名单之外的东西。

use serde_json::{json, Value};

use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::required_for;
use crate::plugin::protocol::{error_code, widget_method, ErrorInfo};
use crate::plugin::widget;

use super::PluginManager;

impl PluginManager {
    /// 挂载一个挂件/页面，返回校验过的视图。
    pub async fn widget_mount(
        &self,
        plugin_id: &str,
        widget_id: &str,
    ) -> Result<Value, ErrorInfo> {
        let raw = self
            .invoke_plugin(
                plugin_id,
                required_for(PluginKind::Widget, widget_method::MOUNT),
                widget_method::MOUNT,
                json!({ "widget": { "id": widget_id } }),
            )
            .await?;
        widget::validate_view(&raw)
            .map_err(|e| ErrorInfo::new(error_code::BAD_REQUEST, format!("挂件视图非法: {e}")))
    }

    /// 把用户在挂件上的动作转发给插件，返回更新后的视图（插件可回 `null` 表示不刷新）。
    pub async fn widget_action(
        &self,
        plugin_id: &str,
        widget_id: &str,
        action: &str,
        payload: Value,
    ) -> Result<Option<Value>, ErrorInfo> {
        let action = action.trim();
        if action.is_empty() || action.len() > widget::MAX_ACTION_CHARS {
            return Err(ErrorInfo::new(
                error_code::BAD_REQUEST,
                "挂件动作名非法".to_string(),
            ));
        }
        let raw = self
            .invoke_plugin(
                plugin_id,
                required_for(PluginKind::Widget, widget_method::UPDATE),
                widget_method::UPDATE,
                json!({
                    "widget": { "id": widget_id },
                    "action": action,
                    "payload": payload,
                }),
            )
            .await?;

        if raw.is_null() {
            return Ok(None);
        }
        let view = widget::validate_view(&raw)
            .map_err(|e| ErrorInfo::new(error_code::BAD_REQUEST, format!("挂件视图非法: {e}")))?;
        Ok(Some(view))
    }

    /// 卸载挂件。插件回什么都忽略——卸载是宿主单方面的决定。
    pub async fn widget_unmount(
        &self,
        plugin_id: &str,
        widget_id: &str,
    ) -> Result<(), ErrorInfo> {
        // 失败不阻断：宿主已经决定移除它了，插件侧的清理失败只会留下内部状态
        let _ = self
            .invoke_plugin(
                plugin_id,
                required_for(PluginKind::Widget, widget_method::UNMOUNT),
                widget_method::UNMOUNT,
                json!({ "widget": { "id": widget_id } }),
            )
            .await;
        Ok(())
    }
}
