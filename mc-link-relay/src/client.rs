use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::now_secs;
use crate::config::is_bandwidth_allowed;
use crate::protocol::{decrypt, encrypt, is_custom_protocol, read_packet, write_packet};
use crate::relay::{tunnel_packet_to_relay, tunnel_to_remote_members};
use crate::room::{cleanup_room, handle_create_room, handle_find_room, handle_regc, handle_regh, send_mc_ready_to_host};
use crate::RelayState;

/// 处理客户端连接主循环
pub fn handle_client(
    mut stream: TcpStream,
    state: Arc<RelayState>,
    addr: SocketAddr,
    bandwidth_limit: Option<f64>,
) {
    let addr_str = addr.to_string();
    let client_clone = match stream.try_clone() {
        Ok(c) => c,
        Err(e) => {
            log(LogLevel::Error, &format!("克隆客户端流失败: {}", e));
            return;
        }
    };
    client_clone.set_nodelay(true).ok();
    match state.clients.lock() {
        Ok(mut clients) => {
            clients.insert(addr_str.clone(), Arc::new(std::sync::Mutex::new(client_clone)));
        }
        Err(_) => {
            log(LogLevel::Error, "客户端列表锁中毒，无法插入");
            return;
        }
    }

    state.traffic_connections.fetch_add(1, Ordering::Relaxed);

    // 带宽限制跟踪
    let mut total_bytes_sent: u64 = 0;
    let mut last_bandwidth_check: u64 = crate::now_secs();

    let mut host_room: Option<String> = None;

    loop {
        match read_packet(&mut stream) {
            Ok(data) => {
                if data.is_empty() {
                    continue;
                }
                if !is_bandwidth_allowed(
                    &mut total_bytes_sent,
                    &mut last_bandwidth_check,
                    data.len(),
                    bandwidth_limit,
                    now_secs(),
                ) {
                    continue;
                }
                let room = handle_packet(&mut stream, &state, addr, &data);
                if let Some(r) = room {
                    host_room = Some(r);
                }
            }
            Err(_) => break,
        }
    }

    // 客户端断开，清理
    state.traffic_connections.fetch_sub(1, Ordering::Relaxed);
    if let Ok(mut clients) = state.clients.lock() {
        clients.remove(&addr_str);
    }
    if let Some(room_name) = host_room {
        cleanup_room(&state, &room_name, &addr, &addr_str);
    }
}

/// 包分发器
fn handle_packet(stream: &mut TcpStream, state: &RelayState, src: SocketAddr, data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }

    if is_custom_protocol(data) {
        return handle_custom_protocol(stream, state, src, data);
    }

    let cmd = data[0];
    let payload = &data[1..];

    match cmd {
        0x41 => {
            handle_test_packet(state, src, data);
            None
        }
        0x20 => handle_create_room(stream, state, src, payload),
        0x22 => {
            handle_find_room(stream, state, src, payload);
            None
        }
        0x40 => {
            handle_data(state, src, payload);
            None
        }
        0x32 => {
            handle_ping(stream);
            None
        }
        0x34 => {
            crate::relay::handle_tunnel_frame(state, src, data);
            None
        }
        0x35 => {
            crate::relay::handle_path_assignment(state, data);
            None
        }
        0x36 => {
            crate::relay::handle_room_route(state, data);
            None
        }
        _ => None,
    }
}

/// 处理自定义协议（MC Link 客户端加密协议）
fn handle_custom_protocol(
    stream: &mut TcpStream,
    state: &RelayState,
    src: SocketAddr,
    data: &[u8],
) -> Option<String> {
    let room_len = data[0] as usize;
    let room = String::from_utf8_lossy(&data[1..1 + room_len]).to_string();
    let pass_len = data[1 + room_len] as usize;
    let password =
        String::from_utf8_lossy(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]).to_string();
    let encrypted = &data[1 + room_len + 1 + pass_len..];

    let decrypted = match decrypt(encrypted, &password) {
        Some(d) => d,
        None => return None,
    };
    if decrypted.len() < 4 {
        return None;
    }
    let command = &decrypted[0..4];

    // REGH（房主注册房间）-> 委托给 room 模块
    if command == b"REGH" {
        return handle_regh(stream, state, src, data, &room, &password, encrypted, room_len, pass_len);
    }

    // REGC（成员加入房间）-> 委托给 room 模块
    if command == b"REGC" {
        let result = handle_regc(state, src, data, &room, &password, encrypted, room_len, pass_len);
        // 无论成功与否，都发送 REGC_OK 或 ERRR 响应
        if result.is_some() {
            let resp_enc = encrypt(b"REGC_OK", &password);
            let mut resp_packet = Vec::new();
            resp_packet.push(room_len as u8);
            resp_packet.extend_from_slice(&data[1..1 + room_len]);
            resp_packet.push(pass_len as u8);
            resp_packet.extend_from_slice(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]);
            resp_packet.extend_from_slice(&resp_enc);
            write_packet(stream, &resp_packet).ok();
        } else {
            let err_enc = encrypt(b"ERRR", &password);
            let mut resp_packet = Vec::new();
            resp_packet.push(room_len as u8);
            resp_packet.extend_from_slice(&data[1..1 + room_len]);
            resp_packet.push(pass_len as u8);
            resp_packet.extend_from_slice(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]);
            resp_packet.extend_from_slice(&err_enc);
            write_packet(stream, &resp_packet).ok();
        }
        return result;
    }

    // MC_READY -> 通知房主
    if command == b"MC_READY" {
        send_mc_ready_to_host(state, data, &room, &password, room_len, pass_len);
        return None;
    }

    // DATA（数据转发）
    if command == b"DATA" {
        handle_custom_data(state, src, data, &room, &password, encrypted, room_len, pass_len);
        return None;
    }

    // 未知命令 -> 发送错误响应
    let resp_enc = encrypt(b"ERRR", &password);
    let mut resp_packet = Vec::new();
    resp_packet.push(room_len as u8);
    resp_packet.extend_from_slice(&data[1..1 + room_len]);
    resp_packet.push(pass_len as u8);
    resp_packet.extend_from_slice(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]);
    resp_packet.extend_from_slice(&resp_enc);
    write_packet(stream, &resp_packet).ok();
    None
}

