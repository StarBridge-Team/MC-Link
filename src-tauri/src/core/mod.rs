//! 基础设施层：错误、事件契约、ID 类型。
//!
//! 这一层**不含业务**，也不依赖 `tauri`。`domain/` 与 `mgr/` 都建在它上面，
//! 因此它可以被纯单测覆盖，不需要起 WebView。
//!
//! 依赖方向：`commands → mgr → domain → core`。反向引用一律视为设计错误。

pub mod error;
pub mod events;
pub mod ids;
pub mod platform;
