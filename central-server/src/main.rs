//! MC Link 中央服务器 - 入口
//!
//! 功能：中继协调、房间管理、路径规划、聊天服务器注册

mod config;
mod console;
mod handlers;
mod oauth;
mod path;
mod protocol;
mod relay;
mod room;
mod session;
mod stats_snapshot;
mod token;
mod types;
mod web_admin;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::load_config;
use crate::handlers::{handle_client, cleanup_thread};
use crate::oauth::{OAuthState, Provider};
use crate::session::SessionStore;
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
        let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
        let mut topo = state.topology.lock().unwrap_or_else(|e| e.into_inner());
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
            *listener_console.lock().unwrap_or_else(|e| e.into_inner()) = Some(l);
            true
        } else {
            false
        }
    };
    console::start_console(state.clone(), restart_fn);

    // Web 管理面板
    let admin_token = token::load_or_generate_token();

    // 初始化 OAuth 状态（从配置加载提供商凭据）
    let mut client_ids = HashMap::new();
    let mut client_secrets = HashMap::new();
    if let Some(github) = config.oauth.get("github") {
        client_ids.insert(Provider::GitHub, github.client_id.clone());
        client_secrets.insert(Provider::GitHub, github.client_secret.clone());
    }
    if let Some(ms) = config.oauth.get("microsoft") {
        client_ids.insert(Provider::Microsoft, ms.client_id.clone());
        client_secrets.insert(Provider::Microsoft, ms.client_secret.clone());
    }
    if let Some(ls) = config.oauth.get("littleskin") {
        client_ids.insert(Provider::LittleSkin, ls.client_id.clone());
        client_secrets.insert(Provider::LittleSkin, ls.client_secret.clone());
    }
    if let Some(msl) = config.oauth.get("msl") {
        client_ids.insert(Provider::MslCenter, msl.client_id.clone());
        client_secrets.insert(Provider::MslCenter, msl.client_secret.clone());
    }
    let oauth_state = Arc::new(OAuthState::new(client_ids, client_secrets, config.account_api_url));
    let sessions = Arc::new(SessionStore::new());

    let web_bind = format!("{}:{}", config.web_admin_bind, config.web_admin_port);
    if let Ok(web_addr) = web_bind.parse::<std::net::SocketAddr>() {
        let web_state = state.clone();
        let web_token = admin_token.clone();
        let web_oauth = oauth_state.clone();
        let web_sessions = sessions.clone();
        thread::spawn(move || {
            web_admin::start_web_admin(web_state, web_addr, web_token, web_oauth, web_sessions);
        });
    } else {
        log(LogLevel::Warn, &format!("Web 管理面板绑定地址无效: {}", web_bind));
    }

    // 统计快照
    stats_snapshot::start_snapshot_thread(state.clone());

    // 主循环
    loop {
        if !state.is_running() { break; }

        if let Some(ref listener) = *listener.clone().lock().unwrap_or_else(|e| e.into_inner()) {
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