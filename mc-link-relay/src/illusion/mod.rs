//! Illusion — 基于域名的单端口 TCP 映射工具
//!
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion
//! 集成到 mc-link-relay 中，共享同一个 TCP 端口。
//!
//! ## 连接分类（端口共享）
//! 通过 TCP peek 前 2 字节区分：
//! - `[0x49, 0x4C]` ("IL") → Illusion 客户端（Register / ProxyReady）
//! - 否则 → 尝试作为 Illusion 访问者嗅探域名，失败则回退到 MC Link 协议

pub mod types;
pub mod frame;
pub mod sniffer;
pub mod registry;
pub mod bridge;
pub mod client;
pub mod visitor;
