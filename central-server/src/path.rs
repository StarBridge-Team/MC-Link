//! 路径规划模块（Dijkstra 算法 + 路径推送）

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use uuid::Uuid;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::write_packet;
use crate::types::*;

/// 寻找最优路径（Dijkstra）
fn find_optimal_path(
    graph: &TopologyGraph,
    host_relay_id: &str,
    client_relay_id: &str,
) -> Option<PathResult> {
    if host_relay_id == client_relay_id {
        if let Some(node) = graph.nodes.get(host_relay_id) {
            let hops = vec![
                PathHop { node_id: node.id.clone(), address: node.address.clone() },
            ];
            return Some(PathResult {
                path_id: Uuid::new_v4().to_string(),
                hops,
                total_latency_ms: 0,
                score: 0.0,
            });
        }
        return None;
    }

    struct State {
        node: String,
        cost: f64,
        hop_count: u32,
        path: Vec<String>,
    }
    impl Eq for State {}
    impl PartialEq for State { fn eq(&self, other: &Self) -> bool { self.cost == other.cost } }
    impl Ord for State {
        fn cmp(&self, other: &Self) -> Ordering {
            other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
        }
    }
    impl PartialOrd for State {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
    }

    let mut distances: HashMap<String, f64> = HashMap::new();
    let mut heap = BinaryHeap::new();

    distances.insert(host_relay_id.to_string(), 0.0);
    heap.push(State {
        node: host_relay_id.to_string(),
        cost: 0.0,
        hop_count: 0,
        path: vec![host_relay_id.to_string()],
    });

    while let Some(State { node, cost, hop_count, path }) = heap.pop() {
        if &node == client_relay_id && hop_count >= 1 {
            let total_latency = path.windows(2).filter_map(|w| {
                let key = if w[0] < w[1] { (w[0].clone(), w[1].clone()) } else { (w[1].clone(), w[0].clone()) };
                graph.edges.get(&key)
            }).map(|m| m.latency_ms as u64).sum::<u64>();

            let hops: Vec<PathHop> = path.iter().filter_map(|id| {
                graph.nodes.get(id).map(|n| PathHop { node_id: n.id.clone(), address: n.address.clone() })
            }).collect();

            return Some(PathResult {
                path_id: Uuid::new_v4().to_string(),
                hops,
                total_latency_ms: total_latency,
                score: cost,
            });
        }

        if hop_count > 6 { continue; }

        for (neighbor, metric) in graph.get_neighbors(&node) {
            if neighbor != client_relay_id && neighbor != host_relay_id {
                if let Some(n) = graph.nodes.get(neighbor) {
                    if !n.transit { continue; }
                }
            }

            let edge_cost = metric.latency_ms as f64 * (1.0 + metric.packet_loss as f64 * 10.0);
            let next_cost = cost + edge_cost;

            if next_cost < *distances.get(neighbor).unwrap_or(&f64::MAX) {
                let mut p = path.clone();
                p.push(neighbor.to_string());
                distances.insert(neighbor.to_string(), next_cost);
                heap.push(State {
                    node: neighbor.to_string(),
                    cost: next_cost,
                    hop_count: hop_count + 1,
                    path: p,
                });
            }
        }
    }

    None
}

/// 为房间分配最优路径并推送给中继
pub fn assign_room_path(state: &CentralState, room_name: &str, host_relay_id: &str, client_relay_id: &str) {
    let topology = state.topology.lock().unwrap();

    let path = if host_relay_id == client_relay_id {
        if let Some(node) = topology.nodes.get(host_relay_id) {
            Some(PathResult {
                path_id: Uuid::new_v4().to_string(),
                hops: vec![PathHop { node_id: node.id.clone(), address: node.address.clone() }],
                total_latency_ms: 0,
                score: 0.0,
            })
        } else { None }
    } else {
        find_optimal_path(&topology, host_relay_id, client_relay_id)
    };

    drop(topology);

    if let Some(path) = path {
        log(LogLevel::Info, &format!(
            "[路径] 房间 {} 路径: {} (延迟={}ms 跳数={})",
            room_name,
            path.hops.iter().map(|h| h.node_id.as_str()).collect::<Vec<_>>().join(" -> "),
            path.total_latency_ms,
            path.hops.len()
        ));

        let path_id = path.path_id.clone();
        state.active_paths.lock().unwrap().insert(path_id.clone(), path.clone());
        state.room_paths.lock().unwrap().insert(room_name.to_string(), path_id.clone());

        push_room_route_to_relays(state, room_name, &path);
    } else {
        log(LogLevel::Warn, &format!("[路径] 房间 {} 无法找到路径", room_name));
    }
}

