mod client;
mod config;
mod heartbeat;
mod path;
mod protocol;
mod relay;
mod room;
mod udp_relay;

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::read_proxy_protocol_header;
use mc_link_common::utils::now_secs;
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
    running: Mutex<bool>,
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
            running: Mutex::new(true),
        }
    }

    fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }

    fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
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
    let bandwidth_limit = config.lock().unwrap().bandwidth_limit_mbps;

    // 启动 UDP 中继线程
    let udp_port_relay = state.udp_port;
    thread::spawn(move || {
        udp_relay::start_udp_relay(udp_port_relay);
    });

    // 心跳线程
    let state_for_heartbeat = state.clone();
    let heartbeat_interval = config.lock().unwrap().heartbeat_interval;
    let central_server_hb = config.lock().unwrap().central_server.clone();
    thread::spawn(move || {
        let central_addr: SocketAddr = resolve_server(&central_server_hb);
        loop {
            if !state_for_heartbeat.is_running() {
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

            send_register(&mut central_stream, &state_for_heartbeat);

            loop {
                thread::sleep(Duration::from_secs(heartbeat_interval));
                if !state_for_heartbeat.is_running() {
                    break;
                }
                if !send_heartbeat(&mut central_stream, &state_for_heartbeat) {
                    log(LogLevel::Warn, "中央服务器心跳发送失败，尝试重连...");
                    break;
                }
            }
        }
    });

    // 中继探测线程
    let state_for_probe = state.clone();
    let central_server_probe = config.lock().unwrap().central_server.clone();
    let relay_id_for_probe = state.relay_id.clone();
    let probe_interval = config.lock().unwrap().probe_interval;
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
    let state_for_health = state.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(30));
        loop {
            if !state_for_health.is_running() {
                break;
            }

            let stale_peers: Vec<String> = {
                let peers = state_for_health.peer_connections.lock().unwrap();
                let mut stale = Vec::new();
                for (addr, stream_arc) in peers.iter() {
                    if let Ok(mut s) = stream_arc.try_lock() {
                        // 发送空包(4字节长度0)检查连接是否存活，对端 read_packet 会读到空包并跳过
                        if s.write_all(&[0u8; 4]).is_err() {
                            stale.push(addr.clone());
                        }
                    }
                }
                stale
            };

            for addr in &stale_peers {
                state_for_health.peer_connections.lock().unwrap().remove(addr);
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
                    let mut cfg = config_for_input.lock().unwrap();
                    if let Some(loaded) = load_config_inner(&config_path) {
                        *cfg = loaded;
                        log(LogLevel::Info, "配置已重载");
                    } else {
                        log(LogLevel::Error, "重载配置失败");
                    }
                }
                "name" => {
                    if arg.is_empty() {
                        let name = state_for_input.relay_name.lock().unwrap().clone();
                        log(LogLevel::Info, &format!("当前名称: {}", name));
                    } else {
                        let new_name = arg.to_string();
                        *state_for_input.relay_name.lock().unwrap() = new_name.clone();
                        let mut cfg = config_for_input.lock().unwrap();
                        cfg.relay_name = Some(new_name.clone());
                        save_config(&config_path, &cfg);
                        log(LogLevel::Info, &format!("中继名称已设置为: {}", new_name));
                    }
                }
                "rate" => {
                    if arg.is_empty() {
                        let cfg = config_for_input.lock().unwrap();
                        match cfg.bandwidth_limit_mbps {
                            Some(limit) => {
                                log(LogLevel::Info, &format!("当前速率限制: {} MB/s", limit))
                            }
                            None => log(LogLevel::Info, "当前速率限制: 无限制"),
                        }
                    } else if let Ok(mbps) = arg.parse::<f64>() {
                        let limit = if mbps <= 0.0 { None } else { Some(mbps) };
                        let mut cfg = config_for_input.lock().unwrap();
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
                    let table = state_for_input.path_table.lock().unwrap();
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

        if let Ok((mut stream, _)) = listener.lock().unwrap().accept() {
            stream.set_nodelay(true).ok();
            // 自定义协议（MC Link 游戏协议）
            stream.set_nonblocking(false).ok();
            let real_addr = match read_proxy_protocol_header(&mut stream) {
                Some(header) => {
                    log(
                        LogLevel::Info,
                        &format!("Proxy Protocol V2 - 客户端: {}", header.src_addr),
                    );
                    header.src_addr
                }
                None => stream.peer_addr().unwrap(),
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

