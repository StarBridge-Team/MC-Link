//! 域名嗅探器（同步版，仅 Minecraft）
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion\src\server\sniffer.rs

/// 嗅探结果
#[derive(Debug, Clone)]
pub enum Target {
    Minecraft { host: String },
    Unknown,
}

/// 对 peek 到的数据进行 Minecraft 握手嗅探
pub fn sniff(data: &[u8]) -> Target {
    match extract_minecraft_host(data) {
        Some(host) => Target::Minecraft { host },
        None => Target::Unknown,
    }
}

/// Decode a Minecraft-style VarInt from bytes. Returns (value, bytes_consumed).
fn decode_varint(data: &[u8]) -> Option<(i32, usize)> {
    let mut value: i32 = 0;
    let mut shift = 0;
    for (i, &b) in data.iter().enumerate() {
        if i >= 3 {
            return None;
        }
        value |= ((b & 0x7F) as i32) << shift;
        if b & 0x80 == 0 {
            return Some((value, i + 1));
        }
        shift += 7;
    }
    None
}

/// Extract server address from Minecraft Handshake (Packet ID 0x00).
fn extract_minecraft_host(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }

    let (_, consumed) = decode_varint(data)?;
    let mut pos = consumed;

    let (packet_id, consumed) = decode_varint(&data[pos..])?;
    if packet_id != 0x00 {
        return None;
    }
    pos += consumed;

    let (_, consumed) = decode_varint(&data[pos..])?;
    pos += consumed;

    let (addr_len, consumed) = decode_varint(&data[pos..])?;
    if addr_len <= 0 || addr_len > 255 {
        return None;
    }
    pos += consumed;

    if pos + addr_len as usize > data.len() {
        return None;
    }

    let addr = std::str::from_utf8(&data[pos..pos + addr_len as usize]).ok()?;
    let addr = addr.trim_end_matches('.');

    if addr.contains('.') || addr.eq_ignore_ascii_case("localhost") {
        Some(addr.to_string())
    } else {
        None
    }
}
