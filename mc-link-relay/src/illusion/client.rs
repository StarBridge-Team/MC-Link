//! Illusion 客户端连接处理
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion\src\server\listener.rs

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use mc_link_common::log::{log, LogLevel};

use crate::illusion::bridge;
use crate::illusion::frame::{encode_frame, read_frame};
use crate::illusion::registry::Registry;
use crate::illusion::types::*;

// ----- 内部使用的类型别名 -----
type ProxyTask = crate::illusion::registry::ProxyTask;

/// 处理 Illusion 客户端连接（通过 "IL" 魔法字节识别）
///
/// 读取首帧区分控制连接（Register）还是数据通道（ProxyReady）
pub fn handle_connection(stream: TcpStream, registry: Arc<Registry>, token: Arc<String>, addr: String) {
    let _ = stream.set_nodelay(true);

    let mut stream = stream;
    let frame = match read_frame(&mut stream) {
        Ok(Some(f)) => f,
        Ok(None) => return,
        Err(e) => {
            log(LogLevel::Warn, &format!("[Illusion] 客户端 {} 读首帧失败: {}", addr, e));
            return;
        }
    };

    let (frame_type, payload) = frame;

    match frame_type {
        TYPE_REGISTER => {
            handle_control_connection(stream, registry, token, addr, &payload);
        }
        TYPE_PROXY_READY => {
            handle_proxy_ready(stream, registry, addr, &payload);
        }
        _ => {
            log(LogLevel::Warn, &format!("[Illusion] 客户端 {} 未知帧类型: {}", addr, frame_type));
        }
    }
}

/// 处理控制连接（Register 帧）
fn handle_control_connection(
    mut stream: TcpStream,
    registry: Arc<Registry>,
    expected_token: Arc<String>,
    addr: String,
    payload: &[u8],
) {
    let register: Register = match serde_json::from_slice(payload) {
        Ok(r) => r,
        Err(e) => {
            log(LogLevel::Warn, &format!("[Illusion] {} Register 解析失败: {}", addr, e));
            return;
        }
    };

    if register.token != *expected_token {
        log(LogLevel::Warn, &format!("[Illusion] {} token 无效", addr));
        let ack = RegisterAck {
            success: false,
            message: "invalid token".to_string(),
        };
        if let Ok(data) = serde_json::to_vec(&ack) {
            let frame = encode_frame(TYPE_REGISTER_ACK, &data);
            let _ = stream.write_all(&frame);
        }
        return;
    }

    let client_id = bridge::next_conn_id();
    let (task_tx, task_rx) = mpsc::channel::<ProxyTask>();

    let registered = registry.register(client_id, register.domains, register.token, task_tx);

    let ack = RegisterAck {
        success: true,
        message: format!("registered {} domains", registered.len()),
    };
    let ack_data = match serde_json::to_vec(&ack) {
        Ok(d) => d,
        Err(e) => {
            log(LogLevel::Error, &format!("[Illusion] {} RegisterAck 序列化失败: {}", addr, e));
            registry.unregister(client_id);
            return;
        }
    };
    let ack_frame = encode_frame(TYPE_REGISTER_ACK, &ack_data);
    if stream.write_all(&ack_frame).is_err() {
        log(LogLevel::Warn, &format!("[Illusion] {} 发送 RegisterAck 失败", addr));
        registry.unregister(client_id);
        return;
    }

    log(LogLevel::Info, &format!(
        "[Illusion] 客户端 {} (ID:{}) 控制连接已建立，域名: {:?}",
        addr, client_id, registered
    ));

    // 控制连接主循环：读心跳 + 转发代理任务
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));

    loop {
        // 1. 尝试读取客户端帧（心跳），带超时
        match try_read_frame(&mut stream) {
            Ok(Some((TYPE_HEARTBEAT, _))) => {
                // heartbeat received, continue
            }
            Ok(Some((t, _))) => {
                log(LogLevel::Warn, &format!("[Illusion] 客户端 {} 未知帧类型: {}", client_id, t));
            }
            Ok(None) => {
                // No frame available within timeout, continue to check tasks
            }
            Err(e) => {
                log(LogLevel::Info, &format!(
                    "[Illusion] 客户端 {} 控制连接断开: {}", client_id, e
                ));
                break;
            }
        }

        // 2. 检查是否有待发送的代理任务（非阻塞）
        loop {
            match task_rx.try_recv() {
                Ok(task) => {
                    let np = NewProxy {
                        conn_id: task.conn_id,
                        domain: task.domain,
                        peeked: Vec::new(),
                    };
                    if let Ok(data) = serde_json::to_vec(&np) {
                        let frame = encode_frame(TYPE_NEW_PROXY, &data);
                        if stream.write_all(&frame).is_err() {
                            log(LogLevel::Warn, &format!(
                                "[Illusion] 发送 NewProxy 到客户端 {} 失败", client_id
                            ));
                            break;
                        }
                        log(LogLevel::Info, &format!(
                            "[Illusion] 发送 NewProxy(conn_id={}) 到客户端 {}", task.conn_id, client_id
                        ));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    log(LogLevel::Info, &format!(
                        "[Illusion] 客户端 {} 通知通道已关闭", client_id
                    ));
                    break;
                }
            }
        }

        // 3. 检查连接是否仍然活跃
        if !is_connected(&mut stream) {
            break;
        }
    }

    registry.unregister(client_id);
}

