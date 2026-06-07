//! 网络协议常量

use std::time::Duration;

/// 心跳超时时间
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(120);
/// 清理间隔
pub const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);