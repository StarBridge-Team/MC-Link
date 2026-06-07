//! # 统一协议定义
//!
//! ## 入口协议
//!
//! 客户端连接 TCP 后，先发送 1 字节服务类型标识符：
//!
//! | 字节值 | 服务     | 后续流程                      |
//! |--------|----------|-------------------------------|
//! | 0x01   | TEXT     | WebSocket 握手升级            |
//! | 0x02   | VOICE    | 服务端返回 UDP 端口信息后关闭 |
//!
//! ## 文字聊天 (WebSocket JSON)
//!
//! ### 客户端 → 服务端
//! ```json
//! {"type":"auth",   "team_id":"...", "sender":"..."}   // 认证/加入队伍
//! {"type":"chat",   "msg_id":"...", "text":"..."}       // 发送消息
//! {"type":"ack",    "msg_ids":["..."]}                  // 确认已收到
//! {"type":"leave"}                                      // 离开队伍
//! ```
//!
//! ### 服务端 → 客户端
//! ```json
//! {"type":"chat",   "msg_id":"...", "sender":"...", "text":"...", "time":123, "from_history":false}
//! {"type":"history","messages":[...]}
//! {"type":"ack_ok", "msg_ids":["..."]}
//! {"type":"error",  "code":"...", "msg":"..."}
//! ```
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
    Text,  // 0x01
    Voice, // 0x02
}

impl ServiceType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Text),
            0x02 => Some(Self::Voice),
            _ => None,
        }
    }
}

// ===== 文字聊天消息 =====

/// 客户端 → 服务端
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "auth")]
    Auth {
        team_id: String,
        sender: String,
    },
    #[serde(rename = "chat")]
    Chat {
        msg_id: String,
        text: String,
    },
    #[serde(rename = "ack")]
    Ack {
        msg_ids: Vec<String>,
    },
    #[serde(rename = "leave")]
    Leave,
}

/// 服务端 → 客户端
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "chat")]
    Chat {
        msg_id: String,
        sender: String,
        text: String,
        time: u64,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        from_history: bool,
    },
    #[serde(rename = "history")]
    History {
        messages: Vec<HistoryMsg>,
    },
    #[serde(rename = "ack_ok")]
    AckOk {
        msg_ids: Vec<String>,
    },
}

/// 历史/待收消息条目
#[derive(Debug, Serialize)]
pub struct HistoryMsg {
    pub msg_id: String,
    pub sender: String,
    pub text: String,
    pub time: u64,
}

/// 内存中的待收消息
#[derive(Debug, Clone)]
pub struct PendingMessage {
    pub msg_id: String,
    pub sender: String,
    pub text: String,
    pub time: u64,
    pub created_at: std::time::Instant,
    pub ttl: std::time::Duration,
}

impl PendingMessage {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.ttl
    }
}

impl From<&PendingMessage> for HistoryMsg {
    fn from(p: &PendingMessage) -> Self {
        HistoryMsg {
            msg_id: p.msg_id.clone(),
            sender: p.sender.clone(),
            text: p.text.clone(),
            time: p.time,
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

/// 消息 TTL（秒）
pub const MESSAGE_TTL_SECS: u64 = 300;

/// TTL 清理间隔（秒）
pub const TTL_CLEANUP_INTERVAL_SECS: u64 = 30;