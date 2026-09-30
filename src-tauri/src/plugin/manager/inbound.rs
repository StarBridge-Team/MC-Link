//! 会话事件泵：把网关侧上报的 `Inbound` 转成提供者注册/注销与前端事件。
//!
//! 这条通道是**唯一**能把插件标记为"可用"的入口：插件只有在完成握手、核心收到
//! `Inbound::Connected` 之后才会进入 `providers`，因此调度层永远不会拿到一个
//! 尚未通过认证的句柄。

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::plugin::capability::Provider;
use crate::plugin::session::Inbound;

use super::Inner;

/// 消费会话事件，直到所有发送端被丢弃。
pub(super) async fn consume_inbound(inner: Arc<Inner>, mut rx: mpsc::UnboundedReceiver<Inbound>) {
    while let Some(message) = rx.recv().await {
        match message {
            Inbound::Connected(handle) => {
                let plugin_id = handle.plugin_id.clone();
                if let Ok(mut providers) = inner.providers.write() {
                    providers.insert(plugin_id.clone(), Provider::Remote(handle));
                }
                emit(&inner, json!({ "type": "connected", "pluginId": plugin_id }));
            }
            Inbound::Disconnected { plugin_id, reason } => {
                // 只移除远程提供者：内置提供者不经过这条通道。
                if let Ok(mut providers) = inner.providers.write() {
                    if matches!(providers.get(&plugin_id), Some(Provider::Remote(_))) {
                        providers.remove(&plugin_id);
                    }
                }
                emit(
                    &inner,
                    json!({ "type": "disconnected", "pluginId": plugin_id, "reason": reason }),
                );
            }
            Inbound::Event {
                plugin_id,
                topic,
                data,
            } => {
                emit(
                    &inner,
                    json!({
                        "type": "event",
                        "pluginId": plugin_id,
                        "topic": topic,
                        "data": data,
                    }),
                );
            }
        }
    }
}

/// 向前端广播 `plugin-event`（未绑定 AppHandle 时静默丢弃）。
pub(super) fn emit(inner: &Arc<Inner>, payload: Value) {
    let app: Option<AppHandle> = inner.app.read().ok().and_then(|g| g.clone());
    if let Some(app) = app {
        let _ = app.emit("plugin-event", payload);
    }
}
