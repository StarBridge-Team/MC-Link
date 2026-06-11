//! 协议分发 + 其他协议处理器 + 清理线程

use std::net::TcpStream;
use std::sync::Arc;
use std::thread;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::{read_packet, write_packet};
use mc_link_common::utils::now_secs;
use crate::types::*;
use crate::protocol::{HEARTBEAT_TIMEOUT, CLEANUP_INTERVAL};
use crate::relay::{handle_relay_register, handle_relay_heartbeat, handle_get_relays};
use crate::room::{handle_create_room, handle_get_room, handle_delete_room, handle_join_room, handle_list_players};
use crate::path::reroute_affected_paths;

/// 客户端连接循环
pub fn handle_client(stream: &mut TcpStream, state: Arc<CentralState>, addr: std::net::SocketAddr) {
    loop {
        match read_packet(stream) {
            Ok(data) => {
                if data.is_empty() { continue; }
                handle_packet(stream, &state, addr, &data);
            }
            Err(_) => break,
        }
    }

    let addr_str = addr.to_string();
    // 统一锁顺序: relays → addr_to_id (与 handle_relay_register 保持一致)
    let relay_id = {
        let _relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
        state.addr_to_id.lock().unwrap_or_else(|e| e.into_inner()).remove(&addr_str)
    };
    if let Some(ref relay_id) = relay_id {
        state.relay_streams.lock().unwrap_or_else(|e| e.into_inner()).remove(relay_id);
        state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner()).remove(relay_id);
        if let Some(relay) = state.relays.lock().unwrap_or_else(|e| e.into_inner()).remove(relay_id) {
            state.topology.lock().unwrap_or_else(|e| e.into_inner()).remove_node(relay_id);
            reroute_affected_paths(&state, relay_id);
            log(LogLevel::Warn, &format!("[中继/断开] {} 已离线，触发重路由", relay.name));
        }
    }
}

/// 协议命令分发
fn handle_packet(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if data.is_empty() { return; }
    let cmd = data[0];
    let payload = &data[1..];

    match cmd {
        0x10 => handle_relay_register(stream, state, src, payload),
        0x11 => handle_relay_heartbeat(state, payload),
        0x12 => handle_get_relays(stream, state),
        0x14 => handle_latency_report(state, payload),
        0x15 => handle_get_topology(stream, state),
        0x20 => handle_create_room(stream, state, src, payload),
        0x22 => handle_get_room(stream, state, src, payload),
        0x24 => handle_delete_room(stream, state, src, payload),
        0x26 => handle_join_room(stream, state, src, payload),
        0x27 => handle_list_players(stream, state, payload),
        0x31 => handle_probe_report(state, payload),
        0x37 => handle_path_broken(state, payload),
        0x38 => handle_traffic_report(state, payload),
        _ => log(LogLevel::Warn, &format!("未知命令: 0x{:02x}", cmd)),
    }
}

// ===== 延迟/探针/路径断裂/拓扑处理器 =====

pub fn handle_latency_report(state: &CentralState, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<LatencyReportReq>(data) {
        let key = format!("{}->{}", req.from_id, req.to_id);
        let mut latencies = state.latencies.lock().unwrap_or_else(|e| e.into_inner());
        let entry = latencies.entry(key).or_insert_with(|| LatencyEntry {
            from_id: req.from_id.clone(), to_id: req.to_id.clone(), latency_ms: 0, samples: Vec::new(),
        });
        entry.samples.push(req.latency_ms);
        if entry.samples.len() > 10 { entry.samples.remove(0); }
        entry.latency_ms = entry.samples.iter().sum::<u64>() / entry.samples.len() as u64;
    }
}

pub fn handle_probe_report(state: &CentralState, data: &[u8]) {
    if let Ok(report) = serde_json::from_slice::<ProbeReport>(data) {
        let metric = LinkMetric {
            node_a: report.from_id.clone(),
            node_b: report.to_id.clone(),
            latency_ms: report.latency_ms,
            packet_loss: report.packet_loss,
            last_updated: now_secs(),
        };
        state.topology.lock().unwrap_or_else(|e| e.into_inner()).add_or_update_metric(&report.from_id, &report.to_id, metric);
        log(LogLevel::Info, &format!("[探针/上报] {} -> {} 延迟={}ms 丢包={:.0}%",
            report.from_id, report.to_id, report.latency_ms, report.packet_loss * 100.0));
    }
}

