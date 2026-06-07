//! 中继客户端：连接最优中继用于加速聊天
//!
//! 聊天服务器作为一个"对等节点"连接到中继网络，
//! 使用中继的 UDP 转发能力优化语音聊天的传输速度。

use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use rand::seq::SliceRandom;

use crate::central::NodeInfo;

/// 连接到一个中继节点
pub fn connect_to_relay(
    relay_addr: &str,
    udp_port: u16,
    team_id: &str,
) -> Option<RelayClient> {
    let addr: SocketAddr = relay_addr.parse().ok()?;

    // 连接到中继的 UDP 端口进行语音中继
    let relay_udp_addr = SocketAddr::new(addr.ip(), udp_port);
    let local_udp = UdpSocket::bind("0.0.0.0:0").ok()?;
    local_udp.set_nonblocking(true).ok()?;

    // 发送注册包标识自己到该队伍
    let mut reg_pkt = [0u8; 36];
    let team_bytes = team_id.as_bytes();
    let copy_len = team_bytes.len().min(35);
    reg_pkt[..copy_len].copy_from_slice(&team_bytes[..copy_len]);
    reg_pkt[copy_len] = 0;

    if local_udp.send_to(&reg_pkt[..36], relay_udp_addr).is_err() {
        log::warn!("[中继/UDP] 注册到队伍 {} 失败", team_id);
        return None;
    }

    log::info!("[中继/UDP] 已连接中继 {} 队伍 {}", relay_addr, team_id);

    Some(RelayClient {
        local_udp: Arc::new(local_udp),
        relay_udp_addr,
        team_id: team_id.to_string(),
    })
}

/// 中继客户端状态
pub struct RelayClient {
    pub local_udp: Arc<UdpSocket>,
    pub relay_udp_addr: SocketAddr,
    pub team_id: String,
}

/// 从中继列表中选择最优中继
pub fn select_best_relay(nodes: &[NodeInfo]) -> Option<&NodeInfo> {
    // 只选择 service_type 为 "relay" 的中继
    let relays: Vec<&NodeInfo> = nodes.iter()
        .filter(|n| n.service_type == "relay" || n.service_type.is_empty())
        .filter(|n| !n.private)
        .collect();

    if relays.is_empty() {
        return None;
    }

    // 随机选择一个（简化版，后续可加入延迟探测）
    Some(relays.choose(&mut rand::thread_rng()).unwrap())
}

/// 探测中继延迟（简化版：TCP 连接耗时）
#[allow(dead_code)]
pub fn probe_relay(address: &str) -> Option<u64> {
    let start = std::time::Instant::now();
    if TcpStream::connect_timeout(&address.parse().ok()?, Duration::from_secs(3)).is_ok() {
        Some(start.elapsed().as_millis() as u64)
    } else {
        None
    }
}

/// 通过中继转发 UDP 语音包的接收循环
pub fn spawn_relay_voice_relay(
    local_udp: Arc<UdpSocket>,
    relay_udp_addr: SocketAddr,
    team_id: String,
) {
    tokio::spawn(async move {
        loop {
            let socket = local_udp.clone();
            let relay = relay_udp_addr;
            let team = team_id.clone();

            let result = tokio::task::spawn_blocking(move || {
                let mut local_buf = [0u8; 4096];
                match socket.recv_from(&mut local_buf) {
                    Ok((n, src)) => {
                        // 转发到中继：前 36 字节为队伍 ID，后续为负载
                        let team_bytes = team.as_bytes();
                        let copy_len = team_bytes.len().min(35);
                        let mut pkt = [0u8; 4096];
                        pkt[..copy_len].copy_from_slice(&team_bytes[..copy_len]);
                        pkt[copy_len] = 0;
                        let payload = &local_buf[..n];
                        let payload_len = payload.len().min(4096 - 36);
                        pkt[36..36 + payload_len].copy_from_slice(&payload[..payload_len]);
                        let _ = socket.send_to(&pkt[..36 + payload_len], relay);
                        (n, src, true)
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        (0, SocketAddr::from(([0, 0, 0, 0], 0)), false)
                    }
                    Err(_) => (0, SocketAddr::from(([0, 0, 0, 0], 0)), false),
                }
            }).await;

            if let Ok((_n, _src, found)) = result {
                if !found {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }
    });
}