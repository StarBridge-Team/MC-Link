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
use std::sync::{Arc, OnceLock, RwLock};

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
    /// 跨窗口的用户意图：`{ action, process? }`。
    ///
    /// 托盘面板要"打开主窗口并落到联机页"，但它不能直接驱动主窗口的 store，
    /// 于是发一条意图，由主窗口自己决定怎么消费。
    pub const APP_INTENT: &str = "mclink://app/intent";
    /// 窗口定向意图：`{ action }`。只发给目标窗口，不广播。
    pub const WINDOW: &str = "mclink://window";
}

/// 事件投递目标（窗口 label）。
///
/// 房间事件按 `sessionId` 路由：**只有该会话自己的房间窗口**需要它的状态，
/// 而**主窗口**与**托盘面板**要的是房间**列表**，所以它们也要收。
/// 其它房间窗口不收——广播会让每个房间窗口都为与自己无关的会话重渲染一遍。
///
/// # 常量必须与窗口创建处一致
///
/// - `MAIN`：主窗口（`tauri.conf.json` 未写 label，Tauri 默认即 `main`）；
/// - `TRAY`：托盘面板（对应 `crate::window::widget::TRAY_LABEL`）；
/// - 房间窗口：见 [`crate::core::ids::SessionId::window_label`]。
pub mod target {
    /// 主窗口 label。
    pub const MAIN: &str = "main";
    /// 托盘面板 label。与 `crate::window::widget::TRAY_LABEL` 必须一致。
    pub const TRAY: &str = "widget-tray";
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

/// 进程级事件出口。
///
/// # 为什么需要它
///
/// 个性化设置的写盘路径（`config::write::write_personalization`）只拿到数据目录，
/// 拿不到 `AppHandle`——但保存成功后**必须**让其它窗口知道设置变了。
/// 于是把 sink 登记在这里：`bridge::events::TauriSink` 创建时登记一次，
/// 需要发事件的模块按需取用。
///
/// 单测里不登记 → 取不到 sink → 不发事件。这不影响正确性：
/// 事件只是**通知**，权威状态在 Rust，窗口挂载时本来就会自己拉一次。
static GLOBAL_SINK: OnceLock<RwLock<Option<Arc<dyn EventSink>>>> = OnceLock::new();

fn global_sink_slot() -> &'static RwLock<Option<Arc<dyn EventSink>>> {
    GLOBAL_SINK.get_or_init(|| RwLock::new(None))
}

/// 登记进程级事件出口。
///
/// 后登记的覆盖先登记的：应用只应该有一个 sink。
pub fn register_global_sink(sink: Arc<dyn EventSink>) {
    let mut slot = global_sink_slot()
        .write()
        .unwrap_or_else(|e| e.into_inner());
    *slot = Some(sink);
}

/// 取进程级事件出口。没登记过时返回 `None`（调用方跳过发事件即可）。
pub fn global_sink() -> Option<Arc<dyn EventSink>> {
    global_sink_slot()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
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
