//! 事件契约：主题常量 + 发送端口。
//!
//! # 为什么要一个 trait
//!
//! `domain/` 不允许 `use tauri::*`——否则房间状态机只能靠起一个真实 WebView 来测。
//! 这里把"发事件"抽象成 `EventSink`，`bridge/events.rs` 用 `AppHandle` 实现它，
//! 单测里换成 [`NoopSink`] 即可。
//!
//! # 命名与负载约定
//!
//! 主题统一 `mclink://<域>/<动作>`。房间相关负载**必须**带 `sessionId` 与 `rev`：
//! 没有 `sessionId` 无法路由到具体窗口，没有 `rev` 无法去重与判序。
//! 历史事件名（`connect-event` 等）没有这两个字段，正是多房间做不到的原因。

use serde_json::Value;

/// 事件主题常量。新增主题先加在这里，不要在调用点写裸字符串。
pub mod topic {
    /// 会话全量快照：`{ sessionId, rev, snapshot }`。不可丢，窗口挂载时用 `room_snapshot` 补拉。
    pub const ROOM_STATE: &str = "mclink://room/state";
    /// 会话过程提示：`{ sessionId, seq, stage, message }`。可丢，新事件覆盖旧提示。
    pub const ROOM_PROGRESS: &str = "mclink://room/progress";
    /// 会话日志行：`{ sessionId, seq, line }`。可丢，`seq` 断档即清空重拉。
    pub const ROOM_LOG: &str = "mclink://room/log";
    /// 设置变更：`{ rev, patch }`。
    pub const APP_SETTINGS: &str = "mclink://app/settings";
    /// 窗口定向意图：`{ action }`。只发给目标窗口，不广播。
    pub const WINDOW: &str = "mclink://window";
}

/// 发事件的唯一出口。
pub trait EventSink: Send + Sync {
    /// 广播给所有窗口。
    fn emit(&self, topic: &str, payload: Value);
    /// 只发给指定窗口（窗口 label）。
    fn emit_to(&self, target: &str, topic: &str, payload: Value);
}

/// 不发任何事件的实现，供单测与无头环境使用。
pub struct NoopSink;

impl EventSink for NoopSink {
    fn emit(&self, _topic: &str, _payload: Value) {}
    fn emit_to(&self, _target: &str, _topic: &str, _payload: Value) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 记录型 sink：断言"发了什么"。
    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<(String, Value)>>,
    }

    impl EventSink for Recorder {
        fn emit(&self, topic: &str, payload: Value) {
            self.events
                .lock()
                .unwrap()
                .push((topic.to_string(), payload));
        }
        fn emit_to(&self, target: &str, topic: &str, payload: Value) {
            self.events
                .lock()
                .unwrap()
                .push((format!("{}@{}", topic, target), payload));
        }
    }

    #[test]
    fn recorder_captures_topic_and_target() {
        let sink = Recorder::default();
        sink.emit(topic::ROOM_STATE, serde_json::json!({ "rev": 1 }));
        sink.emit_to("room-1", topic::WINDOW, serde_json::json!({}));
        let events = sink.events.lock().unwrap();
        assert_eq!(events[0].0, "mclink://room/state");
        assert_eq!(events[1].0, "mclink://window@room-1");
    }
}
