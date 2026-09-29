//! 协议分发 + 其他协议处理器 + 清理线程

use std::net::TcpStream;
use std::sync::Arc;
use std::thread;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::{lock_or_recover, rwlock_read_or_recover, rwlock_write_or_recover};
use mc_link_common::protocol::{read_packet, write_packet};
use mc_link_common::utils::now_secs;
use crate::types::*;
use crate::protocol::{HEARTBEAT_TIMEOUT, CLEANUP_INTERVAL};
use crate::relay::{handle_relay_register, handle_relay_heartbeat, handle_get_relays, handle_get_mixed_relays, handle_relay_mode_switch};
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
        let _relays = lock_or_recover(&state.relays, "[清理] relays");
        lock_or_recover(&state.addr_to_id, "[断开] addr_to_id").remove(&addr_str)
    };
    if let Some(ref relay_id) = relay_id {
        lock_or_recover(&state.relay_streams, "[断开] relay_streams").remove(relay_id);
        lock_or_recover(&state.heartbeat_instants, "[清理] heartbeat_instants").remove(relay_id);
        if let Some(relay) = lock_or_recover(&state.relays, "[断开] relays_remove").remove(relay_id) {
            // 一次写锁清理两张拓扑表
            let mut mgr = rwlock_write_or_recover(&state.topology_manager, "[清理] topology_manager");
            mgr.ipv4.remove_node(relay_id);
            mgr.mixed.remove_node(relay_id);
            drop(mgr);
            // 延迟重路由（debounce），放入待处理队列
            lock_or_recover(&state.pending_reroute, "[清理] pending_reroute").push(relay_id.clone());
            log(LogLevel::Warn, &format!("[中继/断开] {} 已离线，排队重路由", relay.name));
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
        0x17 => handle_get_mixed_relays(stream, state),
        0x20 => handle_create_room(stream, state, src, payload),
        0x22 => handle_get_room(stream, state, src, payload),
        0x24 => handle_delete_room(stream, state, src, payload),
        0x26 => handle_join_room(stream, state, src, payload),
        0x27 => handle_list_players(stream, state, payload),
        0x31 => handle_probe_report(state, payload),
        0x37 => handle_path_broken(state, payload),
        0x38 => handle_traffic_report(state, payload),
        0x30 => handle_get_stats(stream, state),
        0x3A => handle_relay_mode_switch(state, payload),
        _ => log(LogLevel::Warn, &format!("未知命令: 0x{:02x}", cmd)),
    }
}

// ===== 延迟/探针/路径断裂/拓扑处理器 =====

