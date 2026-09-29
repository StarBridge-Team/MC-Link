use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

pub use crate::protocol::{PathAssignment, RoomRoute, PathHop};

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::lock_or_recover;
use crate::protocol::{hex_to_path_id, write_packet, TunnelFrameHeader};
use crate::RelayState;

/// 从 PathHop 中获取首选地址（有 IPv6 且本地支持时优先）
fn hop_preferred_address(hop: &PathHop) -> SocketAddr {
    // 尝试 IPv6 地址
    if let Some(ref v6) = hop.address_v6 {
        if let Ok(addr) = v6.parse::<SocketAddr>() {
            return addr;
        }
    }
    // 回退到 IPv4 地址
    hop.address.parse::<SocketAddr>().unwrap_or_else(|_| {
        // 这不应发生，若解析失败仍尝试 v4 地址
        "127.0.0.1:0".parse().unwrap()
    })
}

/// 向中央服务器上报路径断裂（快速触发重路由）
pub fn report_path_broken(state: &RelayState, path_id: &str, broken_relay_id: &str) {
    let central_addr = match state.central_addr {
        Some(addr) => addr,
        None => return,
    };

    let report = serde_json::json!({
        "relay_id": state.relay_id,
        "path_id": path_id,
        "broken_relay_id": broken_relay_id,
    });

    log(
        LogLevel::Warn,
        &format!(
            "[路径/断裂] 上报中央: path={} broken={}",
            &path_id[..8.min(path_id.len())],
            &broken_relay_id[..8.min(broken_relay_id.len())]
        ),
    );

    if let Ok(mut stream) = TcpStream::connect_timeout(&central_addr, Duration::from_secs(3)) {
        let mut packet = vec![0x37];
        packet.extend_from_slice(report.to_string().as_bytes());
        let _ = write_packet(&mut stream, &packet);
    }
}

/// 处理隧道帧（0x34）- 接收并转发多跳隧道数据
pub fn handle_tunnel_frame(state: &RelayState, src: SocketAddr, data: &[u8]) {
    let packet_data = &data[1..];
    // 统计接收字节（不含命令字节）
    state.traffic_bytes_recv.fetch_add(packet_data.len() as u64, Ordering::Relaxed);

    let mut header = match TunnelFrameHeader::decode(packet_data) {
        Some(h) => h,
        None => {
            log(LogLevel::Warn, &format!("[隧道] 无效的隧道帧头 (来自 {})", src));
            return;
        }
    };

    let path_id_str = hex::encode(header.path_id);
    let path = {
        let table = lock_or_recover(&state.path_table, "[路径] path_table_get");
        match table.get(&path_id_str) {
            Some(p) => p.clone(),
            None => {
                log(
                    LogLevel::Warn,
                    &format!("[隧道] 未知路径 {} (来自 {})", &path_id_str[..8], src),
                );
                return;
            }
        }
    };

    let payload = &packet_data[TunnelFrameHeader::SIZE..];

    // 用本中继在路径中的位置(cur_pos)判断转发/终点
    let cur_pos = match path.hops.iter().position(|h| h.node_id == state.relay_id) {
        Some(p) => p,
        None => {
            log(
                LogLevel::Warn,
                &format!("[隧道] 本中继不在路径 {} 中", &path_id_str[..8]),
            );
            return;
        }
    };

    let is_terminal = if header.direction() == 0 {
        cur_pos + 1 >= path.hops.len()
    } else {
        cur_pos == 0
    };

    if !is_terminal {
        // 转发到下一跳（向其它中继服务器发包）
        let next_idx = if header.direction() == 0 {
            cur_pos + 1
        } else {
            cur_pos - 1
        };
        let next_hop = &path.hops[next_idx];
        let next_addr = hop_preferred_address(next_hop);

        header.hop_index = next_idx as u8;
        let mut packet = vec![0x34];
        packet.extend_from_slice(&header.encode());
        packet.extend_from_slice(payload);

        let mut peers = lock_or_recover(&state.peer_connections, "[隧道] peer_connections_peek");
        if let Some(stream_arc) = peers.get(&next_hop.address) {
            if let Ok(mut s) = stream_arc.lock() {
                if write_packet(&mut s, &packet).is_ok() {
                    return;
                }
            }
            peers.remove(&next_hop.address);
        }
        drop(peers);

        match TcpStream::connect(next_addr) {
            Ok(mut stream) => {
                if write_packet(&mut stream, &packet).is_ok() {
                    if let Ok(clone) = stream.try_clone() {
                        lock_or_recover(&state.peer_connections, "[隧道] peer_connections_insert")
                            .insert(next_hop.address.clone(), Arc::new(std::sync::Mutex::new(clone)));
                    }
                }
            }
            Err(e) => {
                log(
                    LogLevel::Error,
                    &format!("[隧道] 连接下一跳 {} 失败: {}", next_hop.address, e),
                );
                report_path_broken(state, &path_id_str, &next_hop.node_id);
            }
        }
    } else if header.direction() == 1 {
        // reverse 终点(房主侧): 处理回包（收到其它中继服务器的包）
        if !payload.is_empty() {
            handle_reverse_tunnel_payload(state, payload, &path);
        }
    } else {
        // forward 终点(成员侧): payload 投递到本地成员（向MC Link成员客户端发包）
        if payload.is_empty() {
            return;
        }
        let clients = lock_or_recover(&state.clients, "[隧道] clients_forward");
        for (addr, stream_arc) in clients.iter() {
            if let Ok(mut s) = stream_arc.lock() {
                if write_packet(&mut s, payload).is_ok() {
                    log(
                        LogLevel::Info,
                        &format!("[隧道/终点] 投递到成员 {}: {} 字节", addr, payload.len()),
                    );
                    break;
                }
            }
        }
    }
}

