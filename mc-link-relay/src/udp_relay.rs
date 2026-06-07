//! UDP 中继模块
//!
//! 同一队伍的成员通过 UDP 端口收发语音包，中继服务器负责转发。
//! 数据包格式：前 36 字节为队伍 ID (UTF-8 + \0填充)，后续为语音负载。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use mc_link_common::log::{log, LogLevel};

/// 队伍 → (地址列表)
type UdpTeams = Arc<Mutex<HashMap<String, Vec<SocketAddr>>>>;

/// 启动 UDP 中继监听
pub fn start_udp_relay(port: u16) {
    let bind_addr = format!("0.0.0.0:{}", port);
    let socket = match std::net::UdpSocket::bind(&bind_addr) {
        Ok(s) => s,
        Err(e) => {
            log(LogLevel::Error, &format!("UDP 中继绑定失败 {}: {}", bind_addr, e));
            return;
        }
    };
    socket.set_nonblocking(true).ok();
    log(LogLevel::Info, &format!("UDP 中继监听在 {}", bind_addr));

    let teams: UdpTeams = Arc::new(Mutex::new(HashMap::new()));
    let mut buf = [0u8; 4096];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                if n < 36 {
                    continue;
                }
                // 提取队伍 ID（前 36 字节，截断到第一个 \0）
                let team_id_bytes = &buf[..36];
                let team_id = team_id_bytes
                    .split(|&b| b == 0)
                    .next()
                    .and_then(|s| std::str::from_utf8(s).ok())
                    .unwrap_or("")
                    .to_string();

                if team_id.is_empty() {
                    continue;
                }

                let payload = buf[36..n].to_vec();

                // 自动注册该地址到队伍（若不存在）
                {
                    let mut t = teams.lock().unwrap();
                    let members = t.entry(team_id.clone()).or_default();
                    if !members.contains(&src) {
                        members.push(src);
                        log(LogLevel::Info, &format!("[UDP/加入] {} 加入队伍 {}", src, team_id));
                    }
                }

                // 转发给队伍中除发送者外的所有人
                let t = teams.lock().unwrap();
                if let Some(members) = t.get(&team_id) {
                    for &addr in members {
                        if addr != src {
                            let mut pkt = [0u8; 4096];
                            pkt[..36].copy_from_slice(team_id_bytes);
                            pkt[36..36 + payload.len()].copy_from_slice(&payload);
                            if socket.send_to(&pkt[..36 + payload.len()], addr).is_err() {
                                log(LogLevel::Warn, &format!("[UDP] 发送到 {} 失败", addr));
                            }
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                log(LogLevel::Warn, &format!("UDP 接收错误: {}", e));
            }
        }
    }
}

/// 获取下一个可用 UDP 端口
fn get_udp_port(base: u16) -> u16 {
    for port in base..base + 100 {
        if std::net::UdpSocket::bind(format!("0.0.0.0:{}", port)).is_ok() {
            return port;
        }
    }
    0
}

/// 查找可用 UDP 端口（起始端口 = TCP 端口 + 1）
pub fn find_udp_port(tcp_port: u16) -> u16 {
    let base = tcp_port + 1;
    get_udp_port(base)
}