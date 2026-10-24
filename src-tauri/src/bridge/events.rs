//! [`EventSink`] 的 Tauri 实现。

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::core::events::EventSink;

/// 把事件投递给 Tauri 事件系统。
///
/// 失败（窗口已销毁、事件系统未就绪）一律吞掉并留痕：事件是**通知**，
/// 权威状态在 Rust 侧，收不到的一方可以靠 `room_snapshot` 补拉。
/// 若在这里向上抛错，会让"关窗口"这类正常操作变成命令失败。
pub struct TauriSink {
    app: AppHandle,
}

impl TauriSink {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self { app })
    }
}

impl EventSink for TauriSink {
    fn emit(&self, topic: &str, payload: Value) {
        if let Err(e) = self.app.emit(topic, payload) {
            eprintln!("[事件] 广播失败（{}）: {}", topic, e);
        }
    }

    fn emit_to(&self, target: &str, topic: &str, payload: Value) {
        if let Err(e) = self.app.emit_to(target, topic, payload) {
            eprintln!("[事件] 定向发送失败（{} → {}）: {}", topic, target, e);
        }
    }
}
