//! 领域层与 Tauri 之间的桥。
//!
//! 只做一件事：把 [`crate::core::events::EventSink`] 落到真实的 `AppHandle` 上。
//! 所有 emit 都必须经过这里，调用点不得直接 `app.emit(...)`——
//! 那样会让"事件主题写错"变成运行时静默失败（前端收不到，后端不报错）。

pub mod events;
