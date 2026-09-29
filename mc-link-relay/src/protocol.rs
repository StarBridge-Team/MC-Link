pub use mc_link_common::crypto::{encrypt, decrypt};
pub use mc_link_common::protocol::{read_packet, write_packet};
pub use mc_link_common::tunnel::{TunnelFrameHeader, PathAssignment, RoomRoute, hex_to_path_id};
pub use mc_link_common::types::PathHop;

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