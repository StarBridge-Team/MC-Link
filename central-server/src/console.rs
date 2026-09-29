//! 控制台交互模块

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::now_secs;
use crate::types::CentralState;

fn print_help() {
    println!();
    println!("===========================================");
    println!("  MC Link 中央服务器 - 帮助");
    println!("===========================================");
    println!("  h - 显示帮助");
    println!("  s - 停止服务器");
    println!("  r - 重启服务器");
    println!("  c - 显示中继列表");
    println!("  t - 显示拓扑");
    println!("  p - 显示活跃路径");
    println!("  q - 退出");
    println!("===========================================");
    println!();
}

/// 启动控制台输入处理线程
pub fn start_console(state: Arc<CentralState>, restart_fn: impl Fn() -> bool + Send + 'static) {
    thread::spawn(move || {
        loop {
            print!("> ");
            std::io::Write::flush(&mut std::io::stdout()).ok();

            let mut input = String::new();
            match std::io::stdin().read_line(&mut input) {
                Ok(0) | Err(_) => {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                _ => {}
            }

            let input = input.trim();
            match input {
                "h" => print_help(),
                "s" => {
                    log(LogLevel::Info, "正在停止服务器...");
                    state.stop();
                }
                "r" => {
                    log(LogLevel::Info, "正在重启服务器...");
                    state.stop();
                    thread::sleep(Duration::from_secs(1));
                    if restart_fn() {
                        log(LogLevel::Info, "服务器已重启");
                    }
                }
                "c" => {
                    let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
                    println!("\n中继列表 (共 {} 个):", relays.len());
                    for (_id, relay) in relays.iter() {
                        println!("  - {} @ {} (最后活跃: {}s前)",
                            relay.name, relay.address, now_secs() - relay.last_seen);
                    }
                    println!();
                }
                "t" => {
                    let topo = state.topology_manager.read().unwrap_or_else(|e| e.into_inner());
                    println!("\n拓扑:");
                    for edge in topo.ipv4.edges.values() {
                        println!("  {} <-> {} 延迟={}ms 丢包={:.0}%",
                            edge.node_a, edge.node_b, edge.latency_ms, edge.packet_loss * 100.0);
                    }
                    if topo.ipv4.edges.is_empty() {
                        println!("  (空 - 等待探针数据)");
                    }
                    println!();
                }
                "p" => {
                    let paths = state.active_paths.lock().unwrap_or_else(|e| e.into_inner());
                    let room_paths = state.room_paths.lock().unwrap_or_else(|e| e.into_inner());
                    println!("\n活跃路径:");
                    for (path_id, path) in paths.iter() {
                        let room = room_paths.iter().find(|(_, pid)| *pid == path_id);
                        let room_str = room.map(|(r, _)| r.as_str()).unwrap_or("?");
                        println!("  房间 {} [{}]: {} (延迟={}ms)",
                            room_str, &path_id[..8],
                            path.hops.iter().map(|h| h.node_id.as_str()).collect::<Vec<_>>().join(" -> "),
                            path.total_latency_ms);
                    }
                    if paths.is_empty() { println!("  (无)"); }
                    println!();
                }
                "q" => {
                    log(LogLevel::Info, "正在关闭服务器...");
                    state.stop();
                    state.save_relays();
                    break;
                }
                "" => {}
                _ => {
                    log(LogLevel::Warn, &format!("未知命令: {}", input));
                    print_help();
                }
            }
        }
    });
}