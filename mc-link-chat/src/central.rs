//! 中央服务器注册和心跳管理
//!
//! 聊天服务器作为中央服务器的一个节点注册，使用与中继服务器相同的协议：
//! - 0x10: 注册（含 service_type="chat"）
//! - 0x11: 心跳
//! - 0x12: 获取中继/节点列表
//! - 0x13: 节点列表响应

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use mc_link_common::protocol::{read_packet, write_packet};

/// 中央服务器连接状态
pub struct CentralConnection {
    #[allow(dead_code)]
    pub stream: Option<TcpStream>,
    pub server_addr: SocketAddr,
    pub node_id: String,
    pub node_name: String,
    #[allow(dead_code)]
    pub listen_port: u16,
    pub udp_port: u16,
    pub address: String,
}

/// 节点信息（从中央服务器获取的节点列表）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NodeInfo {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(default)]
    pub service_type: String,
    #[serde(default)]
    pub udp_port: u16,
    #[serde(default)]
    pub private: bool,
    #[serde(default = "default_true")]
    #[allow(dead_code)]
    pub transit: bool,
}

fn default_true() -> bool { true }

/// 向中央服务器注册
fn register(stream: &mut TcpStream, conn: &CentralConnection) -> bool {
    let req = serde_json::json!({
        "id": conn.node_id,
        "name": conn.node_name,
        "address": conn.address,
        "private": false,
        "transit": false,
        "service_type": "chat",
        "udp_port": conn.udp_port,
    });

    let mut packet = vec![0x10];
    packet.extend_from_slice(req.to_string().as_bytes());
    if write_packet(stream, &packet).is_err() {
        return false;
    }

    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    match read_packet(stream) {
        Ok(buf) if buf.len() >= 2 && buf[0] == 0x10 && buf[1] == 0x00 => {
            log::info!("[中央] 注册成功");
            true
        }
        _ => {
            log::warn!("[中央] 注册未收到确认");
            false
        }
    }
}

/// 发送心跳包
fn send_heartbeat(stream: &mut TcpStream, node_id: &str) -> bool {
    let req = serde_json::json!({"id": node_id});
    let mut packet = vec![0x11];
    packet.extend_from_slice(req.to_string().as_bytes());
    write_packet(stream, &packet).is_ok()
}

/// 从中央服务器获取可用节点列表
pub fn get_nodes_from_central(central_addr: &SocketAddr) -> Option<Vec<NodeInfo>> {
    let mut stream = TcpStream::connect_timeout(central_addr, Duration::from_secs(5)).ok()?;
    write_packet(&mut stream, &[0x12]).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let buf = read_packet(&mut stream).ok()?;
    if buf.is_empty() || buf[0] != 0x13 {
        return None;
    }
    serde_json::from_slice::<Vec<NodeInfo>>(&buf[1..]).ok()
}

/// 启动中央服务器注册和心跳线程
pub fn start_central_heartbeat(conn: CentralConnection, heartbeat_interval: u64) {
    std::thread::spawn(move || {
        let central_addr = conn.server_addr;
        loop {
            let mut stream = match TcpStream::connect_timeout(&central_addr, Duration::from_secs(5)) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("[中央] 连接失败: {}，10秒后重试", e);
                    std::thread::sleep(Duration::from_secs(10));
                    continue;
                }
            };

            if !register(&mut stream, &conn) {
                log::warn!("[中央] 注册失败，10秒后重试");
                std::thread::sleep(Duration::from_secs(10));
                continue;
            }

            loop {
                std::thread::sleep(Duration::from_secs(heartbeat_interval));
                if !send_heartbeat(&mut stream, &conn.node_id) {
                    log::warn!("[中央] 心跳发送失败，重新注册");
                    break;
                }
            }
        }
    });
}

/// 获取本机 IP
pub fn local_ip() -> String {
    match local_ip_address::local_ip() {
        Ok(ip) => ip.to_string(),
        Err(_) => "127.0.0.1".to_string(),
    }
}

/// 创建中央连接配置
pub fn create_connection(
    central_server: &str,
    listen_port: u16,
    udp_port: u16,
    node_name: String,
) -> CentralConnection {
    let addr: SocketAddr = {
        if let Ok(a) = central_server.parse::<SocketAddr>() {
            a
        } else {
            let parts: Vec<&str> = central_server.rsplitn(2, ':').collect();
            let port: u16 = parts[0].parse().unwrap_or(8878);
            let host = parts.get(1).unwrap_or(&"127.0.0.1");
            std::net::ToSocketAddrs::to_socket_addrs(&(host.to_string(), port))
                .ok()
                .and_then(|mut a| a.next())
                .unwrap_or_else(|| "127.0.0.1:8878".parse().unwrap())
        }
    };

    CentralConnection {
        stream: None,
        server_addr: addr,
        node_id: uuid::Uuid::new_v4().to_string(),
        node_name,
        listen_port,
        udp_port,
        address: format!("{}:{}", local_ip(), listen_port),
    }
}