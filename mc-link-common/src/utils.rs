//! 通用工具函数

use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{MutexGuard, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::log::{log, LogLevel};

/// 获取当前 Unix 时间戳（秒）
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// 获取可执行文件所在目录
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 从地址字符串中提取 host 和 port
fn split_host_port(addr_str: &str) -> Option<(&str, u16)> {
    if let Some(bracket_end) = addr_str.rfind(']') {
        let host = &addr_str[1..bracket_end];
        let rest = &addr_str[bracket_end + 1..];
        if let Some(port_str) = rest.strip_prefix(':') {
            let port = port_str.parse::<u16>().ok()?;
            Some((host, port))
        } else {
            None
        }
    } else {
        let parts: Vec<&str> = addr_str.rsplitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        let port = parts[0].parse::<u16>().ok()?;
        Some((parts[1], port))
    }
}



// ===== SRV 记录解析 =====

/// DNS 记录类型常量
const DNS_TYPE_SRV: u16 = 33;
const DNS_CLASS_IN: u16 = 1;

/// 尝试 SRV 解析 `_mclink._tcp.{domain}`，返回 (目标主机, 端口)
pub fn try_srv_lookup(domain: &str) -> Option<(String, u16)> {
    let srv_name = format!("_mclink._tcp.{}", domain);

    // 首选 8.8.8.8:53，避免系统 DNS 可能不支持 SRV
    let dns_servers = ["8.8.8.8:53", "1.1.1.1:53"];

    for dns in &dns_servers {
        if let Some(result) = dns_srv_query(dns, &srv_name) {
            return Some(result);
        }
    }
    None
}

/// 通过 UDP 向指定 DNS 服务器查询 SRV 记录
fn dns_srv_query(dns_addr: &str, name: &str) -> Option<(String, u16)> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.set_read_timeout(Some(Duration::from_secs(5))).ok()?;

    let dns_server: SocketAddr = dns_addr.parse().ok()?;

    // 编码 DNS 查询
    let query = encode_dns_query(name, DNS_TYPE_SRV, DNS_CLASS_IN);
    sock.send_to(&query, dns_server).ok()?;

    // 接收响应
    let mut buf = [0u8; 512];
    let n = sock.recv_from(&mut buf).ok()?.0;

    // 解析响应
    parse_srv_response(&buf[..n])
}

/// 编码 DNS 查询包
fn encode_dns_query(name: &str, qtype: u16, qclass: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(512);

    // Header
    let id: u16 = 0x1234;
    buf.extend_from_slice(&id.to_be_bytes());  // ID
    buf.extend_from_slice(&[0x01, 0x00]);      // flags: standard query, RD=1
    buf.extend_from_slice(&[0x00, 0x01]);      // QDCOUNT = 1
    buf.extend_from_slice(&[0x00, 0x00]);      // ANCOUNT = 0
    buf.extend_from_slice(&[0x00, 0x00]);      // NSCOUNT = 0
    buf.extend_from_slice(&[0x00, 0x00]);      // ARCOUNT = 0

    // Question: encode domain name
    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        buf.push(label.len() as u8);
        buf.extend_from_slice(label.as_bytes());
    }
    buf.push(0x00); // root label

    // QTYPE, QCLASS
    buf.extend_from_slice(&qtype.to_be_bytes());
    buf.extend_from_slice(&qclass.to_be_bytes());

    buf
}

/// 解析 DNS 响应，提取 SRV 记录
fn parse_srv_response(data: &[u8]) -> Option<(String, u16)> {
    if data.len() < 12 {
        return None;
    }

    // Header
    let _id = u16::from_be_bytes([data[0], data[1]]);
    let flags = u16::from_be_bytes([data[2], data[3]]);
    // Check QR=1 (response) and RCODE=0 (no error)
    if flags & 0x8000 == 0 || (flags & 0x000F) != 0 {
        return None;
    }

    let ancount = u16::from_be_bytes([data[6], data[7]]);
    if ancount == 0 {
        return None;
    }

    let mut pos = 12;

    // Skip question section
    pos = skip_dns_name(data, pos)?;
    pos += 4; // skip QTYPE + QCLASS

    // Parse answer records - find the first SRV record
    for _ in 0..ancount {
        pos = skip_dns_name(data, pos)?;

        if pos + 10 > data.len() {
            return None;
        }

        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let _rclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        let _ttl = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let rdlength = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;

        if rtype == DNS_TYPE_SRV && rdlength >= 7 {
            if pos + rdlength > data.len() {
                return None;
            }
            let _priority = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let _weight = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
            let port = u16::from_be_bytes([data[pos + 4], data[pos + 5]]);

            let target = extract_dns_name(data, pos + 6)?;
            let target = target.trim_end_matches('.').to_string();
            if !target.is_empty() && target != "." {
                return Some((target, port));
            }
            break;
        }

        pos += rdlength;
    }

    None
}

