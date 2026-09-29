//! Illusion 访问者连接处理
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion\src\server\listener.rs

use std::net::TcpStream;
use std::sync::Arc;

use mc_link_common::log::{log, LogLevel};

use crate::illusion::bridge;
use crate::illusion::registry::{ProxyTask, Registry};
use crate::illusion::sniffer;

/// 处理访问者连接：嗅探 Minecraft 域名 → 向客户端发送代理请求
pub fn handle_connection(stream: TcpStream, registry: Arc<Registry>, addr: String) {
    let _ = stream.set_nodelay(true);

    // 嗅探域名
    let mut peek_buf = [0u8; 256];
    let n = match stream.peek(&mut peek_buf) {
        Ok(n) if n > 0 => n,
        Ok(_) => {
            log(LogLevel::Warn, &format!("[Illusion] 访问者 {} 未发送数据", addr));
            return;
        }
        Err(e) => {
            log(LogLevel::Warn, &format!("[Illusion] 访问者 {} peek 失败: {}", addr, e));
            return;
        }
    };

    let target = sniffer::sniff(&peek_buf[..n]);

    let domain = match &target {
        sniffer::Target::Minecraft { host } => host.clone(),
        sniffer::Target::Unknown => {
            log(LogLevel::Warn, &format!("[Illusion] 访问者 {} 无法识别 Minecraft 握手包", addr));
            return;
        }
    };

    log(LogLevel::Info, &format!("[Illusion] 访问者 {} 域名: {}", addr, domain));

    // 查找注册了该域名的客户端
    let sender = match registry.find_client(&domain) {
        Some(s) => s,
        None => {
            log(LogLevel::Warn, &format!("[Illusion] 未找到域名 {} 对应的客户端", domain));
            return;
        }
    };

    let conn_id = bridge::next_conn_id();

    // 存储挂起的访问者连接
    registry.store_pending(conn_id, stream, domain.clone());

    // 通知客户端建立代理
    let task = ProxyTask {
        conn_id,
        domain: domain.clone(),
    };

    log(LogLevel::Info, &format!(
        "[Illusion] 通知客户端代理: conn_id={}, domain={}", conn_id, domain
    ));

    if sender.send(task).is_err() {
        registry.remove_pending(conn_id);
        log(LogLevel::Warn, &format!(
            "[Illusion] 客户端已断开，无法转发域名 {}", domain
        ));
    }
}
