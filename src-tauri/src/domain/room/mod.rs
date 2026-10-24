//! 联机会话：模型与多会话注册表。
//!
//! 编排（调插件、串检测/适配/耦合）在 `mgr::room`；这里只管**状态**。
//! 这样划分的原因：状态机要能被单测覆盖，而插件调用必须起网关。

pub mod model;
pub mod registry;

pub use model::{Phase, Role, RoomMember, RoomSession, RoomSnapshot};
pub use registry::RoomRegistry;