/// 跳过 DNS 名称（可能包含压缩指针）
fn skip_dns_name(data: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        if pos >= data.len() {
            return None;
        }
        let byte = data[pos];
        if byte & 0xC0 == 0xC0 {
            // Compression pointer (2 bytes)
            return Some(pos + 2);
        }
        if byte == 0x00 {
            // Root label
            return Some(pos + 1);
        }
        // Normal label: skip length + label bytes
        pos += 1 + byte as usize;
    }
}

/// 提取 DNS 名称（支持压缩指针）
fn extract_dns_name(data: &[u8], mut pos: usize) -> Option<String> {
    let mut labels = Vec::new();

    loop {
        if pos >= data.len() {
            return None;
        }
        let byte = data[pos];
        if byte & 0xC0 == 0xC0 {
            // Compression pointer
            if pos + 1 >= data.len() {
                return None;
            }
            let offset = ((byte as usize & 0x3F) << 8) | data[pos + 1] as usize;
            pos = offset;
            continue;
        }
        if byte == 0x00 {
            break;
        }
        // Normal label
        pos += 1;
        if pos + byte as usize > data.len() {
            return None;
        }
        let label = std::str::from_utf8(&data[pos..pos + byte as usize]).ok()?;
        labels.push(label.to_string());
        pos += byte as usize;
    }

    Some(labels.join(".") + ".")
}

/// 解析地址，支持 SRV 回退
///
/// 如果地址字符串不含端口号，会先尝试 `_mclink._tcp.{host}` SRV 查询，
/// 如果 SRV 成功则使用返回的目标和端口，否则使用 `default_port` 回退到常规 DNS。
pub fn resolve_address_srv(addr_str: &str, default_port: u16) -> Option<SocketAddr> {
    // 先尝试直接解析（处理纯 IP:port 格式）
    if let Ok(addr) = addr_str.parse::<SocketAddr>() {
        return Some(addr);
    }

    // 尝试解析 host:port，如果没有端口号则尝试 SRV
    if let Some((host, port)) = split_host_port(addr_str) {
        // 显式指定了端口，直接解析
        return std::net::ToSocketAddrs::to_socket_addrs(&(host, port))
            .ok()?
            .next();
    }

    // 没有端口号，先试 SRV
    if let Some((target, srv_port)) = try_srv_lookup(addr_str) {
        return std::net::ToSocketAddrs::to_socket_addrs(&(target.as_str(), srv_port))
            .ok()?
            .next();
    }

    // SRV 失败，使用默认端口回退
    std::net::ToSocketAddrs::to_socket_addrs(&(addr_str, default_port))
        .ok()?
        .next()
}

// ===== 并发锁安全访问辅助函数 =====

/// 获取 Mutex 锁，锁中毒时记录错误日志并强制恢复
///
/// 使用场景：在无法优雅处理锁中毒时，确保程序能继续运行并记录中毒事件。
/// 对于不能安全恢复的关键操作，请使用 `match lock.lock() { Ok(g) => ..., Err(_) => return }` 模式。
pub fn lock_or_recover<'a, T>(lock: &'a std::sync::Mutex<T>, context: &str) -> MutexGuard<'a, T> {
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log(LogLevel::Error, &format!("{} 锁中毒，强制恢复", context));
            poisoned.into_inner()
        }
    }
}

/// 获取 RwLock 读锁，锁中毒时记录错误日志并强制恢复
pub fn rwlock_read_or_recover<'a, T>(lock: &'a std::sync::RwLock<T>, context: &str) -> RwLockReadGuard<'a, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log(LogLevel::Error, &format!("{} 读写锁(读)中毒，强制恢复", context));
            poisoned.into_inner()
        }
    }
}

/// 获取 RwLock 写锁，锁中毒时记录错误日志并强制恢复
pub fn rwlock_write_or_recover<'a, T>(lock: &'a std::sync::RwLock<T>, context: &str) -> RwLockWriteGuard<'a, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log(LogLevel::Error, &format!("{} 读写锁(写)中毒，强制恢复", context));
            poisoned.into_inner()
        }
    }
}
