mod client;
mod config;
mod heartbeat;
mod illusion;
mod path;
mod protocol;
mod relay;
mod room;
mod udp_relay;

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::read_proxy_protocol_header;
use mc_link_common::utils::{now_secs, lock_or_recover};
use crate::config::{load_config, load_config_inner, resolve_server, save_config};
use crate::heartbeat::{send_heartbeat, send_register};

// ===== 共享状态 =====

#[derive(Debug, Clone)]
pub struct RoomInfo {
    pub name: String,
    pub host_addr: SocketAddr,
}

pub struct RelayState {
    pub rooms: Mutex<HashMap<String, RoomInfo>>,
    pub clients: Mutex<HashMap<String, Arc<Mutex<TcpStream>>>>,
    pub relay_id: String,
    pub relay_name: Mutex<String>,
    pub relay_port: u16,
    pub udp_port: u16,
    pub report_address: Option<String>,
    pub central_addr: Option<SocketAddr>,
    pub private_mode: bool,
    pub transit_mode: bool,
    pub path_table: Mutex<HashMap<String, relay::PathAssignment>>,
    pub room_route_map: Mutex<HashMap<String, relay::RoomRoute>>,
    pub reverse_route_map: Mutex<HashMap<String, Vec<relay::RoomRoute>>>,
    pub peer_connections: Mutex<HashMap<String, Arc<Mutex<TcpStream>>>>,
    /// 房间级限速: room_name → limit_bps (0=不限速)
    pub room_speed_limits: Mutex<HashMap<String, u64>>,
    /// 房间级限速跟踪: room_name → (当前秒字节数, 当前秒时间戳)
    pub room_byte_tracker: Mutex<HashMap<String, (u64, u64)>>,
    running: Mutex<bool>,
    pub traffic_bytes_sent: AtomicU64,
    pub traffic_bytes_recv: AtomicU64,
    pub traffic_connections: AtomicU32,
}

impl RelayState {
    fn new(port: u16, udp_port: u16, report_addr: Option<String>, name: String, central_addr: Option<SocketAddr>,
           private_mode: bool, transit_mode: bool) -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
            relay_id: uuid::Uuid::new_v4().to_string(),
            relay_name: Mutex::new(name),
            relay_port: port,
            udp_port,
            report_address: report_addr,
            central_addr,
            private_mode,
            transit_mode,
            path_table: Mutex::new(HashMap::new()),
            room_route_map: Mutex::new(HashMap::new()),
            reverse_route_map: Mutex::new(HashMap::new()),
            peer_connections: Mutex::new(HashMap::new()),
            room_speed_limits: Mutex::new(HashMap::new()),
            room_byte_tracker: Mutex::new(HashMap::new()),
            running: Mutex::new(true),
            traffic_bytes_sent: AtomicU64::new(0),
            traffic_bytes_recv: AtomicU64::new(0),
            traffic_connections: AtomicU32::new(0),
        }
    }

    fn stop(&self) {
        *lock_or_recover(&self.running, "[状态] running") = false;
    }

    fn is_running(&self) -> bool {
        *lock_or_recover(&self.running, "[状态] running")
    }
}

// ===== 帮助 =====

fn print_help() {
    println!();
    println!("===========================================");
    println!("  MC-Link 中继服务器 - 帮助");
    println!("===========================================");
    println!("  h         - 显示帮助");
    println!("  s / stop  - 停止服务器");
    println!("  r / reload- 重载配置文件");
    println!("  pt        - 显示路径表");
    println!("  name <n>  - 设置中继名称");
    println!("  rate <n>  - 设置速率限制 (MB/s, 0=无限制)");
    println!("  q / exit  - 退出");
    println!("===========================================");
    println!();
}

// ===== 入口 =====

