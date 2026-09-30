//! 官方内置插件。
//!
//! 内置插件的清单、权限与实现都来自核心代码，因此信任度为
//! [`crate::plugin::permission::TrustLevel::Official`]；但它们**不走特例**：
//! 仍然以标准能力方法（`adapter.*` / `detector.*` / `coupler.*`）被调度，
//! 区别只是传输从回环 WebSocket 换成进程内直调。

pub mod terracotta;

pub use terracotta::{TerracottaProvider, PLUGIN_ID as TERRACOTTA_PLUGIN_ID};
