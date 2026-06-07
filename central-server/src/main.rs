//! MC Link 中央服务器 - 入口
//!
//! 功能：中继协调、房间管理、路径规划、聊天服务器注册

mod config;
mod console;
mod handlers;
mod path;
mod protocol;
mod relay;
mod room;
mod types;

use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::load_config;
use crate::handlers::{handle_client, cleanup_thread};
use crate::types::CentralState;
use mc_link_common::protocol::read_proxy_protocol_header;
use mc_link_common::log::{log, LogLevel};

fn main() {
    let config = load_config();
    let state = Arc::new(CentralState::new());

    println!();
    println!("===========================================");
    println!("  MC Link 中央服务器 v2 (多跳)");
    println!("===========================================");
    println!("  监听地址: {}", config.listen_addr);
    println!("  监听端口: {}", config.listen_port);
    println!("===========================================");
    println!();
    println!("按 h 获取帮助");
    println!();

    state.load_relays();
    {
        let relays = state.relays.lock().unwrap();
        let mut topo = state.topology.lock().unwrap();
        for (id, relay) in relays.iter() {
            topo.add_or_update_node(types::RelayNode {
                id: id.clone(),
                name: relay.name.clone(),
                address: relay.address.clone(),
                last_seen: relay.last_seen,
                private: relay.private,
                transit: relay.transit,
                service_type: relay.service_type.clone(),
                udp_port: relay.udp_port,
            });
        }
    }

    let listener = Arc::new(Mutex::new(start_server(state.clone(), &config)));

    // 控制台线程
    let state_console = state.clone();
    let listener_console = listener.clone();
    let restart_fn = move || {
        if let Some(l) = start_server(state_console.clone(), &load_config()) {
            *listener_console.lock().unwrap() = Some(l);
            true
        } else {
            false
        }
    };
    console::start_console(state.clone(), restart_fn);

    // 主循环
    loop {
        if !state.is_running() { break; }

        if let Some(ref listener) = *listener.clone().lock().unwrap() {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    let real_addr = match read_proxy_protocol_header(&mut stream) {
                        Some(header) => {
                            log(LogLevel::Info, &format!("Proxy Protocol V2 - 客户端: {}", header.src_addr));
                            header.src_addr
                        }
                        None => addr,
                    };
                    let state_clone = state.clone();
                    thread::spawn(move || handle_client(&mut stream, state_clone, real_addr));
                }
                Err(_) => {}
            }
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }

    state.save_relays();
    log(LogLevel::Info, "中央服务器已关闭");
}

fn start_server(state: Arc<CentralState>, config: &config::Config) -> Option<std::net::TcpListener> {
    let bind_addr = format!("{}:{}", config.listen_addr, config.listen_port);
    match std::net::TcpListener::bind(&bind_addr) {
        Ok(listener) => {
            log(LogLevel::Info, &format!("中央服务器启动在 {}", bind_addr));
            let state_clone = state.clone();
            thread::spawn(move || cleanup_thread(state_clone));
            Some(listener)
        }
        Err(e) => {
            log(LogLevel::Error, &format!("无法绑定端口 {}: {}", bind_addr, e));
            None
        }
    }
}