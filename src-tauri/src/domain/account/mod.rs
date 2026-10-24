//! 离线账号。
//!
//! 现阶段**只有玩家名**，但身份标识是一个按 Minecraft 离线模式算法生成的 UUID：
//! 同一个名字永远得到同一个 UUID，改名即换身份。这样后续接入 IdP 时，
//! 服务端可以先用 UUID 认人，再把在线账号绑定到它上面，不必迁移历史数据。

pub mod model;
pub mod service;
pub mod uuid;

pub use model::OfflineAccount;
pub use service::AccountService;
