use std::net::SocketAddr;
use std::net::TcpStream;

use serde::Deserialize;

use mc_link_common::log::{log, LogLevel};
use crate::protocol::{encrypt, write_packet};
use crate::RelayState;

#[derive(Deserialize)]
pub struct CreateRoomReq {
    pub room_name: String,
}

#[derive(Deserialize)]
pub struct FindRoomReq {
    pub room_name: String,
}

/// 处理创建房间请求（0x20）
pub fn handle_create_room(
    stream: &mut TcpStream,
    state: &RelayState,
    src: SocketAddr,
    data: &[u8],
) -> Option<String> {
    if let Ok(req) = serde_json::from_slice::<CreateRoomReq>(data) {
        let mut rooms = state.rooms.lock().unwrap();
        if rooms.contains_key(&req.room_name) {
            write_packet(stream, &[0x21, 0x01]).ok();
            return None;
        }
        rooms.insert(
            req.room_name.clone(),
            crate::RoomInfo {
                name: req.room_name.clone(),
                host_addr: src,
            },
        );
        log(
            LogLevel::Info,
            &format!("[房间/创建] 房间 {} (来自 {})", req.room_name, src),
        );
        write_packet(stream, &[0x21, 0x00]).ok();
        return Some(req.room_name);
    }
    None
}

/// 处理查找房间请求（0x22）
pub fn handle_find_room(
    stream: &mut TcpStream,
    state: &RelayState,
    _src: SocketAddr,
    data: &[u8],
) {
    if let Ok(req) = serde_json::from_slice::<FindRoomReq>(data) {
        let rooms = state.rooms.lock().unwrap();
        if let Some(room) = rooms.get(&req.room_name) {
            let response = serde_json::json!({"exists": true, "host_addr": room.host_addr.to_string()});
            let mut packet = vec![0x23];
            packet.extend_from_slice(response.to_string().as_bytes());
            write_packet(stream, &packet).ok();
        } else {
            write_packet(stream, &[0x23, 0x00]).ok();
        }
    }
}

/// 处理自定义协议 REGH（房主注册房间）
pub fn handle_regh(
    stream: &mut TcpStream,
    state: &RelayState,
    src: SocketAddr,
    data: &[u8],
    room: &str,
    password: &str,
    _encrypted: &[u8],
    room_len: usize,
    pass_len: usize,
) -> Option<String> {
    let mut rooms = state.rooms.lock().unwrap();
    if rooms.contains_key(room) {
        drop(rooms);
        log(LogLevel::Warn, &format!("[REGH] 房间已存在: {}", room));
        return None;
    }
    rooms.insert(
        room.to_string(),
        crate::RoomInfo {
            name: room.to_string(),
            host_addr: src,
        },
    );
    drop(rooms);
    log(
        LogLevel::Info,
        &format!("[REGH] 房间创建: {} (来自 {})", room, src),
    );

    let resp_enc = encrypt(b"REGH_OK", password);
    let mut resp_packet = Vec::new();
    resp_packet.push(room_len as u8);
    resp_packet.extend_from_slice(&data[1..1 + room_len]);
    resp_packet.push(pass_len as u8);
    resp_packet.extend_from_slice(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]);
    resp_packet.extend_from_slice(&resp_enc);
    write_packet(stream, &resp_packet).ok();
    Some(room.to_string())
}

/// 处理自定义协议 REGC（成员加入房间）
pub fn handle_regc(
    state: &RelayState,
    src: SocketAddr,
    data: &[u8],
    room: &str,
    password: &str,
    _encrypted: &[u8],
    room_len: usize,
    pass_len: usize,
) -> Option<String> {
    let rooms = state.rooms.lock().unwrap();
    let room_info = match rooms.get(room) {
        Some(info) => info.clone(),
        None => {
            drop(rooms);
            log(LogLevel::Warn, &format!("[REGC] 房间不存在: {}", room));
            // 错误响应通过调用方发送
            return None;
        }
    };
    drop(rooms);

    log(
        LogLevel::Info,
        &format!("[REGC] 成员加入房间: {} (来自 {})", room, src),
    );

    // 通知房主有成员加入
    let clients = state.clients.lock().unwrap();
    if let Some(host_stream) = clients.get(&room_info.host_addr.to_string()) {
        let member_join_enc = encrypt(b"MEMBER_JOIN", password);
        let mut member_join_packet = Vec::new();
        member_join_packet.push(room_len as u8);
        member_join_packet.extend_from_slice(&data[1..1 + room_len]);
        member_join_packet.push(pass_len as u8);
        member_join_packet.extend_from_slice(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]);
        member_join_packet.extend_from_slice(&member_join_enc);
        let mut host_stream = host_stream.lock().unwrap();
        write_packet(&mut host_stream, &member_join_packet).ok();
        log(
            LogLevel::Info,
            &format!("[房间/加入] 向房主发送成员加入通知: {}", room),
        );
    }
    drop(clients);
    Some(room.to_string())
}

/// 房间断开连接时的清理工作
pub fn cleanup_room(state: &RelayState, room_name: &str, addr: &SocketAddr, addr_str: &str) {
    let is_host = {
        let rooms = state.rooms.lock().unwrap();
        rooms
            .get(room_name)
            .map(|r| r.host_addr == *addr)
            .unwrap_or(false)
    };
    if is_host {
        let member_count;
        {
            let clients = state.clients.lock().unwrap();
            let member_addrs: Vec<String> = clients
                .keys()
                .filter(|ca| *ca != addr_str)
                .cloned()
                .collect();
            member_count = member_addrs.len();
            for ca in &member_addrs {
                if let Some(stream_arc) = clients.get(ca) {
                    if let Ok(s) = stream_arc.lock() {
                        let _ = s.shutdown(std::net::Shutdown::Both);
                    }
                }
            }
        }
        state.rooms.lock().unwrap().remove(room_name);
        log(
            LogLevel::Info,
            &format!(
                "[房间/注销] 房间 {} 已注销，{} 个成员连接已关闭",
                room_name, member_count
            ),
        );
    }
}

/// 向房主发送 MC_READY 通知
pub fn send_mc_ready_to_host(
    state: &RelayState,
    data: &[u8],
    room: &str,
    password: &str,
    room_len: usize,
    pass_len: usize,
) {
    let host_addr = {
        let rooms = state.rooms.lock().unwrap();
        rooms.get(room).map(|info| info.host_addr)
    };
    if let Some(host_addr) = host_addr {
        let clients = state.clients.lock().unwrap();
        if let Some(host_stream) = clients.get(&host_addr.to_string()) {
            let mc_ready_enc = encrypt(b"MC_READY", password);
            let mut mc_ready_packet = Vec::new();
            mc_ready_packet.push(room_len as u8);
            mc_ready_packet.extend_from_slice(&data[1..1 + room_len]);
            mc_ready_packet.push(pass_len as u8);
            mc_ready_packet.extend_from_slice(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]);
            mc_ready_packet.extend_from_slice(&mc_ready_enc);
            let mut host_stream = host_stream.lock().unwrap();
            write_packet(&mut host_stream, &mc_ready_packet).ok();
            log(
                LogLevel::Info,
                &format!("[房间/就绪] 向房主发送MC_READY通知: {}", room),
            );
        }
    }
}