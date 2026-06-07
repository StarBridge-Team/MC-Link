//! 房间管理模块

use std::net::TcpStream;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::write_packet;
use mc_link_common::utils::now_secs;
use crate::types::*;
use crate::path::assign_room_path;

pub fn handle_create_room(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<CreateRoomReq>(data) {
        let mut rooms = state.rooms.lock().unwrap();
        if rooms.contains_key(&req.room_name) {
            write_packet(stream, &[0x21, 0x01]).ok();
            return;
        }

        let room = RoomInfo {
            name: req.room_name.clone(),
            password_hash: req.password,
            host_relay_id: req.relay_id,
            created_at: now_secs(),
        };

        rooms.insert(req.room_name.clone(), room.clone());
        log(LogLevel::Info, &format!("房间创建: {} (来自 {}, 中继: {})", req.room_name, src, room.host_relay_id));

        let mut response = vec![0x21, 0x00];
        if let Ok(json) = serde_json::to_string(&room) {
            response.extend_from_slice(json.as_bytes());
        }
        write_packet(stream, &response).ok();
    }
}

pub fn handle_get_room(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<GetRoomReq>(data) {
        let room = {
            let rooms = state.rooms.lock().unwrap();
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
            let relays = state.relays.lock().unwrap();
            relays.keys()
                .find(|id| *id != &room.host_relay_id)
                .cloned()
                .unwrap_or_else(|| room.host_relay_id.clone())
        });

        assign_room_path(state, &req.room_name, &room.host_relay_id, &client_relay_id);

        let path = {
            let room_paths = state.room_paths.lock().unwrap();
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
            state.active_paths.lock().unwrap().get(&path_id).cloned()
        };

        let response_data = serde_json::to_string(&serde_json::json!({
            "exists": true,
            "room": room,
            "path": path.map(|p| serde_json::json!({
                "path_id": p.path_id,
                "hops": p.hops,
                "total_latency_ms": p.total_latency_ms,
            }))
        })).unwrap_or_default();

        let mut response = vec![0x23];
        response.extend_from_slice(response_data.as_bytes());
        write_packet(stream, &response).ok();
    }
}

pub fn handle_delete_room(stream: &mut TcpStream, state: &CentralState, src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<DeleteRoomReq>(data) {
        let mut rooms = state.rooms.lock().unwrap();
        if rooms.remove(&req.room_name).is_some() {
            state.room_paths.lock().unwrap().remove(&req.room_name);
            state.players.lock().unwrap().remove(&req.room_name);
            log(LogLevel::Info, &format!("房间已删除: {} (来自 {})", req.room_name, src));
            write_packet(stream, &[0x25, 0x00]).ok();
        } else {
            write_packet(stream, &[0x25, 0x01]).ok();
        }
    }
}

pub fn handle_join_room(stream: &mut TcpStream, state: &CentralState, _src: std::net::SocketAddr, data: &[u8]) {
    if let Ok(req) = serde_json::from_slice::<JoinRoomReq>(data) {
        let rooms = state.rooms.lock().unwrap();
        if !rooms.contains_key(&req.room_name) {
            write_packet(stream, &[0x28, 0x01]).ok();
            return;
        }
        let relay_id = req.relay_id.clone();
        let room_name = req.room_name.clone();
        let host_relay_id = rooms.get(&room_name).map(|r| r.host_relay_id.clone());
        drop(rooms);

        let player = PlayerInfo {
            name: req.player_name.clone(),
            role: req.role.clone(),
            joined_at: now_secs(),
        };

        let mut players = state.players.lock().unwrap();
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
            if let (Some(client_relay), Some(host_relay)) = (relay_id, host_relay_id) {
                assign_room_path(state, &room_name, &host_relay, &client_relay);
            }
        }

        write_packet(stream, &[0x28, 0x00]).ok();
    }
}

pub fn handle_list_players(stream: &mut TcpStream, state: &CentralState, data: &[u8]) {
    #[derive(serde::Deserialize)]
    struct ListPlayersReq { room_name: String }

    if let Ok(req) = serde_json::from_slice::<ListPlayersReq>(data) {
        let players = state.players.lock().unwrap();
        let list = players.get(&req.room_name).cloned().unwrap_or_default();
        drop(players);

        let mut response = vec![0x29];
        if let Ok(json) = serde_json::to_string(&list) {
            response.extend_from_slice(json.as_bytes());
        }
        write_packet(stream, &response).ok();
    }
}