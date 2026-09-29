use serde::{Deserialize, Serialize};

/// 隧道帧头
#[derive(Debug, Clone)]
pub struct TunnelFrameHeader {
    pub path_id: [u8; 16],
    pub hop_index: u8,
    pub total_hops: u8,
    pub flags: u8,
    pub seq_no: u32,
    pub payload_len: u16,
}

impl TunnelFrameHeader {
    pub const MAGIC: [u8; 4] = [b'M', b'T', b'U', b'N'];
    pub const SIZE: usize = 30;

    pub fn direction(&self) -> u8 {
        self.flags & 0x01
    }
    pub fn set_direction(&mut self, dir: u8) {
        self.flags = (self.flags & !0x01) | (dir & 0x01);
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::SIZE);
        buf.extend_from_slice(&Self::MAGIC);
        buf.push(0x01);
        buf.push(self.flags);
        buf.push(self.hop_index);
        buf.push(self.total_hops);
        buf.extend_from_slice(&self.path_id);
        buf.extend_from_slice(&self.seq_no.to_be_bytes());
        buf.extend_from_slice(&self.payload_len.to_be_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE || &data[0..4] != &Self::MAGIC {
            return None;
        }
        if data[4] != 0x01 {
            return None;
        }
        Some(Self {
            flags: data[5],
            hop_index: data[6],
            total_hops: data[7],
            path_id: data[8..24].try_into().ok()?,
            seq_no: u32::from_be_bytes(data[24..28].try_into().ok()?),
            payload_len: u16::from_be_bytes(data[28..30].try_into().ok()?),
        })
    }
}

/// 路径分配
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathAssignment {
    pub path_id: String,
    pub hops: Vec<super::types::PathHop>,
}

/// 房间路由
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomRoute {
    pub room_name: String,
    pub target_relay_id: String,
    pub path_id: String,
    pub direction: String,
}

/// 16进制字符串转路径ID（支持短 ID 和含连字符格式）
pub fn hex_to_path_id(hex_str: &str) -> Option<[u8; 16]> {
    let cleaned = hex_str.replace('-', "");
    hex::decode(cleaned).ok().map(|b| {
        let mut arr = [0u8; 16];
        let len = b.len().min(16);
        arr[..len].copy_from_slice(&b[..len]);
        arr
    })
}
