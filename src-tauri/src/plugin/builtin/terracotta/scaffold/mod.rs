//! 陶瓦的 Scaffolding 传输层：裸 TCP 上的"一请求一响应"帧。
//!
//! # 帧格式（与上游逐字节一致）
//!
//! ```text
//! 请求: [u8 kind_len][kind = "命名空间:方法"][u32 body_len BE][body]
//! 响应: [u8 状态码  ][u32 body_len BE][body]        状态码 0 = 成功
//! ```
//!
//! 没有长度超过 255 的 kind（命名空间与方法名都很短），也没有加密——这条连接本身
//! 就跑在 EasyTier 的加密隧道里，再套一层只会让互通变复杂。
//!
//! # 与上游的两处差异
//!
//! 1. **body 长度上限**：上游直接 `vec![0u8; body_len]` 分配，对端只要声称 4GB
//!    就能让我们 OOM。这里加了上限，超限即断开。
//! 2. **超时**：上下游都有 64 秒读写超时；保留，避免半开连接把线程挂死。

pub mod client;
pub mod server;

use std::io::{self, Read, Write};
use std::time::Duration;

/// 读写超时（与上游同值）。
pub const TIMEOUT: Duration = Duration::from_secs(64);

/// 单帧正文上限。陶瓦最大的响应是成员列表（几十字节量级），1 MiB 已经很宽松。
const MAX_BODY: usize = 1024 * 1024;

/// 请求帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 命名空间（陶瓦只有 `c`）。
    pub namespace: String,
    /// 方法名（`ping` / `server_port` …）。
    pub path: String,
    /// 正文。
    pub body: Vec<u8>,
}

/// 响应帧。状态码 `0` 表示成功。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketResponse {
    Ok { data: Vec<u8> },
    Fail { status: u8, data: Vec<u8> },
}

impl PacketResponse {
    /// 成功响应。
    pub fn ok(data: Vec<u8>) -> io::Result<Self> {
        Ok(PacketResponse::Ok { data })
    }

    /// 带状态码的失败响应（上游用 32 表示"状态不对"，255 表示"未实现"）。
    pub fn fail(status: u8, data: Vec<u8>) -> io::Result<Self> {
        Ok(PacketResponse::Fail { status, data })
    }

    /// 状态码（成功为 0）。
    pub fn status(&self) -> u8 {
        match self {
            PacketResponse::Ok { .. } => 0,
            PacketResponse::Fail { status, .. } => *status,
        }
    }

    /// 正文。
    pub fn data(&self) -> &[u8] {
        match self {
            PacketResponse::Ok { data } | PacketResponse::Fail { data, .. } => data,
        }
    }
}

/// 未实现的方法（上游的同一取值）。
pub const STATUS_UNIMPLEMENTED: u8 = 255;

/// 读一个请求帧。
pub fn read_frame(reader: &mut impl Read) -> io::Result<Frame> {
    let mut kind_len = [0u8; 1];
    reader.read_exact(&mut kind_len)?;
    let mut kind = vec![0u8; kind_len[0] as usize];
    reader.read_exact(&mut kind)?;
    let kind = String::from_utf8(kind)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    let mut parts = kind.splitn(3, ':');
    let namespace = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    if namespace.is_empty() || path.is_empty() || parts.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("请求方法名非法: {}", kind),
        ));
    }

    let body = read_body(reader)?;
    Ok(Frame {
        namespace,
        path,
        body,
    })
}

/// 写一个请求帧。
pub fn write_frame(writer: &mut impl Write, frame: &Frame) -> io::Result<()> {
    let kind = format!("{}:{}", frame.namespace, frame.path);
    if kind.len() > u8::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "请求方法名过长",
        ));
    }
    let mut buffer = Vec::with_capacity(kind.len() + frame.body.len() + 5);
    buffer.push(kind.len() as u8);
    buffer.extend_from_slice(kind.as_bytes());
    buffer.extend_from_slice(&(frame.body.len() as u32).to_be_bytes());
    buffer.extend_from_slice(&frame.body);
    writer.write_all(&buffer)?;
    writer.flush()
}

/// 读一个响应帧。
pub fn read_response(reader: &mut impl Read) -> io::Result<PacketResponse> {
    let mut status = [0u8; 1];
    reader.read_exact(&mut status)?;
    let data = read_body(reader)?;
    Ok(if status[0] == 0 {
        PacketResponse::Ok { data }
    } else {
        PacketResponse::Fail {
            status: status[0],
            data,
        }
    })
}

/// 写一个响应帧。
pub fn write_response(writer: &mut impl Write, response: &PacketResponse) -> io::Result<()> {
    let mut buffer = Vec::with_capacity(response.data().len() + 5);
    buffer.push(response.status());
    buffer.extend_from_slice(&(response.data().len() as u32).to_be_bytes());
    buffer.extend_from_slice(response.data());
    writer.write_all(&buffer)?;
    writer.flush()
}

/// 读 `[u32 长度][正文]`。
fn read_body(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut raw = [0u8; 4];
    reader.read_exact(&mut raw)?;
    let len = u32::from_be_bytes(raw) as usize;
    if len > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("正文过长（{} 字节，上限 {}）", len, MAX_BODY),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let frame = Frame {
            namespace: "c".to_string(),
            path: "ping".to_string(),
            body: b"hello".to_vec(),
        };
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &frame).unwrap();

        // 逐字节对照协议：kind_len=6, "c:ping", body_len=5
        assert_eq!(buffer[0], 6);
        assert_eq!(&buffer[1..7], b"c:ping");
        assert_eq!(&buffer[7..11], &5u32.to_be_bytes());

        let parsed = read_frame(&mut &buffer[..]).unwrap();
        assert_eq!(parsed, frame);
    }

    #[test]
    fn response_roundtrip_and_status() {
        let ok = PacketResponse::ok(b"data".to_vec()).unwrap();
        let mut buffer = Vec::new();
        write_response(&mut buffer, &ok).unwrap();
        assert_eq!(buffer[0], 0);
        assert_eq!(read_response(&mut &buffer[..]).unwrap(), ok);

        let fail = PacketResponse::fail(32, b"nope".to_vec()).unwrap();
        let mut buffer = Vec::new();
        write_response(&mut buffer, &fail).unwrap();
        assert_eq!(buffer[0], 32);
        let parsed = read_response(&mut &buffer[..]).unwrap();
        assert_eq!(parsed.status(), 32);
        assert_eq!(parsed.data(), b"nope");
    }

    #[test]
    fn oversized_body_is_rejected() {
        // 声称 2 MiB，实际不给正文：必须在分配前就被拒
        let mut buffer = vec![1u8];
        buffer.push(b'x');
        buffer.extend_from_slice(&(2 * 1024 * 1024u32).to_be_bytes());
        let err = read_frame(&mut &buffer[..]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn malformed_kind_is_rejected() {
        let mut buffer = vec![3u8];
        buffer.extend_from_slice(b"abc");
        buffer.extend_from_slice(&0u32.to_be_bytes());
        assert!(read_frame(&mut &buffer[..]).is_err());
    }

    #[test]
    fn overlong_kind_is_refused_on_write() {
        let frame = Frame {
            namespace: "c".to_string(),
            path: "x".repeat(300),
            body: Vec::new(),
        };
        let mut buffer = Vec::new();
        assert!(write_frame(&mut buffer, &frame).is_err());
    }
}
