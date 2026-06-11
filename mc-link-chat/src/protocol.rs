//! # 统一协议定义
//!
//! ## 入口协议
//!
//! 客户端连接 TCP 后，先发送 1 字节服务类型标识符：
//!
//! | 字节值 | 服务     | 后续流程                      |
//! |--------|----------|-------------------------------|
//! | 0x02   | VOICE    | 服务端返回 UDP 端口信息后关闭 |
//!
//! ## 语音协商 (TCP JSON)
//! 服务端收到 0x02 后，返回：
//! ```json
//! {"service":"VOICE", "udp_port": 57900, "team_id":"..."}
//! ```
//! 客户端关闭 TCP 连接，开始向该 UDP 端口发送语音包。

use serde::{Deserialize, Serialize};

// ===== 服务类型 =====

/// TCP 入口服务类型标识符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    Voice, // 0x02
}

impl ServiceType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x02 => Some(Self::Voice),
            _ => None,
        }
    }
}

// ===== 语音协商 =====

#[derive(Debug, Serialize, Deserialize)]
pub struct VoiceNegotiation {
    pub service: String, // "VOICE"
    pub udp_port: u16,
    pub team_id: String,
}

// ===== 常量 =====

/// 默认聊天服务器端口
pub const DEFAULT_CHAT_PORT: u16 = 57896;