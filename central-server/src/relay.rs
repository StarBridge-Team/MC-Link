//! 中继注册/心跳/列表查询模块

use std::net::TcpStream;
use std::sync::Arc;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::write_packet;
use crate::types::*;

use mc_link_common::utils::now_secs;

pub fn handle_relay_register(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<RelayRegisterReq>(data) {
        let mut relays = state.relays.lock().unwrap();
        let mut addr_to_id = state.addr_to_id.lock().unwrap();

        let relay = RelayNode {
            id: req.id.clone(),
            name: req.name.clone(),
            address: req.address.clone(),
            last_seen: now_secs(),
            private: req.private,
            transit: req.transit,
            service_type: if req.service_type.is_empty() { "relay".to_string() } else { req.service_type.clone() },
            udp_port: req.udp_port,
        };

        let mode_tag = if req.private && !req.transit { " [私人/孤岛]" }
            else if req.private { " [私人]" }
            else if !req.transit { " [孤岛]" }
            else { "" };

        let st = relay.service_type.clone();
        state.topology.lock().unwrap().add_or_update_node(relay.clone());
        relays.insert(req.id.clone(), relay);
        addr_to_id.insert(src.to_string(), req.id.clone());
        drop(addr_to_id);

        if let Ok(clone) = stream.try_clone() {
            state.relay_streams.lock().unwrap().insert(req.id.clone(), Arc::new(std::sync::Mutex::new(clone)));
            log(LogLevel::Info, &format!("[{}] 保存 {} 的流连接", st, req.name));
        }

        log(LogLevel::Info, &format!("[{}] {} 注册在 {}{}", st, req.name, req.address, mode_tag));
        write_packet(stream, &[0x10, 0x00]).ok();
    }
}

pub fn handle_relay_heartbeat(state: &CentralState, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<RelayHeartbeatReq>(data) {
        let mut relays = state.relays.lock().unwrap();
        if let Some(relay) = relays.get_mut(&req.id) {
            relay.last_seen = now_secs();
            let mut topo = state.topology.lock().unwrap();
            if let Some(node) = topo.nodes.get_mut(&req.id) {
                node.last_seen = now_secs();
            }
        }
    }
}

pub fn handle_get_relays(stream: &mut TcpStream, state: &CentralState) {
    let relays = state.relays.lock().unwrap();
    let relay_list: Vec<&RelayNode> = relays.values().filter(|r| !r.private).collect();
    let mut response = vec![0x13];
    if let Ok(json) = serde_json::to_string(&relay_list) {
        response.extend_from_slice(json.as_bytes());
    }
    write_packet(stream, &response).ok();
}