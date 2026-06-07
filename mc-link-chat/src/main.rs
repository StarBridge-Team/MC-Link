//! MC Link 聊天服务器
//!
//! 统一文字/语音服务，单端口入口 + 协议分流。
//! 注册到中央服务器，连接最优中继加速聊天。

mod central;
mod inbox;
mod protocol;
mod relay_client;
mod text;
mod voice;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::inbox::InboxStore;
use crate::protocol::ServiceType;

/// 默认配置
const DEFAULT_BIND: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = protocol::DEFAULT_CHAT_PORT;
const DEFAULT_CENTRAL_SERVER: &str = "mk.aini2.cn:8878";
const HEARTBEAT_INTERVAL: u64 = 10;

/// 语音 UDP 基础端口（从该起点分配）
const VOICE_BASE_PORT: u16 = 57900;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    let bind_addr = format!("{}:{}", DEFAULT_BIND, DEFAULT_PORT);
    let listener = match TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("无法绑定 {}: {}", bind_addr, e);
            return;
        }
    };

    // 查找可用 UDP 端口
    let udp_base = VOICE_BASE_PORT;
    let chat_udp_port = find_available_udp(udp_base).unwrap_or(udp_base);

    log::info!("===========================================");
    log::info!("  MC-Link 聊天服务器 v0.1.0");
    log::info!("===========================================");
    log::info!("  监听端口: {}", DEFAULT_PORT);
    log::info!("  UDP 端口: {}", chat_udp_port);
    log::info!("  中央服务器: {}", DEFAULT_CENTRAL_SERVER);
    log::info!("===========================================");

    // ===== 注册到中央服务器 =====
    let node_name = format!("Chat-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let conn = central::create_connection(
        DEFAULT_CENTRAL_SERVER,
        DEFAULT_PORT,
        chat_udp_port,
        node_name,
    );
    central::start_central_heartbeat(conn, HEARTBEAT_INTERVAL);

    // ===== 获取中继列表并连接最优中继 =====
    let central_addr = {
        let c = central::create_connection(
            DEFAULT_CENTRAL_SERVER,
            DEFAULT_PORT,
            chat_udp_port,
            "temp".into(),
        );
        c.server_addr
    };

    tokio::spawn(async move {
        // 等待中央服务器注册完成
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        loop {
            match central::get_nodes_from_central(&central_addr) {
                Some(nodes) => {
                    if let Some(relay) = relay_client::select_best_relay(&nodes) {
                        let relay_addr = &relay.address;
                        let relay_udp_port = relay.udp_port;
                        log::info!(
                            "[中继] 选中中继 {} @ {} (UDP端口: {})",
                            relay.name, relay_addr, relay_udp_port
                        );

                        // 连接中继
                        if let Some(client) = relay_client::connect_to_relay(
                            relay_addr,
                            relay_udp_port,
                            "voice_relay",
                        ) {
                            log::info!("[中继] 成功连接中继 {}", relay.name);

                            // 启动 UDP 中继转发循环
                            relay_client::spawn_relay_voice_relay(
                                client.local_udp.clone(),
                                client.relay_udp_addr,
                                client.team_id.clone(),
                            );
                        }
                    } else {
                        log::warn!("[中继] 未找到可用中继");
                    }
                }
                None => {
                    log::warn!("[中继] 获取节点列表失败");
                }
            }

            // 每 5 分钟重新选择一次中继
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        }
    });

    // ===== 共享状态 =====
    let inbox = InboxStore::default();
    crate::inbox::spawn_ttl_cleaner(
        inbox.clone(),
        std::time::Duration::from_secs(protocol::TTL_CLEANUP_INTERVAL_SECS),
    );

    let teams: Arc<RwLock<HashMap<String, HashMap<String, tokio::sync::mpsc::UnboundedSender<tokio_tungstenite::tungstenite::Message>>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    let voice_rooms: Arc<RwLock<HashMap<String, Vec<String>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let port_map: Arc<RwLock<HashMap<u16, String>>> =
        Arc::new(RwLock::new(HashMap::new()));

    log::info!("[入口] 等待连接...");

    // ===== 主循环：接受连接 =====
    while let Ok((mut stream, peer_addr)) = listener.accept().await {
        let inbox = inbox.clone();
        let teams = teams.clone();
        let voice_rooms = voice_rooms.clone();
        let port_map = port_map.clone();

        tokio::spawn(async move {
            // 读取 1 字节服务类型标识符
            let mut header = [0u8; 1];
            if let Err(e) = stream.read_exact(&mut header).await {
                log::warn!("[入口] 读取服务类型失败 ({}): {}", peer_addr, e);
                return;
            }

            let service = match ServiceType::from_byte(header[0]) {
                Some(s) => s,
                None => {
                    log::warn!("[入口] 未知服务类型 0x{:02x} ({})", header[0], peer_addr);
                    return;
                }
            };

            match service {
                ServiceType::Text => {
                    text::handle_text_stream(stream, peer_addr, inbox, teams).await;
                }
                ServiceType::Voice => {
                    let team_id = "voice_unknown".to_string();
                    let udp_port = voice::create_voice_room(
                        voice_rooms,
                        port_map,
                        &team_id,
                        VOICE_BASE_PORT,
                    )
                    .await;
                    voice::handle_voice_stream(stream, peer_addr, team_id, udp_port).await;
                }
            }
        });
    }
}

/// 查找可用 UDP 端口
fn find_available_udp(base: u16) -> Option<u16> {
    for port in base..base + 100 {
        if std::net::UdpSocket::bind(format!("0.0.0.0:{}", port)).is_ok() {
            return Some(port);
        }
    }
    None
}