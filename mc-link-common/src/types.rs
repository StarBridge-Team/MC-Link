use serde::{Deserialize, Serialize};

/// 中继服务器信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(default)]
    pub address_v6: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub transit: bool,
    #[serde(default)]
    pub idle: bool,
    #[serde(default)]
    pub service_type: String,
    #[serde(default)]
    pub udp_port: u16,
}

impl RelayInfo {
    /// 获取首选连接地址（优先 IPv6）
    pub fn preferred_address(&self) -> &str {
        self.address_v6.as_deref().unwrap_or(&self.address)
    }
}

/// 房间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub name: String,
    pub password_hash: String,
    pub host_relay_id: String,
    pub created_at: u64,
}

/// 玩家信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub name: String,
    pub role: String,
    pub joined_at: u64,
}

/// 路径跳
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathHop {
    pub node_id: String,
    pub address: String,
    #[serde(default)]
    pub address_v6: Option<String>,
}

/// 打包加密包
pub fn pack_packet(room: &str, password: &str, data: &[u8]) -> Vec<u8> {
    let encrypted = crate::crypto::encrypt(data, password);
    let mut packet = Vec::with_capacity(2 + room.len() + password.len() + encrypted.len());
    packet.push(room.len() as u8);
    packet.extend_from_slice(room.as_bytes());
    packet.push(password.len() as u8);
    packet.extend_from_slice(password.as_bytes());
    packet.extend_from_slice(&encrypted);
    packet
}

/// 尝试解密响应
pub fn try_decrypt_response(data: &[u8], password: &str) -> Option<Vec<u8>> {
    if data.len() < 2 {
        return None;
    }
    let room_len = data[0] as usize;
    if data.len() < 1 + room_len + 1 {
        return None;
    }
    let pass_len = data[1 + room_len] as usize;
    if data.len() < 1 + room_len + 1 + pass_len {
        return None;
    }
    let encrypted_start = 1 + room_len + 1 + pass_len;
    let encrypted = &data[encrypted_start..];
    crate::crypto::decrypt(encrypted, password)
}