pub fn handle_latency_report(state: &CentralState, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<LatencyReportReq>(data) {
        let key = format!("{}->{}", req.from_id, req.to_id);
        let mut latencies = lock_or_recover(&state.latencies, "[延迟] latencies");
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
        rwlock_write_or_recover(&state.topology_manager, "[清理] topology_manager").ipv4.add_or_update_metric(&report.from_id, &report.to_id, metric);
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
        let mut reports = lock_or_recover(&state.traffic_reports, "[统计] traffic_reports");
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
    let relays = lock_or_recover(&state.relays, "[清理] relays");
    let latencies = lock_or_recover(&state.latencies, "[拓扑] latencies_get");

    let relay_list: Vec<&RelayNode> = relays.values().collect();
    let mut latency_matrix: std::collections::HashMap<String, std::collections::HashMap<String, u64>> = std::collections::HashMap::new();
    for (key, entry) in latencies.iter() {
        let parts: Vec<&str> = key.split("->").collect();
        if parts.len() == 2 {
            latency_matrix.entry(parts[0].to_string()).or_default().insert(parts[1].to_string(), entry.latency_ms);
        }
    }

    // 获取混合拓扑视图的节点数
    let mixed_nodes = rwlock_read_or_recover(&state.topology_manager, "[拓扑] topology_manager").mixed.nodes.len();

    let resp = serde_json::to_string(&serde_json::json!({
        "relays": relay_list,
        "latency_matrix": latency_matrix,
        "mixed_node_count": mixed_nodes,
    })).unwrap_or_default();

    let mut response = vec![0x16];
    response.extend_from_slice(resp.as_bytes());
    write_packet(stream, &response).ok();
}

/// 生成完整统计快照
pub fn compute_stats(state: &CentralState) -> StatsSnapshot {
    let relays = lock_or_recover(&state.relays, "[清理] relays");
    let rooms = lock_or_recover(&state.rooms, "[统计] rooms_stats");
    let players = lock_or_recover(&state.players, "[统计] players");
    let paths = lock_or_recover(&state.active_paths, "[统计] active_paths");
    let traffic = lock_or_recover(&state.traffic_reports, "[统计] traffic_reports");
    let auth = lock_or_recover(&state.room_auth, "[统计] room_auth");
    let instants = lock_or_recover(&state.heartbeat_instants, "[清理] heartbeat_instants");
    let mgr = rwlock_read_or_recover(&state.topology_manager, "[拓扑] topology_manager");

    let heartbeat_timeout = crate::protocol::HEARTBEAT_TIMEOUT;

    let relay_count = relays.len();
    let online_relay_count = instants.iter()
        .filter(|(_, t)| t.elapsed() <= heartbeat_timeout)
        .count();
    let client_relay_count = relays.values()
        .filter(|r| r.service_type == "client-relay")
        .count();
    let online_client_relay_count = relays.values()
        .filter(|r| r.service_type == "client-relay")
        .filter(|r| instants.get(&r.id).map(|t| t.elapsed() <= heartbeat_timeout).unwrap_or(false))
        .count();

    let room_count = rooms.len();
    let player_count: usize = players.values().map(|v| v.len()).sum();
    let mut player_roles = std::collections::HashMap::new();
    for plist in players.values() {
        for p in plist {
            *player_roles.entry(p.role.clone()).or_insert(0) += 1;
        }
    }

    let path_count = paths.len();
    let total_traffic_bytes: u64 = traffic.values().map(|t| t.bytes_sent_total + t.bytes_recv_total).sum();
    let traffic_in: u64 = traffic.values().map(|t| t.bytes_recv_total).sum();
    let traffic_out: u64 = traffic.values().map(|t| t.bytes_sent_total).sum();

    let topology_ipv4_nodes = mgr.ipv4.nodes.len();
    let topology_mixed_nodes = mgr.mixed.nodes.len();

    let active_connections: usize = traffic.values().map(|t| t.current_connections as usize).sum();
    let room_auth_count = auth.len();
    let uptime_secs = state.server_start_time.elapsed().as_secs();

    StatsSnapshot {
        timestamp: now_secs(),
        relay_count,
        online_relay_count,
        client_relay_count,
        online_client_relay_count,
        room_count,
        player_count,
        player_roles,
        path_count,
        total_traffic_bytes,
        traffic_in,
        traffic_out,
        topology_ipv4_nodes,
        topology_mixed_nodes,
        active_connections,
        room_auth_count,
        uptime_secs,
    }
}

/// 处理 0x30 统计请求
pub fn handle_get_stats(stream: &mut TcpStream, state: &CentralState) {
    let snapshot = compute_stats(state);
    let json = serde_json::to_string(&snapshot).unwrap_or_default();
    let mut resp = vec![0x30];
    resp.extend_from_slice(json.as_bytes());
    write_packet(stream, &resp).ok();
}

/// 批量处理待重路由队列（debounce + 去重）
pub fn process_pending_reroutes(state: &CentralState) {
    let pending = lock_or_recover(&state.pending_reroute, "[清理] pending_reroute").drain(..).collect::<Vec<_>>();
    if pending.is_empty() { return; }
    let mut unique = std::collections::HashSet::new();
    for id in &pending { unique.insert(id.clone()); }
    for id in unique {
        reroute_affected_paths(state, &id);
    }
}

// ===== 清理线程 =====

pub fn cleanup_thread(state: Arc<CentralState>) {
    loop {
        thread::sleep(CLEANUP_INTERVAL);

        // 处理待重路由队列
        process_pending_reroutes(&state);

        let dead_ids = {
            let relays = lock_or_recover(&state.relays, "[清理] relays");
            let instants = lock_or_recover(&state.heartbeat_instants, "[清理] heartbeat_instants");
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
            let mut relays = lock_or_recover(&state.relays, "[清理] relays");
            let mut instants = lock_or_recover(&state.heartbeat_instants, "[清理] heartbeat_instants");
            for id in &dead_ids {
                if let Some(relay) = relays.remove(id) {
                    instants.remove(id);
                    log(LogLevel::Warn, &format!("清理离线中继: {} (ID: {})", relay.name, id));
                    // 清理两张拓扑表
                    let mut mgr = rwlock_write_or_recover(&state.topology_manager, "[清理] topology_manager");
                    mgr.ipv4.remove_node(id);
                    mgr.mixed.remove_node(id);
                    drop(mgr);
                    // 放入重路由队列
                    lock_or_recover(&state.pending_reroute, "[清理] pending_reroute").push(id.clone());
                }
            }
            drop(relays);
            drop(instants);
        }

        let mut rooms = lock_or_recover(&state.rooms, "[清理] rooms_retain");
        let now = now_secs();
        let before = rooms.len();
        rooms.retain(|_, room| now - room.created_at < 86400);
        let removed = before - rooms.len();
        if removed > 0 {
            log(LogLevel::Info, &format!("清理过期房间: {} 个", removed));
        }
    }
}