/// 处理自定义协议中的 DATA 转发
fn handle_custom_data(
    state: &RelayState,
    src: SocketAddr,
    data: &[u8],
    room: &str,
    _password: &str,
    encrypted: &[u8],
    room_len: usize,
    pass_len: usize,
) {
    let host_addr = {
        let rooms = match state.rooms.lock() {
            Ok(r) => r,
            Err(_) => return,
        };
        rooms.get(room).map(|info| info.host_addr)
    };
    let host_addr = match host_addr {
        Some(addr) => addr,
        None => {
            // 本地无此房间，尝试多跳隧道转发
            tunnel_packet_to_relay(state, data, room);
            return;
        }
    };

    let mut packet = Vec::new();
    packet.push(room_len as u8);
    packet.extend_from_slice(&data[1..1 + room_len]);
    packet.push(pass_len as u8);
    packet.extend_from_slice(&data[1 + room_len + 1..1 + room_len + 1 + pass_len]);
    packet.extend_from_slice(encrypted);

    let clients = match state.clients.lock() {
        Ok(c) => c,
        Err(_) => return,
    };
    if host_addr != src {
        // 收到成员的数据 -> 转发给房主（回包给MC Link房主客户端）
        if let Some(host_stream) = clients.get(&host_addr.to_string()) {
            let mut host_stream = match host_stream.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            write_packet(&mut host_stream, &packet).ok();
        }
    } else {
        // 收到房主的数据 -> 转发给所有成员（发包给MC Link成员客户端）
        for (addr, client) in clients.iter() {
            if *addr != host_addr.to_string() {
                let mut client = match client.lock() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                write_packet(&mut client, &packet).ok();
            }
        }
        drop(clients);
        // 多跳转发到远程成员
        tunnel_to_remote_members(state, data, room);
    }
}

/// 处理数据转发（0x40 协议）
fn handle_data(state: &RelayState, src: SocketAddr, data: &[u8]) {
    if data.len() < 4 {
        return;
    }
    let room_name_len = u32::from_be_bytes(data[0..4].try_into().unwrap()) as usize;
    if data.len() < 4 + room_name_len {
        return;
    }
    let room_name = String::from_utf8_lossy(&data[4..4 + room_name_len]);
    let actual_data = &data[4 + room_name_len..];
    let host_addr = {
        let rooms = match state.rooms.lock() {
            Ok(r) => r,
            Err(_) => return,
        };
        rooms
            .get(room_name.as_ref())
            .map(|room| room.host_addr)
    };
    let host_addr = match host_addr {
        Some(addr) => addr,
        None => return,
    };
    if host_addr != src {
        // 成员->房主：转发到房主（回包给房主客户端）
        let clients = match state.clients.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Some(host_stream) = clients.get(&host_addr.to_string()) {
            let mut host_stream = match host_stream.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut packet = vec![0x40];
            packet.extend_from_slice(&room_name_len.to_be_bytes());
            packet.extend_from_slice(room_name.as_bytes());
            packet.extend_from_slice(actual_data);
            write_packet(&mut host_stream, &packet).ok();
        }
    }
}

/// 处理 Ping（0x32）
fn handle_ping(stream: &mut TcpStream) {
    write_packet(stream, &[0x33]).ok();
}

/// 处理测试包（0x41）
fn handle_test_packet(state: &RelayState, src: SocketAddr, data: &[u8]) {
    if data.len() < 2 {
        return;
    }
    let room_name_len = data[1] as usize;
    if data.len() < 2 + room_name_len {
        return;
    }
    let room_name = String::from_utf8_lossy(&data[2..2 + room_name_len]);
    let pass_len = data[2 + room_name_len] as usize;
    if data.len() < 2 + room_name_len + 1 + pass_len {
        return;
    }
    let host_info = {
        let rooms = match state.rooms.lock() {
            Ok(r) => r,
            Err(_) => return,
        };
        rooms
            .get(room_name.as_ref())
            .map(|room| (room.host_addr, room.host_addr == src))
    };
    let (host_addr, is_same_relay) = match host_info {
        Some(pair) => pair,
        None => return,
    };
    if !is_same_relay {
        // 成员->房主：转发测试包到房主
        let clients = match state.clients.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Some(host_stream) = clients.get(&host_addr.to_string()) {
            let mut host_stream = match host_stream.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            write_packet(&mut host_stream, &[0x41]).ok();
            log(
                LogLevel::Info,
                &format!("[测试] 已转发测试数据包到房主 {}", host_addr),
            );
        }
    } else {
        // 房主->成员：转发测试包到所有成员（发包给成员客户端）
        let clients = match state.clients.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        for (addr, client) in clients.iter() {
            if *addr != host_addr.to_string() {
                let mut client = match client.lock() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                write_packet(&mut client, &[0x41]).ok();
                log(
                    LogLevel::Info,
                    &format!("[测试] 已转发测试数据包到成员 {}", addr),
                );
            }
        }
    }
}