/// 处理路径分配（0x35）
pub fn handle_path_assignment(state: &RelayState, data: &[u8]) {
    let packet_data = &data[1..];
    match serde_json::from_slice::<PathAssignment>(packet_data) {
        Ok(assignment) => {
            let mut table = state.path_table.lock().unwrap_or_else(|e| e.into_inner());
            table.insert(assignment.path_id.clone(), assignment.clone());
            log(
                LogLevel::Info,
                &format!(
                    "[路径/分配] 新路径 {}: {}",
                    &assignment.path_id[..8],
                    assignment
                        .hops
                        .iter()
                        .map(|h| h.node_id.as_str())
                        .collect::<Vec<_>>()
                        .join(" -> ")
                ),
            );
        }
        Err(_) => log(LogLevel::Warn, "[路径/分配] 路径分配数据解析失败"),
    }
}

/// 处理房间路由（0x36）
pub fn handle_room_route(state: &RelayState, data: &[u8]) {
    let packet_data = &data[1..];
    match serde_json::from_slice::<RoomRoute>(packet_data) {
        Ok(route) => {
            if route.direction == "reverse" {
                let mut rev = lock_or_recover(&state.reverse_route_map, "[路由] reverse_route_map_insert");
                rev.entry(route.room_name.clone()).or_default().push(route.clone());
                log(
                    LogLevel::Info,
                    &format!(
                        "[路由/反向] 房间 {} 目标中继={}",
                        route.room_name,
                        &route.target_relay_id[..8]
                    ),
                );
            } else {
                let mut fwd = lock_or_recover(&state.room_route_map, "[路由] room_route_map");
                fwd.insert(route.room_name.clone(), route.clone());
                log(
                    LogLevel::Info,
                    &format!(
                        "[路由/正向] 房间 {} 目标中继={}",
                        route.room_name,
                        &route.target_relay_id[..8]
                    ),
                );
            }
        }
        Err(_) => log(LogLevel::Warn, "[路由/房间] 房间路由数据解析失败"),
    }
}

/// 发送隧道帧到下一跳（向其它中继服务器发包）
pub fn send_tunnel_direct(
    state: &RelayState,
    packet: &[u8],
    path: &PathAssignment,
    dir: u8,
    cur_pos: usize,
) -> bool {
    let next_idx = if dir == 0 {
        cur_pos + 1
    } else {
        cur_pos.saturating_sub(1)
    };
    if next_idx >= path.hops.len() {
        return false;
    }
    let next_hop = &path.hops[next_idx];
    let next_addr = hop_preferred_address(next_hop);

    let path_id_bytes = match hex_to_path_id(&path.path_id) {
        Some(b) => b,
        None => return false,
    };

    let mut header = TunnelFrameHeader {
        path_id: path_id_bytes,
        hop_index: cur_pos as u8,
        total_hops: path.hops.len() as u8,
        flags: 0,
        seq_no: 0,
        payload_len: packet.len() as u16,
    };
    header.set_direction(dir);

    let mut frame_data = header.encode();
    frame_data.extend_from_slice(packet);
    let mut out = vec![0x34];
    out.extend_from_slice(&frame_data);

    // 统计发送字节
    state.traffic_bytes_sent.fetch_add(out.len() as u64, Ordering::Relaxed);

    let mut peers = lock_or_recover(&state.peer_connections, "[发送] peer_connections_peek");
    if let Some(stream_arc) = peers.get(&next_hop.address) {
        if let Ok(mut s) = stream_arc.lock() {
            if write_packet(&mut s, &out).is_ok() {
                return true;
            }
        }
        peers.remove(&next_hop.address);
    }
    let next_addr_clone = next_hop.address.clone();
    drop(peers);

    match TcpStream::connect(next_addr) {
        Ok(mut stream) => {
            if write_packet(&mut stream, &out).is_ok() {
                if let Ok(clone) = stream.try_clone() {
                    if let Ok(mut peers) = state.peer_connections.lock() {
                        peers.insert(next_addr_clone, Arc::new(std::sync::Mutex::new(clone)));
                    }
                }
                true
            } else {
                false
            }
        }
        Err(e) => {
            log(
                LogLevel::Error,
                &format!("[隧道/发送] 连接下一跳 {} 失败: {}", next_hop.address, e),
            );
            report_path_broken(state, &path.path_id, &next_hop.node_id);
            false
        }
    }
}

