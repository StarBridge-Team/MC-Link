//! 中继注册/心跳/列表查询模块

use std::net::TcpStream;
use std::sync::Arc;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::write_packet;
use crate::types::*;

use mc_link_common::utils::now_secs;

pub fn handle_relay_register(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<RelayRegisterReq>(data) {
        let mut relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
        let mut addr_to_id = state.addr_to_id.lock().unwrap_or_else(|e| e.into_inner());

        let relay = RelayNode {
            id: req.id.clone(),
            name: req.name.clone(),
            address: req.address.clone(),
            address_v6: req.address_v6.clone(),
            last_seen: now_secs(),
            private: req.private,
            transit: req.transit,
            idle: req.idle,
            service_type: if req.service_type.is_empty() { "relay".to_string() } else { req.service_type.clone() },
            udp_port: req.udp_port,
        };

        // 记录单调时钟，用于不受时间跳变影响的超时判断
        state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner()).insert(req.id.clone(), std::time::Instant::now());

        let mode_tag = if req.private && !req.transit { " [私人/孤岛]" }
            else if req.private { " [私人]" }
            else if req.idle { " [闲置]" }
            else if !req.transit { " [孤岛]" }
            else { "" };

        let st = relay.service_type.clone();

        // 同时更新两张拓扑表
        let mut mgr = state.topology_manager.write().unwrap_or_else(|e| e.into_inner());
        mgr.ipv4.add_or_update_node(relay.clone());
        if relay.address_v6.is_some() && relay.transit {
            mgr.mixed.add_or_update_node(relay.clone());
        }
        drop(mgr);

        relays.insert(req.id.clone(), relay);
        addr_to_id.insert(src.to_string(), req.id.clone());
        drop(addr_to_id);

        if let Ok(clone) = stream.try_clone() {
            state.relay_streams.lock().unwrap_or_else(|e| e.into_inner()).insert(req.id.clone(), Arc::new(std::sync::Mutex::new(clone)));
            log(LogLevel::Info, &format!("[{}] 保存 {} 的流连接", st, req.name));
        }

        log(LogLevel::Info, &format!("[{}] {} 注册在 {}{}", st, req.name, req.address, mode_tag));

        // 如果是客户端中继，下发限速配置
        if req.service_type == "client-relay" {
            let limits = state.client_relay_limits.lock().unwrap_or_else(|e| e.into_inner());
            let limits_json = serde_json::json!({
                "max_bandwidth_bps": limits.max_bandwidth_bps,
                "max_connections": limits.max_connections,
            });
            drop(limits);
            let mut resp = vec![0x10, 0x00];
            resp.extend_from_slice(&serde_json::to_vec(&limits_json).unwrap_or_default());
            write_packet(stream, &resp).ok();
            log(LogLevel::Info, &format!("[client-relay] `{}` 配置: {} Mbps / {} 连接",
                req.name, limits_json["max_bandwidth_bps"].as_u64().unwrap_or(0) / 1_000_000,
                limits_json["max_connections"].as_u64().unwrap_or(0)));
        } else {
            write_packet(stream, &[0x10, 0x00]).ok();
        }
    }
}

pub fn handle_relay_heartbeat(state: &CentralState, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<RelayHeartbeatReq>(data) {
        let mut relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(relay) = relays.get_mut(&req.id) {
            relay.last_seen = now_secs();
            // 更新单调时钟心跳记录
            state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner()).insert(req.id.clone(), std::time::Instant::now());
            let mut mgr = state.topology_manager.write().unwrap_or_else(|e| e.into_inner());
            if let Some(node) = mgr.ipv4.nodes.get_mut(&req.id) {
                node.last_seen = now_secs();
            }
            if let Some(node) = mgr.mixed.nodes.get_mut(&req.id) {
                node.last_seen = now_secs();
            }
        }
    }
}

pub fn handle_get_relays(stream: &mut TcpStream, state: &CentralState) {
    let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
    let relay_list: Vec<&RelayNode> = relays.values().filter(|r| !r.private && !r.idle).collect();
    let mut response = vec![0x13];
    if let Ok(json) = serde_json::to_string(&relay_list) {
        response.extend_from_slice(json.as_bytes());
    }
    write_packet(stream, &response).ok();
}

/// 获取混合中继列表（含 IPv6 的 client-relay）
pub fn handle_get_mixed_relays(stream: &mut TcpStream, state: &CentralState) {
    let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
    let relay_list: Vec<&RelayNode> = relays.values()
        .filter(|r| r.address_v6.is_some() && r.transit && !r.idle)
        .collect();
    let mut response = vec![0x18];
    if let Ok(json) = serde_json::to_string(&relay_list) {
        response.extend_from_slice(json.as_bytes());
    }
    write_packet(stream, &response).ok();
}

/// 处理中继模式切换（0x3A）
pub fn handle_relay_mode_switch(state: &CentralState, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<RelayModeSwitch>(data) {
        let mut relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(relay) = relays.get_mut(&req.id) {
            match req.mode.as_str() {
                "idle" => { relay.idle = true; relay.private = false; relay.transit = true; }
                "private" => { relay.idle = false; relay.private = true; relay.transit = false; }
                "normal" => { relay.idle = false; relay.private = false; relay.transit = true; }
                _ => return,
            }
            log(LogLevel::Info, &format!("[模式切换] {} → {}（闲置={}, 私人={}, 中转={}）",
                req.id, req.mode, relay.idle, relay.private, relay.transit));
            // 同步更新拓扑表
            let mut mgr = state.topology_manager.write().unwrap_or_else(|e| e.into_inner());
            mgr.ipv4.add_or_update_node(relay.clone());
            if relay.address_v6.is_some() && relay.transit {
                mgr.mixed.add_or_update_node(relay.clone());
            } else {
                mgr.mixed.remove_node(&req.id);
            }
        }
    }
}