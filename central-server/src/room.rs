//! 房间管理模块

use std::net::TcpStream;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::write_packet;
use mc_link_common::utils::now_secs;
use crate::types::*;
use crate::path::assign_room_path;

/// 限速常量（bps）
const SPEED_LIMIT_UNAUTH: u64 = 2_000_000;  // 2 Mbps
const SPEED_LIMIT_AUTH: u64 = 8_000_000;    // 8 Mbps

/// 判断房间是否走陶瓦联机（不限制）
fn is_terracotta_room(room_name: &str) -> bool {
    room_name.starts_with("U/")
}

/// 发送限速指令 (0x39) 给中继
fn send_speed_limit(stream: &mut TcpStream, room_name: &str, limit_bps: u64) {
    let payload = serde_json::json!({
        "room_name": room_name,
        "limit_bps": limit_bps,
    });
    let mut pkt = vec![0x39];
    pkt.extend_from_slice(payload.to_string().as_bytes());
    write_packet(stream, &pkt).ok();
}

/// 验证 token 并返回限速值 (bps)
fn validate_token_and_get_limit(token: Option<&str>, room_name: &str) -> u64 {
    if is_terracotta_room(room_name) {
        return 0; // 不限速
    }
    match token {
        Some(t) if !t.is_empty() => {
            match crate::account_api::verify(t) {
                Ok(resp) => {
                    log(LogLevel::Info, &format!("Token 验证成功: {}", resp.username));
                    SPEED_LIMIT_AUTH
                }
                Err(e) => {
                    log(LogLevel::Warn, &format!("Token 验证失败 ({}), 降级为未登录限速", e));
                    SPEED_LIMIT_UNAUTH
                }
            }
        }
        _ => {
            log(LogLevel::Info, "未提供 token, 使用未登录限速");
            SPEED_LIMIT_UNAUTH
        }
    }
}

pub fn handle_create_room(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<CreateRoomReq>(data) {
        let mut rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());
        if rooms.contains_key(&req.room_name) {
            write_packet(stream, &[0x21, 0x01]).ok();
            return;
        }

        // 验证 token 并确定限速
        let speed_limit = validate_token_and_get_limit(req.token.as_deref(), &req.room_name);
        let is_auth = speed_limit >= SPEED_LIMIT_AUTH;
        state.room_auth.lock().unwrap_or_else(|e| e.into_inner()).insert(req.room_name.clone(), is_auth);

        let room = RoomInfo {
            name: req.room_name.clone(),
            password_hash: req.password,
            host_relay_id: req.relay_id,
            created_at: now_secs(),
        };

        rooms.insert(req.room_name.clone(), room.clone());
        log(LogLevel::Info, &format!(
            "房间创建: {} (来自 {}, 中继: {}, 认证: {}, 限速: {} bps)",
            req.room_name, src, room.host_relay_id, is_auth, speed_limit
        ));

        // 向中继发送限速指令
        if let Some(stream_arc) = state.relay_streams.lock().unwrap_or_else(|e| e.into_inner()).get(&room.host_relay_id) {
            if let Ok(mut s) = stream_arc.lock() {
                send_speed_limit(&mut s, &req.room_name, speed_limit);
            }
        }

        // 响应中包含限速信息
        let mut response = vec![0x21, 0x00];
        let resp_data = serde_json::json!({
            "name": room.name,
            "password_hash": room.password_hash,
            "host_relay_id": room.host_relay_id,
            "created_at": room.created_at,
            "speed_limit_bps": speed_limit,
            "is_authenticated": is_auth,
        });
        if let Ok(json) = serde_json::to_string(&resp_data) {
            response.extend_from_slice(json.as_bytes());
        }
        write_packet(stream, &response).ok();
    }
}