/// 通过隧道将数据包发送到目标中继（多跳发包）
pub fn tunnel_packet_to_relay(state: &RelayState, packet: &[u8], room: &str) -> bool {
    let route = match lock_or_recover(&state.room_route_map, "[隧道] room_route_map_get").get(room) {
        Some(r) => r.clone(),
        None => return false,
    };

    let full_path = {
        let table = lock_or_recover(&state.path_table, "[隧道] path_table_packet");
        match table.get(&route.path_id) {
            Some(p) => p.clone(),
            None => return false,
        }
    };
    if full_path.hops.len() < 2 {
        return false;
    }

    let cur_pos = full_path
        .hops
        .iter()
        .position(|h| h.node_id == state.relay_id)
        .unwrap_or(!0);
    if cur_pos == !0 {
        return false;
    }
    let target_pos = full_path
        .hops
        .iter()
        .position(|h| h.node_id == route.target_relay_id)
        .unwrap_or(!0);
    if target_pos == !0 || cur_pos == target_pos {
        return false;
    }

    let dir = if cur_pos < target_pos { 0u8 } else { 1u8 };
    send_tunnel_direct(state, packet, &full_path, dir, cur_pos)
}

/// 通过隧道将数据包转发到远程成员（多跳发包到其它中继）
pub fn tunnel_to_remote_members(state: &RelayState, packet: &[u8], room: &str) {
    let routes = {
        let rev = lock_or_recover(&state.reverse_route_map, "[隧道] reverse_route_map_get");
        rev.get(room).cloned()
    };
    let routes = match routes {
        Some(r) => r,
        None => return,
    };

    for route in &routes {
        let full_path = {
            let table = lock_or_recover(&state.path_table, "[隧道] path_table_member");
            table.get(&route.path_id).cloned()
        };
        let full_path = match full_path {
            Some(p) => p,
            None => continue,
        };

        let cur_pos = full_path
            .hops
            .iter()
            .position(|h| h.node_id == state.relay_id)
            .unwrap_or(!0);
        if cur_pos == !0 {
            continue;
        }
        let target_pos = full_path
            .hops
            .iter()
            .position(|h| h.node_id == route.target_relay_id)
            .unwrap_or(!0);
        if target_pos == !0 || cur_pos == target_pos {
            continue;
        }

        let dir = if cur_pos < target_pos { 0u8 } else { 1u8 };
        send_tunnel_direct(state, packet, &full_path, dir, cur_pos);
    }
}

