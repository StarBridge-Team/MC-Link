//! Illusion 协议类型定义
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion\src\protocol\types.rs

use serde::{Deserialize, Serialize};

pub const MAGIC: [u8; 2] = [0x49, 0x4C]; // "IL"

pub const TYPE_REGISTER: u8 = 0x01;
pub const TYPE_REGISTER_ACK: u8 = 0x02;
pub const TYPE_NEW_PROXY: u8 = 0x03;
pub const TYPE_PROXY_READY: u8 = 0x04;
pub const TYPE_HEARTBEAT: u8 = 0x05;

#[allow(dead_code)]
pub const TYPE_ERROR: u8 = 0x06;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Register {
    pub token: String,
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAck {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProxy {
    pub conn_id: u64,
    pub domain: String,
    pub peeked: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyReady {
    pub conn_id: u64,
    pub token: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub code: u32,
    pub message: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat;