pub fn handle_get_room(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<GetRoomReq>(data) {
        let room = {
            let rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());
            match rooms.get(&req.room_name) {
                Some(r) => r.clone(),
                None => {
                    drop(rooms);
                    log(LogLevel::Info, &format!("[房间/查询] 房间不存在: {} (来自 {})", req.room_name, src));
                    let response_data = serde_json::to_string(&serde_json::json!({"exists": false})).unwrap_or_default();
                    let mut response = vec![0x23];
                    response.extend_from_slice(response_data.as_bytes());
                    write_packet(stream, &response).ok();
                    return;
                }
            }
        };

        log(LogLevel::Info, &format!("[房间/加入] {} 加入房间 {} (来自 {})", room.host_relay_id, req.room_name, src));

        let client_relay_id = req.client_relay_id.unwrap_or_else(|| {
            let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
            relays.keys()
                .find(|id| *id != &room.host_relay_id)
                .cloned()
                .unwrap_or_else(|| room.host_relay_id.clone())
        });

        assign_room_path(state, &req.room_name, &room.host_relay_id, &client_relay_id, req.has_ipv6);

        let path = {
            let room_paths = state.room_paths.lock().unwrap_or_else(|e| e.into_inner());
            let path_id = match room_paths.get(&req.room_name) {
                Some(pid) => pid.clone(),
                None => {
                    drop(room_paths);
                    let response_data = serde_json::to_string(&serde_json::json!({
                        "exists": true,
                        "room": room,
                        "path": None::<serde_json::Value>
                    })).unwrap_or_default();
                    let mut response = vec![0x23];
                    response.extend_from_slice(response_data.as_bytes());
                    write_packet(stream, &response).ok();
                    return;
                }
            };
            drop(room_paths);
            state.active_paths.lock().unwrap_or_else(|e| e.into_inner()).get(&path_id).cloned()
        };

        // 获取限速信息
        let speed_limit = state.room_auth.lock().unwrap_or_else(|e| e.into_inner()).get(&req.room_name)
            .map(|&auth| if auth { SPEED_LIMIT_AUTH } else { SPEED_LIMIT_UNAUTH })
            .unwrap_or(if is_terracotta_room(&req.room_name) { 0 } else { SPEED_LIMIT_UNAUTH });
        let is_auth = speed_limit >= SPEED_LIMIT_AUTH;

        let response_data = serde_json::to_string(&serde_json::json!({
            "exists": true,
            "room": room,
            "path": path.map(|p| serde_json::json!({
                "path_id": p.path_id,
                "hops": p.hops,
                "total_latency_ms": p.total_latency_ms,
            })),
            "speed_limit_bps": speed_limit,
            "is_authenticated": is_auth,
        })).unwrap_or_default();

        let mut response = vec![0x23];
        response.extend_from_slice(response_data.as_bytes());
        write_packet(stream, &response).ok();
    }
}

pub fn handle_delete_room(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<DeleteRoomReq>(data) {
        let mut rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());
        if rooms.remove(&req.room_name).is_some() {
            state.room_paths.lock().unwrap_or_else(|e| e.into_inner()).remove(&req.room_name);
            state.players.lock().unwrap_or_else(|e| e.into_inner()).remove(&req.room_name);
            log(LogLevel::Info, &format!("房间已删除: {} (来自 {})", req.room_name, src));
            write_packet(stream, &[0x25, 0x00]).ok();
        } else {
            write_packet(stream, &[0x25, 0x01]).ok();
        }
    }
}

pub fn handle_join_room(stream: &mut TcpStream, state: &CentralState, _src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<JoinRoomReq>(data) {
        let rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());
        let room = match rooms.get(&req.room_name) {
            Some(r) => r.clone(),
            None => {
                drop(rooms);
                write_packet(stream, &[0x28, 0x01]).ok();
                return;
            }
        };
        // 验证密码
        if let Some(ref pass) = req.password {
            if room.password_hash != *pass {
                drop(rooms);
                write_packet(stream, &[0x28, 0x02]).ok();
                return;
            }
        }
        drop(rooms);
        let relay_id = req.relay_id.clone();
        let room_name = req.room_name.clone();
        let host_relay_id = room.host_relay_id.clone();

        let player = PlayerInfo {
            name: req.player_name.clone(),
            role: req.role.clone(),
            joined_at: now_secs(),
        };

        let mut players = state.players.lock().unwrap_or_else(|e| e.into_inner());
        let entry = players.entry(req.room_name.clone()).or_default();
        if let Some(existing) = entry.iter_mut().find(|p| p.name == req.player_name) {
            existing.role = req.role.clone();
            existing.joined_at = now_secs();
        } else {
            entry.push(player);
        }
        drop(players);

        log(LogLevel::Info, &format!("[玩家/加入] {} 加入房间 {} (角色: {})", req.player_name, req.room_name, req.role));

        if req.role == "member" {
            if let Some(client_relay) = relay_id {
                assign_room_path(state, &room_name, &host_relay_id, &client_relay, req.has_ipv6);
            }
        }

        write_packet(stream, &[0x28, 0x00]).ok();
    }
}

pub fn handle_list_players(stream: &mut TcpStream, state: &CentralState, data: &[u8]) {
    #[derive(serde::Deserialize)]
    struct ListPlayersReq { room_name: String }

    if let Ok(req) = serde_json::from_slice::<ListPlayersReq>(data) {
        let players = state.players.lock().unwrap_or_else(|e| e.into_inner());
        let list = players.get(&req.room_name).cloned().unwrap_or_default();
        drop(players);

        let mut response = vec![0x29];
        if let Ok(json) = serde_json::to_string(&list) {
            response.extend_from_slice(json.as_bytes());
        }
        write_packet(stream, &response).ok();
    }
}