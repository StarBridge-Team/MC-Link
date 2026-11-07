//! 官方内置插件。
//!
//! 内置插件的清单、权限与实现都来自核心代码，因此信任度为
//! [`crate::plugin::permission::TrustLevel::Official`]；但它们**不走特例**：
//! 仍然以标准能力方法（`adapter.*` / `detector.*` / `coupler.*`）被调度，
//! 区别只是传输从回环 WebSocket 换成进程内直调。

/// 房间码的字符集与进制编解码（两个内置适配器共用）。
pub mod code;
/// 局域网 MOTD 广播的共享契约（耦合器发、扫描器认）。
pub mod lan;
/// 内置适配器：通用 EasyTier 房间。
pub mod easytier;
/// 内置适配器：陶瓦联机协议。
pub mod terracotta;
/// 两个内置适配器共用的小工具。
mod support;

pub mod minecraft_coupler;
pub mod minecraft_scanner;

pub use easytier::{EasyTierProvider, PLUGIN_ID as EASYTIER_PLUGIN_ID};
pub use minecraft_coupler::{MinecraftCouplerProvider, PLUGIN_ID as MINECRAFT_COUPLER_PLUGIN_ID};
pub use minecraft_scanner::{MinecraftScannerProvider, PLUGIN_ID as MINECRAFT_DETECTOR_PLUGIN_ID};
pub use terracotta::{TerracottaProvider, PLUGIN_ID as TERRACOTTA_PLUGIN_ID};
