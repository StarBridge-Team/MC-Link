use serde::{Deserialize, Serialize};

pub use mc_link_common::crypto::{encrypt, decrypt};
pub use mc_link_common::protocol::{read_packet, write_packet};

// ===== 自定义协议检测 =====

pub fn is_custom_protocol(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    let room_len = data[0] as usize;
    if room_len == 0 || room_len > 100 {
        return false;
    }
    if data.len() < 1 + room_len + 1 {
        return false;
    }
    let pass_len = data[1 + room_len] as usize;
    if pass_len == 0 || pass_len > 100 {
        return false;
    }
    // GCM 加密输出: nonce(12) + ciphertext_with_tag(plaintext + 16)
    // 最小密文长度 = 12 + 最小明文(至少含4字节命令) + 16 = 32
    const GCM_OVERHEAD: usize = 28; // NONCE_SIZE(12) + TAG_SIZE(16)
    if data.len() < 1 + room_len + 1 + pass_len + GCM_OVERHEAD {
        return false;
    }
    true
}

// ===== 多跳隧道协议 =====

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathHop {
    pub node_id: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomRoute {
    pub room_name: String,
    pub target_relay_id: String,
    pub path_id: String,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathAssignment {
    pub path_id: String,
    pub hops: Vec<PathHop>,
}

// ===== 辅助函数 =====

pub fn hex_to_path_id(hex_str: &str) -> Option<[u8; 16]> {
    let cleaned = hex_str.replace('-', "");
    hex::decode(cleaned).ok().map(|b| {
        let mut arr = [0u8; 16];
        let len = b.len().min(16);
        arr[..len].copy_from_slice(&b[..len]);
        arr
    })
}