/// reverse 终点(房主侧中继): 处理隧道负载（收到其它中继服务器的包）
fn handle_reverse_tunnel_payload(state: &RelayState, payload: &[u8], path: &PathAssignment) {
    if payload.len() < 4 {
        return;
    }
    let room_len = payload[0] as usize;
    if room_len == 0 || room_len > 100 || payload.len() < 1 + room_len + 1 {
        return;
    }
    let room = String::from_utf8_lossy(&payload[1..1 + room_len]).to_string();
    let pass_len = payload[1 + room_len] as usize;
    if pass_len == 0 || pass_len > 100 || payload.len() < 1 + room_len + 1 + pass_len + 4 {
        return;
    }
    let password =
        String::from_utf8_lossy(&payload[1 + room_len + 1..1 + room_len + 1 + pass_len])
            .to_string();
    let encrypted = &payload[1 + room_len + 1 + pass_len..];

    let decrypted = match crate::protocol::decrypt(encrypted, &password) {
        Some(d) => d,
        None => return,
    };
    if decrypted.len() < 4 {
        return;
    }
    let command = &decrypted[0..4];

    if command == b"REGC" {
        let rooms = lock_or_recover(&state.rooms, "[隧道] rooms_reverse");
        if !rooms.contains_key(&room) {
            return;
        }
        drop(rooms);

        // REGC_OK 隧道回成员中继 (forward)
        let resp_enc = crate::protocol::encrypt(b"REGC_OK", &password);
        let mut resp_pkt = Vec::new();
        resp_pkt.push(room_len as u8);
        resp_pkt.extend_from_slice(&payload[1..1 + room_len]);
        resp_pkt.push(pass_len as u8);
        resp_pkt.extend_from_slice(&payload[1 + room_len + 1..1 + room_len + 1 + pass_len]);
        resp_pkt.extend_from_slice(&resp_enc);

        let cur_pos = path
            .hops
            .iter()
            .position(|h| h.node_id == state.relay_id)
            .unwrap_or(!0);
        if cur_pos != !0 && cur_pos + 1 < path.hops.len() {
            send_tunnel_direct(state, &resp_pkt, path, 0, cur_pos);
            log(
                LogLevel::Info,
                &format!("[隧道/REGC] REGC_OK 隧道回成员中继 (房间 {})", room),
            );
        }

        // MEMBER_JOIN 通知房主
        let join_enc = crate::protocol::encrypt(b"MEMBER_JOIN", &password);
        let mut join_pkt = Vec::new();
        join_pkt.push(room_len as u8);
        join_pkt.extend_from_slice(&payload[1..1 + room_len]);
        join_pkt.push(pass_len as u8);
        join_pkt.extend_from_slice(&payload[1 + room_len + 1..1 + room_len + 1 + pass_len]);
        join_pkt.extend_from_slice(&join_enc);

        let host_addr = {
            lock_or_recover(&state.rooms, "[隧道] rooms_MEMBER_JOIN")
                .get(&room)
                .map(|r| r.host_addr)
        };
        if let Some(addr) = host_addr {
            if let Some(stream_arc) = lock_or_recover(&state.clients, "[隧道] clients_MEMBER_JOIN").get(&addr.to_string()) {
                let mut s = lock_or_recover(stream_arc, "[隧道] host_stream_MEMBER_JOIN");
                crate::protocol::write_packet(&mut s, &join_pkt).ok();
            }
        }
    } else if command == b"DATA" {
        // DATA 直送房主（回包给MC Link房主客户端）
        let host_addr = {
            lock_or_recover(&state.rooms, "[隧道] rooms_DATA")
                .get(&room)
                .map(|r| r.host_addr)
        };
        if let Some(addr) = host_addr {
            if let Some(stream_arc) = lock_or_recover(&state.clients, "[隧道] clients_DATA").get(&addr.to_string()) {
                let mut s = lock_or_recover(stream_arc, "[隧道] host_stream_DATA");
                crate::protocol::write_packet(&mut s, payload).ok();
            }
        }
    } else if command == b"MC_READY" {
        // MC_READY 直送房主（通知房主成员就绪）
        let host_addr = {
            lock_or_recover(&state.rooms, "[隧道] rooms_MC_READY")
                .get(&room)
                .map(|r| r.host_addr)
        };
        if let Some(addr) = host_addr {
            if let Some(stream_arc) = lock_or_recover(&state.clients, "[隧道] clients_MC_READY").get(&addr.to_string()) {
                let mut s = lock_or_recover(stream_arc, "[隧道] host_stream_MC_READY");
                crate::protocol::write_packet(&mut s, payload).ok();
                log(
                    LogLevel::Info,
                    &format!("[隧道/MC_READY] 转发到房主: {}", room),
                );
            }
        }
    }
}

// ===== 中央服务器指令处理 =====

/// 处理中央服务器下发的指令（如 0x39 限速）
pub fn handle_central_command(state: &RelayState, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let cmd = data[0];
    let payload = &data[1..];

    match cmd {
        0x39 => {
            handle_speed_limit(state, payload);
        }
        _ => {
            log(LogLevel::Info, &format!("[中央/指令] 未知指令: 0x{:02x}", cmd));
        }
    }
}

/// 处理限速指令 (0x39)
fn handle_speed_limit(state: &RelayState, data: &[u8]) {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct SpeedLimitPayload {
        room_name: String,
        limit_bps: u64,
    }

    if let Ok(payload) = serde_json::from_slice::<SpeedLimitPayload>(data) {
        let mut limits = lock_or_recover(&state.room_speed_limits, "[限速] room_speed_limits");
        limits.insert(payload.room_name.clone(), payload.limit_bps);
        log(
            LogLevel::Info,
            &format!("[限速] 房间 {} 限速 {} bps", payload.room_name, payload.limit_bps),
        );
    }
}