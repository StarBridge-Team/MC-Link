use std::net::TcpStream;
use std::time::Duration;

use crate::config::local_ip;
use mc_link_common::log::{log, LogLevel};
use crate::protocol::{read_packet, write_packet};
use crate::RelayState;

/// 向中央服务器发送注册请求
pub fn send_register(stream: &mut TcpStream, state: &RelayState) {
    let address = state
        .report_address
        .clone()
        .unwrap_or_else(|| format!("{}:{}", local_ip(), state.relay_port));
    let req = serde_json::json!({
        "id": state.relay_id.clone(),
        "name": state.relay_name.lock().unwrap().clone(),
        "address": address,
        "private": state.private_mode,
        "transit": state.transit_mode,
        "udp_port": state.udp_port,
    });
    let mut packet = vec![0x10];
    packet.extend_from_slice(req.to_string().as_bytes());
    write_packet(stream, &packet).ok();
    log(LogLevel::Info, "发送注册请求");
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    match read_packet(stream) {
        Ok(buf) => {
            if buf.len() >= 2 && buf[0] == 0x10 && buf[1] == 0x00 {
                log(LogLevel::Info, "注册成功");
            }
        }
        Err(_) => {}
    }
}

/// 发送心跳包
pub fn send_heartbeat(stream: &mut TcpStream, state: &RelayState) -> bool {
    let req = serde_json::json!({"id": state.relay_id.clone()});
    let mut packet = vec![0x11];
    packet.extend_from_slice(req.to_string().as_bytes());
    write_packet(stream, &packet).is_ok()
}