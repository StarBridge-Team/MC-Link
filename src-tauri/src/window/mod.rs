//! 全局窗口管理器。
//!
//! 约定：
//!
//! - **窗口 label 是窗口身份的唯一权威**。房间窗口 label = `room-<sessionId>`，
//!   路由参数只是镜像；两者不一致时以 label 为准。
//! - 窗口几何（尺寸、位置、多屏夹取）由 Rust 决定，前端不参与——只有 Rust 拿得到
//!   monitor / work_area / scale_factor。
//! - 房间窗口**关闭即只关窗口**，不断链路；断开必须走显式的 `room_leave`。
//!
//! 窗口级显隐动画统一走 [`animation`]：由系统渲染，不写 CSS。

pub mod animation;
pub mod factory;
pub mod widget;