/// 处理数据通道连接（ProxyReady 帧）
fn handle_proxy_ready(
    stream: TcpStream,
    registry: Arc<Registry>,
    addr: String,
    payload: &[u8],
) {
    let ready: ProxyReady = match serde_json::from_slice(payload) {
        Ok(r) => r,
        Err(e) => {
            log(LogLevel::Warn, &format!("[Illusion] {} ProxyReady 解析失败: {}", addr, e));
            return;
        }
    };

    // 验证 token
    let valid = registry.validate_token(&ready.token).is_some();
    if !valid {
        log(LogLevel::Warn, &format!("[Illusion] {} 数据通道 token 无效 (conn_id={})", addr, ready.conn_id));
        return;
    }

    log(LogLevel::Info, &format!(
        "[Illusion] 数据通道就绪: conn_id={} (来自 {})", ready.conn_id, addr
    ));

    // 取出挂起的访问者连接
    const TIMEOUT_SECS: u64 = 30;
    let pending = poll_pending(&registry, ready.conn_id, TIMEOUT_SECS);

    match pending {
        Some(pending) => {
            log(LogLevel::Info, &format!(
                "[Illusion] 开始桥接 {} ({}): 访问者 ↔ 客户端", ready.conn_id, pending.domain
            ));

            bridge::bridge(pending.stream, stream, &format!("{}", ready.conn_id));
        }
        None => {
            log(LogLevel::Warn, &format!(
                "[Illusion] conn_id={} 未找到对应的挂起连接或等待超时({}s)", ready.conn_id, TIMEOUT_SECS
            ));
        }
    }
}

/// 尝试从流中读取一帧（带超时处理）
fn try_read_frame(stream: &mut TcpStream) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0u8; 5];
    match stream.read_exact(&mut header) {
        Ok(()) => {}
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
            return Ok(None);
        }
        Err(e) => return Err(e),
    }

    if header[0] != MAGIC[0] || header[1] != MAGIC[1] {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid magic"));
    }

    let frame_type = header[2];
    let payload_len = u16::from_be_bytes([header[3], header[4]]) as usize;

    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload)?;
    }

    Ok(Some((frame_type, payload)))
}

/// 检查连接是否仍然活跃
fn is_connected(stream: &mut TcpStream) -> bool {
    match stream.write(&[]) {
        Ok(_) => true,
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => true,
        Err(_) => false,
    }
}

/// 轮询等待指定 conn_id 的挂起连接
fn poll_pending(registry: &Registry, conn_id: u64, timeout_secs: u64) -> Option<super::registry::PendingConn> {
    let start = std::time::Instant::now();
    loop {
        if let Some(pending) = registry.take_pending(conn_id) {
            return Some(pending);
        }
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            registry.remove_pending(conn_id);
            return None;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

// ProxyTask is defined in registry
