//! 网络协议模块（Packet 读写、Proxy Protocol V2 解析）

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};

/// Proxy Protocol V2 签名
pub const PROXY_PROTOCOL_V2_SIGNATURE: [u8; 12] =
    [0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A];

/// 读取一个带 4 字节长度前缀的包
pub fn read_packet(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// 写入一个带 4 字节长度前缀的包
pub fn write_packet(stream: &mut TcpStream, data: &[u8]) -> std::io::Result<()> {
    let len_buf = (data.len() as u32).to_be_bytes();
    stream.write_all(&len_buf)?;
    stream.write_all(data)?;
    stream.flush()?;
    Ok(())
}

// ===== Proxy Protocol V2 =====

#[derive(Debug, Clone)]
pub struct ProxyProtocolHeader {
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
}

fn parse_proxy_protocol_v2(data: &[u8]) -> Option<ProxyProtocolHeader> {
    if data.len() < 16 || !data.starts_with(&PROXY_PROTOCOL_V2_SIGNATURE) {
        return None;
    }
    let version = (data[12] >> 4) & 0x0F;
    if version != 0x02 {
        return None;
    }
    let family = (data[13] >> 4) & 0x0F;
    if data[13] & 0x0F != 0x01 {
        return None;
    }
    let len = u16::from_be_bytes(data[14..16].try_into().unwrap()) as usize;
    let hd = &data[16..16 + len];
    match family {
        0x01 if hd.len() >= 12 => Some(ProxyProtocolHeader {
            src_addr: SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(hd[0], hd[1], hd[2], hd[3])),
                u16::from_be_bytes(hd[8..10].try_into().unwrap()),
            ),
            dst_addr: SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(hd[4], hd[5], hd[6], hd[7])),
                u16::from_be_bytes(hd[10..12].try_into().unwrap()),
            ),
        }),
        0x02 if hd.len() >= 36 => {
            let src_ip = IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&hd[0..16]).unwrap()));
            let dst_ip = IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&hd[16..32]).unwrap()));
            Some(ProxyProtocolHeader {
                src_addr: SocketAddr::new(
                    src_ip,
                    u16::from_be_bytes(hd[32..34].try_into().unwrap()),
                ),
                dst_addr: SocketAddr::new(
                    dst_ip,
                    u16::from_be_bytes(hd[34..36].try_into().unwrap()),
                ),
            })
        }
        _ => None,
    }
}

pub fn read_proxy_protocol_header(stream: &mut TcpStream) -> Option<ProxyProtocolHeader> {
    let mut buf = [0u8; 16];
    match stream.peek(&mut buf) {
        Ok(n) if n >= 12 && buf[0..12] == PROXY_PROTOCOL_V2_SIGNATURE => {
            let mut header_buf = vec![0u8; 16];
            stream.read_exact(&mut header_buf).ok()?;
            let len = u16::from_be_bytes(header_buf[14..16].try_into().unwrap()) as usize;
            let mut addr_buf = vec![0u8; len];
            stream.read_exact(&mut addr_buf).ok()?;
            let mut full = header_buf;
            full.extend(addr_buf);
            parse_proxy_protocol_v2(&full)
        }
        _ => None,
    }
}