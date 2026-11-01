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
        let sink = Arc::new(Self { app });
        // 登记为**进程级**事件出口：拿不到 `AppHandle` 的地方（配置的写盘路径）
        // 要靠它把"设置变了"通知给其它窗口。
        let global: Arc<dyn EventSink> = sink.clone();
        crate::core::events::register_global_sink(global);
        sink
    }
}

impl EventSink for TauriSink {
    fn emit(&self, topic: &str, payload: Value) {
        if let Err(e) = self.app.emit(topic, payload) {
            eprintln!("[事件] 广播失败（{}）: {}", topic, e);
        }
    }

    fn emit_to(&self, target: &str, topic: &str, payload: Value) {
        // 目标窗口不存在（房间窗口还没开、已被关掉）是**正常情况**，不是错误：
        // Tauri v2 的 `emit_to` 对未知 label 是 no-op（只投递给 label 匹配的监听器），
        // 返回 `Ok(())`。这里仍然留痕，便于排查"事件发出去却没人收"的问题。
        if let Err(e) = self.app.emit_to(target, topic, payload) {
            eprintln!("[事件] 定向发送失败（{} → {}）: {}", topic, target, e);
        }
    }
}
