//! MC Link 中央服务器 - 入口
//!
//! 功能：中继协调、房间管理、路径规划、聊天服务器注册

mod account_api;
mod apnic;
mod config;
mod console;
mod email;
mod handlers;
mod identity;
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
use crate::email::EmailVerifyState;
use crate::handlers::{handle_client, cleanup_thread};
use crate::identity::IdentityStore;
use crate::oauth::{OAuthState, Provider};
use crate::session::SessionStore;
use crate::types::CentralState;
use mc_link_common::protocol::read_proxy_protocol_header;
use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::{lock_or_recover, rwlock_write_or_recover};

fn main() {
    let config = load_config();
    let state = Arc::new(CentralState::new());

    // 应用配置中的客户端中继限速
    {
        let mut limits = lock_or_recover(&state.client_relay_limits, "[配置] client_relay_limits");
        *limits = config.client_relay.clone().into();
    }

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
        let relays = lock_or_recover(&state.relays, "[启动] relays");
        let mut mgr = rwlock_write_or_recover(&state.topology_manager, "[启动] topology_manager");
        for (id, relay) in relays.iter() {
            mgr.ipv4.add_or_update_node(types::RelayNode {
                id: id.clone(),
                name: relay.name.clone(),
                address: relay.address.clone(),
                address_v6: relay.address_v6.clone(),
                last_seen: relay.last_seen,
                private: relay.private,
                transit: relay.transit,
                idle: relay.idle,
                service_type: relay.service_type.clone(),
                udp_port: relay.udp_port,
            });
            if relay.address_v6.is_some() && relay.transit {
                mgr.mixed.add_or_update_node(types::RelayNode {
                    id: id.clone(),
                    name: relay.name.clone(),
                    address: relay.address.clone(),
                    address_v6: relay.address_v6.clone(),
                    last_seen: relay.last_seen,
                    private: relay.private,
                    transit: relay.transit,
                    idle: relay.idle,
                    service_type: relay.service_type.clone(),
                    udp_port: relay.udp_port,
                });
            }
        }
    }

    let listener = Arc::new(Mutex::new(start_server(state.clone(), &config)));

    // 控制台线程
    let state_console = state.clone();
    let listener_console = listener.clone();
    let restart_fn = move || {
        if let Some(l) = start_server(state_console.clone(), &load_config()) {
            *lock_or_recover(&*listener_console, "[控制台] listener") = Some(l);
            true
        } else {
            false
        }
    };
    console::start_console(state.clone(), restart_fn);

    // Web 管理面板
    let admin_token = token::load_or_generate_token();

    // APNIC 白名单
    let apnic_whitelist = apnic::ApnicWhitelist::new(config.apnic.clone());
    if config.apnic.enabled {
        log(LogLevel::Info, "[APNIC] IP 白名单已启用，仅允许 APNIC 亚太地区 IP 连接");
        apnic_whitelist.start_daily_refresh();
    }

    // 初始化 OAuth 状态（从配置加载提供商凭据）
    let mut client_ids = HashMap::new();
    let mut client_secrets = HashMap::new();
    if let Some(github) = config.oauth.get("github") {
        client_ids.insert(Provider::GitHub, github.client_id.clone());
        client_secrets.insert(Provider::GitHub, github.client_secret.clone());
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
    let email_state = Arc::new(EmailVerifyState::new());
    let identity_store = Arc::new(IdentityStore::new());
    let smtp_config = config.smtp.clone();

    let web_bind = format!("{}:{}", config.web_admin_bind, config.web_admin_port);
    if let Ok(web_addr) = web_bind.parse::<std::net::SocketAddr>() {
        let web_state = state.clone();
        let web_token = admin_token.clone();
        let web_oauth = oauth_state.clone();
        let web_sessions = sessions.clone();
        let web_email = email_state.clone();
        let web_identity = identity_store.clone();
        thread::spawn(move || {
            web_admin::start_web_admin(web_state, web_addr, web_token, web_oauth, web_sessions, web_email, smtp_config, web_identity);
        });
    } else {
        log(LogLevel::Warn, &format!("Web 管理面板绑定地址无效: {}", web_bind));
    }

    // 统计快照
    stats_snapshot::start_snapshot_thread(state.clone());

    // 身份令牌、邮箱验证码和会话清理线程（每5分钟清理一次过期数据）
    let cleanup_identity = identity_store.clone();
    let cleanup_sessions = sessions.clone();
    let cleanup_email = email_state.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_secs(300));
            cleanup_identity.cleanup();
            cleanup_sessions.cleanup();
            cleanup_email.cleanup();
        }
    });

    // 主循环
    loop {
        if !state.is_running() { break; }

        if let Some(ref listener) = *lock_or_recover(&*listener, "[主循环] listener") {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    // APNIC IP 白名单检查
                    if !apnic_whitelist.is_allowed(addr.ip()) {
                        log(LogLevel::Warn, &format!("[APNIC] 拒绝非亚太地区连接: {}", addr));
                        drop(stream);
                        continue;
                    }

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
    log(LogLevel::Info, "中央服务器已关闭（主线程退出）");
    // 注意: web 管理面板、统计快照等线程作为守护线程运行，
    // 主线程退出后进程会自动退出，无需强制 exit
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