/// 推送路径和路由到相关中继
fn push_room_route_to_relays(state: &CentralState, room_name: &str, path: &PathResult) {
    let relay_streams = state.relay_streams.lock().unwrap();

    let host_relay_id = path.hops.first().map(|h| h.node_id.as_str()).unwrap_or("");
    let member_relay_id = path.hops.last().map(|h| h.node_id.as_str()).unwrap_or("");

    let path_assign = serde_json::json!({
        "path_id": path.path_id,
        "hops": path.hops,
    });
    let pa_bytes = path_assign.to_string().into_bytes();

    // 1. 向所有中继推送 0x35 (路径分配)
    for hop in &path.hops {
        if let Some(stream_arc) = relay_streams.get(&hop.node_id) {
            if let Ok(mut stream) = stream_arc.lock() {
                let mut pkt = vec![0x35];
                pkt.extend_from_slice(&pa_bytes);
                write_packet(&mut stream, &pkt).ok();
                log(LogLevel::Info, &format!("[路径/推送] 路径 {} 分配到中继 {}",
                    &path.path_id[..8], &hop.node_id[..8]));
            }
        }
    }

    // 2. 正向路由: member → host
    if !member_relay_id.is_empty() && member_relay_id != host_relay_id {
        let fwd_route = serde_json::json!({
            "room_name": room_name,
            "direction": "forward",
            "target_relay_id": host_relay_id,
            "path_id": path.path_id,
        });
        if let Some(stream_arc) = relay_streams.get(member_relay_id) {
            if let Ok(mut stream) = stream_arc.lock() {
                let mut pkt = vec![0x36];
                pkt.extend_from_slice(fwd_route.to_string().as_bytes());
                write_packet(&mut stream, &pkt).ok();
                log(LogLevel::Info, &format!("[路由/正向] -> 中继 {} 房间 {} 目标={}",
                    &member_relay_id[..8], room_name, &host_relay_id[..8]));
            }
        }
    }

    // 3. 反向路由: host → member
    if !host_relay_id.is_empty() && host_relay_id != member_relay_id {
        let rev_route = serde_json::json!({
            "room_name": room_name,
            "direction": "reverse",
            "target_relay_id": member_relay_id,
            "path_id": path.path_id,
        });
        if let Some(stream_arc) = relay_streams.get(host_relay_id) {
            if let Ok(mut stream) = stream_arc.lock() {
                let mut pkt = vec![0x36];
                pkt.extend_from_slice(rev_route.to_string().as_bytes());
                write_packet(&mut stream, &pkt).ok();
                log(LogLevel::Info, &format!("[路由/反向] -> 中继 {} 房间 {} 目标={}",
                    &host_relay_id[..8], room_name, &member_relay_id[..8]));
            }
        }
    }
}

/// 当中继断开时重路由受影响的路径
pub fn reroute_affected_paths(state: &CentralState, dead_relay_id: &str) {
    let room_paths = state.room_paths.lock().unwrap();
    let active_paths = state.active_paths.lock().unwrap();
    let rooms = state.rooms.lock().unwrap();

    let mut affected: Vec<(String, String)> = Vec::new();
    for (room_name, path_id) in room_paths.iter() {
        if let Some(path) = active_paths.get(path_id) {
            if path.hops.iter().any(|h| h.node_id == dead_relay_id) {
                if let Some(room) = rooms.get(room_name) {
                    affected.push((room_name.clone(), room.host_relay_id.clone()));
                }
            }
        }
    }
    drop(room_paths);
    drop(active_paths);
    drop(rooms);

    if affected.is_empty() { return; }

    let relays_snapshot = state.relays.lock().unwrap();
    let fallback_relays: Vec<String> = relays_snapshot.keys()
        .filter(|id| *id != dead_relay_id)
        .cloned()
        .collect();
    drop(relays_snapshot);

    if fallback_relays.is_empty() {
        log(LogLevel::Error, "[重路由] 没有可用中继节点，无法重路由");
        return;
    }

    for (room_name, host_relay) in &affected {
        let idx = rand::random::<usize>() % fallback_relays.len();
        let client_relay = &fallback_relays[idx];
        log(LogLevel::Warn, &format!("[重路由] 路径 {} 中断，重新规划 {} -> {}", room_name, host_relay, client_relay));
        assign_room_path(state, room_name, host_relay, client_relay);
    }
}