fn main() {
    let config = load_config();
    let bind_addr = format!("0.0.0.0:{}", config.relay_port);

    println!();
    println!("===========================================");
    println!("  MC-Link 中继服务器 v{}", env!("CARGO_PKG_VERSION"));
    println!("===========================================");
    println!("  监听端口: {}", config.relay_port);
    println!("  中央服务器: {}", config.central_server);
    println!("  心跳间隔: {}秒", config.heartbeat_interval);
    println!("  延迟探测间隔: {}秒", config.latency_check_interval);
    println!("  中继探测间隔: {}秒", config.probe_interval);
    match config.bandwidth_limit_mbps {
        Some(limit) => println!("  带宽限制: {} MB/s", limit),
        None => println!("  带宽限制: 无限制"),
    }
    match &config.report_address {
        Some(addr) => println!("  上报地址: {}", addr),
        None => println!("  上报地址: 自动检测"),
    }
    println!("===========================================");
    println!();
    println!("按 h 获取帮助");
    println!();

    let central_addr = resolve_server(&config.central_server);

    // 查找可用 UDP 端口
    let udp_port = udp_relay::find_udp_port(config.relay_port);
    println!("  UDP 中继端口: {}", udp_port);

    let state = Arc::new(RelayState::new(
        config.relay_port,
        udp_port,
        config.report_address.clone(),
        config
            .relay_name
            .clone()
            .unwrap_or_else(|| format!("Relay-{}", rand::random::<u32>())),
        Some(central_addr),
        config.private_mode,
        config.transit_mode,
    ));
    let config = Arc::new(Mutex::new(config));
    let bandwidth_limit = lock_or_recover(&*config, "[配置] bandwidth_limit").bandwidth_limit_mbps;

    // ===== Illusion 内网穿透初始化 =====
    let illusion_registry: Option<std::sync::Arc<illusion::registry::Registry>>;
    let illusion_token_arc: Option<std::sync::Arc<String>>;
    {
        let cfg = lock_or_recover(&*config, "[配置] illusion_token");
        match &cfg.illusion_token {
            Some(token) if !token.is_empty() => {
                illusion_registry = Some(std::sync::Arc::new(illusion::registry::Registry::new()));
                illusion_token_arc = Some(std::sync::Arc::new(token.clone()));
                log(LogLevel::Info, &format!("[Illusion] 内网穿透已启用"));
            }
            _ => {
                illusion_registry = None;
                illusion_token_arc = None;
            }
        }
    }

    // 启动 UDP 中继线程
    let udp_port_relay = state.udp_port;
    thread::spawn(move || {
        udp_relay::start_udp_relay(udp_port_relay);
    });

    // 心跳线程 - 同时监听中央服务器的指令(如0x39限速)
    let state_for_central_cmd = state.clone();
    let heartbeat_interval = lock_or_recover(&*config, "[配置] heartbeat_interval").heartbeat_interval;
    let central_server_hb = lock_or_recover(&*config, "[配置] central_server_hb").central_server.clone();
    thread::spawn(move || {
        let central_addr: SocketAddr = resolve_server(&central_server_hb);
        loop {
            if !state_for_central_cmd.is_running() {
                break;
            }

            let mut central_stream = match TcpStream::connect(central_addr) {
                Ok(s) => s,
                Err(e) => {
                    log(
                        LogLevel::Error,
                        &format!("无法连接中央服务器: {}，10秒后重试", e),
                    );
                    thread::sleep(Duration::from_secs(10));
                    continue;
                }
            };

            send_register(&mut central_stream, &state_for_central_cmd);

            loop {
                // 发送心跳
                if !send_heartbeat(&mut central_stream, &state_for_central_cmd) {
                    log(LogLevel::Warn, "中央服务器心跳发送失败，尝试重连...");
                    break;
                }

                // 设置短暂读超时，检查中央服务器是否有指令下发
                central_stream.set_read_timeout(Some(Duration::from_millis(500))).ok();
                loop {
                    match crate::protocol::read_packet(&mut central_stream) {
                        Ok(buf) => {
                            if buf.is_empty() { continue; }
                            crate::relay::handle_central_command(&state_for_central_cmd, &buf);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                            break;
                        }
                        Err(_) => {
                            // 连接可能已断开
                            break;
                        }
                    }
                }

                thread::sleep(Duration::from_secs(heartbeat_interval));
                if !state_for_central_cmd.is_running() {
                    break;
                }
            }
        }
    });

    // 中继探测线程
    let state_for_probe = state.clone();
    let central_server_probe = lock_or_recover(&*config, "[配置] central_server_probe").central_server.clone();
    let relay_id_for_probe = state.relay_id.clone();
    let probe_interval = lock_or_recover(&*config, "[配置] probe_interval").probe_interval;
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(15));
        let central_addr: SocketAddr = resolve_server(&central_server_probe);
        loop {
            if !state_for_probe.is_running() {
                break;
            }
            let relays = match path::get_relays_from_central(&central_addr) {
                Some(r) => r,
                None => {
                    thread::sleep(Duration::from_secs(60));
                    continue;
                }
            };

            for relay in &relays {
                let relay_id = relay.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if relay_id == relay_id_for_probe.as_str() {
                    continue;
                }
                let address = match relay.get("address").and_then(|v| v.as_str()) {
                    Some(a) => a,
                    None => continue,
                };
                let addr: SocketAddr = match address.parse() {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                if let Some((latency_ms, packet_loss)) = path::probe_relay(&addr) {
                    if let Ok(mut stream) =
                        TcpStream::connect_timeout(&central_addr, Duration::from_secs(5))
                    {
                        let report = serde_json::json!({
                            "from_id": relay_id_for_probe,
                            "to_id": relay_id,
                            "latency_ms": latency_ms as u16,
                            "packet_loss": packet_loss,
                        });
                        let mut packet = vec![0x31];
                        packet.extend_from_slice(report.to_string().as_bytes());
                        protocol::write_packet(&mut stream, &packet).ok();
                        log(
                            LogLevel::Info,
                            &format!(
                                "[探针/上报] -> {} 延迟={}ms 丢包={:.0}%",
                                relay_id,
                                latency_ms,
                                packet_loss * 100.0
                            ),
                        );
                    }
                }
            }

            thread::sleep(Duration::from_secs(probe_interval));
        }
    });

    // 对等连接健康检测线程
    // 使用 TCP keepalive 检测死连接，不发送假包破坏包边界同步
    let state_for_health = state.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(30));
        loop {
            if !state_for_health.is_running() {
                break;
            }

            let stale_peers: Vec<String> = {
                let peers = lock_or_recover(&state_for_health.peer_connections, "[对等连接] peer_connections");
                let mut stale = Vec::new();
                for (addr, stream_arc) in peers.iter() {
                    if let Ok(mut s) = stream_arc.try_lock() {
                        // 使用 write 检查对端是否存活（写入0字节不会影响包边界）
                        // 若写入失败则连接已断开
                        if s.flush().is_err() {
                            stale.push(addr.clone());
                        }
                    }
                }
                stale
            };

            for addr in &stale_peers {
                lock_or_recover(&state_for_health.peer_connections, "[对等连接] peer_connections_remove").remove(addr);
                log(
                    LogLevel::Warn,
                    &format!("[对等连接] 连接 {} 已断开，已清理", addr),
                );
            }

            if !stale_peers.is_empty() {
                log(
                    LogLevel::Info,
                    &format!("[对等连接] 清理了 {} 个失连连接", stale_peers.len()),
                );
            }

            thread::sleep(Duration::from_secs(15));
        }
    });

    // 流量上报线程 - 每 30 秒上报流量统计到中央服务器
    let state_for_traffic = state.clone();
    let central_traffic = lock_or_recover(&*config, "[配置] central_traffic").central_server.clone();
    thread::spawn(move || {
        let central_addr: SocketAddr = resolve_server(&central_traffic);
        // 启动后先等 10 秒再开始上报
        thread::sleep(Duration::from_secs(10));
        loop {
            if !state_for_traffic.is_running() {
                break;
            }

            let sent = state_for_traffic.traffic_bytes_sent.swap(0, Ordering::Relaxed);
            let recv = state_for_traffic.traffic_bytes_recv.swap(0, Ordering::Relaxed);
            let conns = state_for_traffic.traffic_connections.load(Ordering::Relaxed);

            // 只在有流量或连接时才上报
            if sent > 0 || recv > 0 || conns > 0 {
                let report = serde_json::json!({
                    "relay_id": state_for_traffic.relay_id,
                    "bytes_sent": sent,
                    "bytes_recv": recv,
                    "connections": conns,
                    "timestamp": now_secs(),
                });

                if let Ok(mut stream) = TcpStream::connect_timeout(&central_addr, Duration::from_secs(5)) {
                    let mut packet = vec![0x38];
                    packet.extend_from_slice(report.to_string().as_bytes());
                    crate::protocol::write_packet(&mut stream, &packet).ok();
                    log(LogLevel::Info, &format!(
                        "[流量/上报] sent={} recv={} conns={}",
                        sent, recv, conns
                    ));
                }
            }

            thread::sleep(Duration::from_secs(30));
        }
    });

    // 监听端口
    let listener = match TcpListener::bind(&bind_addr) {
        Ok(l) => l,
        Err(e) => {
            log(
                LogLevel::Error,
                &format!("无法绑定端口 {}: {}", bind_addr, e),
            );
            return;
        }
    };
    listener.set_nonblocking(true).ok();

    let listener = Arc::new(Mutex::new(listener));
    let _listener_for_console = listener.clone();
    let state_for_input = state.clone();
    let config_for_input = config.clone();
    let config_path = crate::config::config_path();

    // 控制台输入线程
    thread::spawn(move || {
        let is_tty = std::io::stdin().is_terminal();
        loop {
            if is_tty {
                print!("> ");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            let mut input = String::new();
            match std::io::stdin().read_line(&mut input) {
                Ok(0) => break,
                Err(_) => {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                _ => {}
            }
            let input = input.trim().to_string();
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            let cmd = parts[0];
            let arg = parts.get(1).copied().unwrap_or("");

            match cmd {
                "h" | "help" => print_help(),
                "s" | "stop" => {
                    log(LogLevel::Info, "正在停止服务器...");
                    state_for_input.stop();
                }
                "r" | "reload" => {
                    let mut cfg = lock_or_recover(&*config_for_input, "[配置] reload");
                    if let Some(loaded) = load_config_inner(&config_path) {
                        *cfg = loaded;
                        log(LogLevel::Info, "配置已重载");
                    } else {
                        log(LogLevel::Error, "重载配置失败");
                    }
                }
                "name" => {
                    if arg.is_empty() {
                        let name = lock_or_recover(&state_for_input.relay_name, "[中继] relay_name_get").clone();
                        log(LogLevel::Info, &format!("当前名称: {}", name));
                    } else {
                        let new_name = arg.to_string();
                        *lock_or_recover(&state_for_input.relay_name, "[中继] relay_name_set") = new_name.clone();
                        let mut cfg = lock_or_recover(&*config_for_input, "[配置] name_set");
                        cfg.relay_name = Some(new_name.clone());
                        save_config(&config_path, &cfg);
                        log(LogLevel::Info, &format!("中继名称已设置为: {}", new_name));
                    }
                }
                "rate" => {
                    if arg.is_empty() {
                        let cfg = lock_or_recover(&*config_for_input, "[配置] rate_get");
                        match cfg.bandwidth_limit_mbps {
                            Some(limit) => {
                                log(LogLevel::Info, &format!("当前速率限制: {} MB/s", limit))
                            }
                            None => log(LogLevel::Info, "当前速率限制: 无限制"),
                        }
                    } else if let Ok(mbps) = arg.parse::<f64>() {
                        let limit = if mbps <= 0.0 { None } else { Some(mbps) };
                        let mut cfg = lock_or_recover(&*config_for_input, "[配置] rate_set");
                        cfg.bandwidth_limit_mbps = limit;
                        save_config(&config_path, &cfg);
                        match limit {
                            Some(l) => {
                                log(LogLevel::Info, &format!("速率限制已设置为: {} MB/s", l))
                            }
                            None => log(LogLevel::Info, "速率限制: 无限制"),
                        }
                    } else {
                        log(LogLevel::Error, "参数错误: 请输入有效的数字 (MB/s)");
                    }
                }
                "pt" => {
                    let table = lock_or_recover(&state_for_input.path_table, "[路径] path_table");
                    println!("\n路径表 (共 {} 条):", table.len());
                    for (path_id, path) in table.iter() {
                        println!(
                            "  [{}]: {}",
                            &path_id[..8],
                            path.hops
                                .iter()
                                .map(|h| h.node_id.as_str())
                                .collect::<Vec<_>>()
                                .join(" -> ")
                        );
                    }
                    if table.is_empty() {
                        println!("  (空)");
                    }
                    println!();
                }
                "q" | "exit" => {
                    log(LogLevel::Info, "正在关闭服务器...");
                    state_for_input.stop();
                    break;
                }
                "" => {}
                _ => {
                    log(LogLevel::Warn, &format!("未知命令: {}", cmd));
                    print_help();
                }
            }
        }
    });

    // 主循环：接受客户端连接
    loop {
        if !state.is_running() {
            break;
        }

        if let Ok((mut stream, _)) = lock_or_recover(&*listener, "[监听] listener").accept() {
            stream.set_nodelay(true).ok();
            stream.set_nonblocking(false).ok();
            let addr = stream.peer_addr().unwrap();

            // Illusion 连接分类（只有启用时才做 peek）
            if let Some(ref registry) = illusion_registry {
                let mut peek_buf = [0u8; 2];
                if let Ok(n) = stream.peek(&mut peek_buf) {
                    if n >= 2 && illusion::frame::is_illusion_connection(&peek_buf) {
                        let reg = registry.clone();
                        let tok = illusion_token_arc.clone().unwrap();
                        let addr_str = addr.to_string();
                        thread::spawn(move || {
                            illusion::client::handle_connection(stream, reg, tok, addr_str);
                        });
                        continue;
                    }

                    // 非 IL 连接：尝试作为 Illusion 访问者嗅探（仅 Peek 一次）
                    let mut peek_large = [0u8; 256];
                    if let Ok(n) = stream.peek(&mut peek_large) {
                        if n > 0 {
                            let target = illusion::sniffer::sniff(&peek_large[..n]);
                            let is_visitor = matches!(&target,
                                illusion::sniffer::Target::Minecraft { .. }
                            );
                            if is_visitor {
                                let reg = registry.clone();
                                let addr_str = addr.to_string();
                                thread::spawn(move || {
                                    illusion::visitor::handle_connection(stream, reg, addr_str);
                                });
                                continue;
                            }
                        }
                    }
                }
            }

            // MC Link 协议
            let real_addr = match read_proxy_protocol_header(&mut stream) {
                Some(header) => {
                    log(
                        LogLevel::Info,
                        &format!("Proxy Protocol V2 - 客户端: {}", header.src_addr),
                    );
                    header.src_addr
                }
                None => addr,
            };
            let state_clone = state.clone();
            let bandwidth_limit = bandwidth_limit;
            thread::spawn(move || {
                client::handle_client(stream, state_clone, real_addr, bandwidth_limit)
            });
        }

        thread::sleep(Duration::from_millis(10));
    }

    log(LogLevel::Info, "中继服务器已关闭");
}

