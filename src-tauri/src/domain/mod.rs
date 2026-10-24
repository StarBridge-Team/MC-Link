//! 业务领域层。
//!
//! 这里放**状态机与不变量**，不放 Tauri 调用。当前只有 `room`（多房间会话），
//! 后续 `account` / `settings` / `proxy` 按同一套约定迁入：
//!
//! - 对外只暴露快照（`*Snapshot`），内部字段不外泄；
//! - 状态推进集中在 `registry` 的 `update` 里，`rev` 与状态同锁推进；
//! - 发事件只经 [`crate::core::events::EventSink`]。

pub mod account;
pub mod proxy;
pub mod room;
