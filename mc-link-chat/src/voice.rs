//! # 语音聊天模块（入口框架）
//!
//! TCP 握手中协商 UDP 端口，之后客户端通过 UDP 发送语音包，
//! 服务端将该队伍的语音包广播给其他成员。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::RwLock;

use crate::protocol::VoiceNegotiation;

/// 队伍 → UDP 地址列表
type VoiceRooms = Arc<RwLock<HashMap<String, Vec<String>>>>;
/// UDP 端口 → 队伍 ID
type PortMap = Arc<RwLock<HashMap<u16, String>>>;

/// 处理一条已识别的 VOICE 服务 TCP 流
///
/// 服务端返回 UDP 端口信息，客户端据此开始发送 UDP 语音包。
pub async fn handle_voice_stream(
    stream: tokio::net::TcpStream,
    _peer_addr: std::net::SocketAddr,
    team_id: String,
    udp_port: u16,
) {
    let neg = VoiceNegotiation {
        service: "VOICE".to_string(),
        udp_port,
        team_id,
    };

    let json = match serde_json::to_string(&neg) {
        Ok(j) => j,
        Err(_) => return,
    };

    if let Err(e) = stream.try_write(json.as_bytes()) {
        log::warn!("[VOICE] TCP 写入失败: {}", e);
        return;
    }

    log::info!("[VOICE] 已协商 UDP 端口 {}", udp_port);
    // TCP 连接在此关闭，客户端转用 UDP
}

/// 启动 UDP 语音转发服务
///
/// 分配一个端口给指定队伍，收到的所有 UDP 包广播给该队伍其他成员。
pub fn spawn_udp_relay(
    voice_rooms: VoiceRooms,
    port: u16,
    team_id: String,
) {
    tokio::spawn(async move {
        let bind_addr = format!("0.0.0.0:{}", port);
        let socket = match UdpSocket::bind(&bind_addr).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("[VOICE] 绑定 UDP {} 失败: {}", port, e);
                return;
            }
        };

        log::info!("[VOICE] UDP 语音房间已启动: team={}, port={}", team_id, port);

        let mut buf = [0u8; 1024];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((n, src_addr)) => {
                    let data = buf[..n].to_vec();
                    let src = format!("{}", src_addr);

                    // 读取房间成员列表并转发
                    let room = voice_rooms.read().await;
                    if let Some(members) = room.get(&team_id) {
                        for member in members {
                            if member != &src {
                                if let Ok(addr) = member.parse::<std::net::SocketAddr>() {
                                    let _ = socket.send_to(&data, addr).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[VOICE] UDP 接收错误: {}", e);
                }
            }
        }
    });
}

/// 分配下一个可用 UDP 端口
pub fn allocate_udp_port(base_port: u16) -> u16 {
    static NEXT_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    let offset = NEXT_PORT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    base_port + (offset % 100) // 100 个端口池循环
}

/// 创建一个 UDP 语音房间
pub async fn create_voice_room(
    voice_rooms: VoiceRooms,
    port_map: PortMap,
    team_id: &str,
    base_port: u16,
) -> u16 {
    let port = allocate_udp_port(base_port);

    {
        let mut rooms = voice_rooms.write().await;
        rooms.entry(team_id.to_string()).or_default();
    }

    {
        let mut ports = port_map.write().await;
        ports.insert(port, team_id.to_string());
    }

    spawn_udp_relay(voice_rooms.clone(), port, team_id.to_string());
    port
}