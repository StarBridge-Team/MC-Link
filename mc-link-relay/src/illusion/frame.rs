//! Illusion 帧协议编解码（同步版本）
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion\src\protocol\frame.rs

use std::io::{self, Read};
use std::net::TcpStream;

use crate::illusion::types::MAGIC;

/// 编码一个协议帧为字节序列
pub fn encode_frame(frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + payload.len());
    buf.extend_from_slice(&MAGIC);
    buf.push(frame_type);
    buf.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// 检查数据是否以 Illusion 魔法字节开头
pub fn is_illusion_connection(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == MAGIC[0] && data[1] == MAGIC[1]
}

/// 从流中读取一帧（完整的 5 字节头 + 负载）
pub fn read_frame(stream: &mut TcpStream) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0u8; 5];
    stream.read_exact(&mut header)?;

    if header[0] != MAGIC[0] || header[1] != MAGIC[1] {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid illusion magic"));
    }

    let frame_type = header[2];
    let payload_len = u16::from_be_bytes([header[3], header[4]]) as usize;

    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload)?;
    }

    Ok(Some((frame_type, payload)))
}