pub fn handle_path_broken(state: &CentralState, data: &[u8]) {
    if let Ok(report) = serde_json::from_slice::<PathBrokenReport>(data) {
        log(LogLevel::Warn, &format!(
            "[路径/断裂] 中继 {} 上报: path={} 到中继 {} 连接断裂，触发快速重路由",
            &report.relay_id[..8.min(report.relay_id.len())],
            &report.path_id[..8.min(report.path_id.len())],
            &report.broken_relay_id[..8.min(report.broken_relay_id.len())]
        ));
        reroute_affected_paths(state, &report.broken_relay_id);
    }
}

pub fn handle_traffic_report(state: &CentralState, data: &[u8]) {
    if let Ok(report) = serde_json::from_slice::<TrafficReport>(data) {
        let mut reports = state.traffic_reports.lock().unwrap_or_else(|e| e.into_inner());
        let stats = reports.entry(report.relay_id.clone()).or_insert_with(|| TrafficStats {
            bytes_sent_total: 0,
            bytes_recv_total: 0,
            last_report: 0,
            current_connections: 0,
        });
        stats.bytes_sent_total = stats.bytes_sent_total.saturating_add(report.bytes_sent);
        stats.bytes_recv_total = stats.bytes_recv_total.saturating_add(report.bytes_recv);
        stats.last_report = now_secs();
        stats.current_connections = report.connections;
    }
}

pub fn handle_get_topology(stream: &mut TcpStream, state: &CentralState) {
    let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
    let latencies = state.latencies.lock().unwrap_or_else(|e| e.into_inner());

    let relay_list: Vec<&RelayNode> = relays.values().collect();
    let mut latency_matrix: std::collections::HashMap<String, std::collections::HashMap<String, u64>> = std::collections::HashMap::new();
    for (key, entry) in latencies.iter() {
        let parts: Vec<&str> = key.split("->").collect();
        if parts.len() == 2 {
            latency_matrix.entry(parts[0].to_string()).or_default().insert(parts[1].to_string(), entry.latency_ms);
        }
    }

    let resp = serde_json::to_string(&serde_json::json!({
        "relays": relay_list,
        "latency_matrix": latency_matrix,
    })).unwrap_or_default();

    let mut response = vec![0x16];
    response.extend_from_slice(resp.as_bytes());
    write_packet(stream, &response).ok();
}

// ===== 清理线程 =====

pub fn cleanup_thread(state: Arc<CentralState>) {
    loop {
        thread::sleep(CLEANUP_INTERVAL);

        let dead_ids = {
            let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
            let instants = state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner());
            let timeout = HEARTBEAT_TIMEOUT;
            let ids: Vec<String> = relays.keys()
                .filter(|id| {
                    instants.get(*id)
                        .map(|t| t.elapsed() > timeout)
                        .unwrap_or(true)
                })
                .map(|id| id.clone())
                .collect();
            // 释放 instants 锁后再操作
            ids
        };

        if !dead_ids.is_empty() {
            let mut relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
            let mut instants = state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner());
            for id in &dead_ids {
                if let Some(relay) = relays.remove(id) {
                    instants.remove(id);
                    log(LogLevel::Warn, &format!("清理离线中继: {} (ID: {})", relay.name, id));
                    state.topology.lock().unwrap_or_else(|e| e.into_inner()).remove_node(id);
                }
            }
            drop(relays);
            drop(instants);

            for id in &dead_ids {
                reroute_affected_paths(&state, id);
            }
        }

        let mut rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());
        let now = now_secs();
        let before = rooms.len();
        rooms.retain(|_, room| now - room.created_at < 86400);
        let removed = before - rooms.len();
        if removed > 0 {
            log(LogLevel::Info, &format!("清理过期房间: {} 个", removed));
        }
    }
}