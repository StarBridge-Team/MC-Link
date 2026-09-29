//! 路径规划模块（智能多跳：拥塞感知 + 短路径优先）
//!
//! ## 设计原则
//! 1. 优先短路径：正常情况路径不超过 3 个节点（1-2 跳）
//! 2. 拥塞回避：避开连接数/带宽占用高的中继节点
//! 3. 两阶段搜索：先尝试 3 跳内最优解，找不到再放宽到 5 跳

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use uuid::Uuid;

use mc_link_common::log::{log, LogLevel};
use mc_link_common::protocol::write_packet;
use crate::types::*;

/// 理想的单隧道节点数上限（含两端端点，即 1-2 跳）
const MAX_HOPS_PREFERRED: u32 = 3;
/// 绝对最大跳数上限（回退）
const MAX_HOPS_FALLBACK: u32 = 5;
/// 每额外一跳的惩罚（ms 等价），使算法倾向短路径
const HOP_PENALTY_MS: f64 = 80.0;
/// 拥塞惩罚上限（ms 等价），参考：100 并发连接 = 满负荷
const CONGEST_REF_CONNECTIONS: f64 = 100.0;
const CONGEST_PENALTY_MAX_MS: f64 = 60.0;
/// 低丢包节点的奖励折扣上限（ms），丢包率 0% → 折扣 15ms，100% → 折扣 0
const LOSS_REWARD_MAX_MS: f64 = 15.0;

/// 从流量上报统计中计算各中继的拥塞得分 (0.0 ~ 1.0)
fn compute_congestion_scores(state: &CentralState) -> HashMap<String, f64> {
    let traffic = state.traffic_reports.lock().unwrap_or_else(|e| e.into_inner());
    traffic.iter().map(|(id, stats)| {
        let score = (stats.current_connections as f64 / CONGEST_REF_CONNECTIONS).min(1.0);
        (id.clone(), score)
    }).collect()
}

/// 计算每个中继节点的平均丢包率（从所有邻接链路的探针数据中取均值）
///
/// 返回值：node_id → 0.0(无丢包) ~ 1.0(完全丢包)。若节点无探针数据则默认 1.0。
fn compute_node_reliability(graph: &TopologyGraph) -> HashMap<String, f64> {
    graph.nodes.keys().map(|id| {
        let neighbors = graph.get_neighbors(id);
        if neighbors.is_empty() {
            return (id.clone(), 1.0);
        }
        let avg = neighbors.iter()
            .map(|(_, m)| m.packet_loss as f64)
            .sum::<f64>() / neighbors.len() as f64;
        (id.clone(), avg)
    }).collect()
}

/// 寻找最优路径（Dijkstra + 智能代价函数）
///
/// 代价 = 基础链路代价（延迟×丢包）+ 跳数惩罚 + 中间节点拥塞惩罚 - 低丢包奖励
fn find_optimal_path(
    graph: &TopologyGraph,
    host_relay_id: &str,
    client_relay_id: &str,
    congestion_scores: &HashMap<String, f64>,
    max_hops: u32,
) -> Option<PathResult> {
    if host_relay_id == client_relay_id {
        if let Some(node) = graph.nodes.get(host_relay_id) {
            let hops = vec![
                PathHop { node_id: node.id.clone(), address: node.address.clone(), address_v6: node.address_v6.clone() },
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

    // 预计算各节点平均丢包率，用于低丢包奖励
    let node_reliability = compute_node_reliability(graph);

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
                graph.nodes.get(id).map(|n| PathHop { node_id: n.id.clone(), address: n.address.clone(), address_v6: n.address_v6.clone() })
            }).collect();

            return Some(PathResult {
                path_id: Uuid::new_v4().to_string(),
                hops,
                total_latency_ms: total_latency,
                score: cost,
            });
        }

        if hop_count >= max_hops { continue; }

        for (neighbor, metric) in graph.get_neighbors(&node) {
            if neighbor != client_relay_id && neighbor != host_relay_id {
                if let Some(n) = graph.nodes.get(neighbor) {
                    if !n.transit { continue; }
                }
            }

            // --- 智能代价函数 ---
            // 1. 基础链路代价：延迟 + 丢包加权
            let edge_cost = metric.latency_ms as f64 * (1.0 + metric.packet_loss as f64 * 10.0);

            // 2. 跳数惩罚：超过 2 跳（3 节点）后每跳加重惩罚，使算法倾向短路径
            let hop_penalty = if hop_count >= 2 {
                (hop_count as f64 - 1.0) * HOP_PENALTY_MS
            } else {
                0.0
            };

            // 3. 拥塞惩罚：避开连接数高的繁忙节点
            let congest_penalty = if neighbor != client_relay_id && neighbor != host_relay_id {
                congestion_scores.get(neighbor)
                    .map(|&score| score * CONGEST_PENALTY_MAX_MS)
                    .unwrap_or(0.0)
            } else {
                0.0  // 不对端点（房主/成员中继）施加拥塞惩罚
            };

            // 4. 低丢包奖励：平均丢包率越低折扣越大，优质节点更受欢迎
            let loss_reward = if neighbor != client_relay_id && neighbor != host_relay_id {
                node_reliability.get(neighbor)
                    .map(|&avg_loss| (1.0 - avg_loss) * LOSS_REWARD_MAX_MS)
                    .unwrap_or(0.0)
            } else {
                0.0
            };

            let next_cost = cost + edge_cost + hop_penalty + congest_penalty - loss_reward;

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
///
/// 策略：两阶段搜索
/// 1. 先以 MAX_HOPS_PREFERRED（3 跳）尝试找短路径
/// 2. 找不到再放宽到 MAX_HOPS_FALLBACK（5 跳）
pub fn assign_room_path(state: &CentralState, room_name: &str, host_relay_id: &str, client_relay_id: &str, use_mixed: bool) {
    let mgr = state.topology_manager.read().unwrap_or_else(|e| e.into_inner());
    let topology = if use_mixed { &mgr.mixed } else { &mgr.ipv4 };

    // 计算各中继拥塞得分
    let congestion_scores = compute_congestion_scores(state);

    let path = if host_relay_id == client_relay_id {
        if let Some(node) = topology.nodes.get(host_relay_id) {
            Some(PathResult {
                path_id: Uuid::new_v4().to_string(),
                hops: vec![PathHop { node_id: node.id.clone(), address: node.address.clone(), address_v6: node.address_v6.clone() }],
                total_latency_ms: 0,
                score: 0.0,
            })
        } else { None }
    } else {
        // 阶段 1：优先找短路径（≤ MAX_HOPS_PREFERRED 跳）
        log(LogLevel::Info, &format!(
            "[路径] {} 尝试 {} 跳内路径：{} -> {}",
            room_name, MAX_HOPS_PREFERRED, host_relay_id, client_relay_id
        ));
        let mut path = find_optimal_path(&topology, host_relay_id, client_relay_id, &congestion_scores, MAX_HOPS_PREFERRED);

        // 阶段 2：找不到则放宽跳数限制
        if path.is_none() {
            log(LogLevel::Warn, &format!(
                "[路径] {} {} 跳内无可用路径，放宽到 {} 跳",
                room_name, MAX_HOPS_PREFERRED, MAX_HOPS_FALLBACK
            ));
            path = find_optimal_path(&topology, host_relay_id, client_relay_id, &congestion_scores, MAX_HOPS_FALLBACK);
        }

        path
    };

    drop(mgr);

    if let Some(path) = path {
        let hop_count = path.hops.len().saturating_sub(1);
        log(LogLevel::Info, &format!(
            "[路径] 房间 {} 路径: {} (延迟={}ms 跳数={} 节点数={}{})",
            room_name,
            path.hops.iter().map(|h| h.node_id.as_str()).collect::<Vec<_>>().join(" -> "),
            path.total_latency_ms,
            hop_count,
            path.hops.len(),
            if hop_count > 2 { " (超过3节点)" } else { "" },
        ));

        let path_id = path.path_id.clone();
        state.active_paths.lock().unwrap_or_else(|e| e.into_inner()).insert(path_id.clone(), path.clone());
        state.room_paths.lock().unwrap_or_else(|e| e.into_inner()).insert(room_name.to_string(), path_id.clone());

        push_room_route_to_relays(state, room_name, &path);
    } else {
        log(LogLevel::Warn, &format!("[路径] 房间 {} 无法找到路径", room_name));
    }
}

/// 推送路径和路由到相关中继
fn push_room_route_to_relays(state: &CentralState, room_name: &str, path: &PathResult) {
    let relay_streams = state.relay_streams.lock().unwrap_or_else(|e| e.into_inner());

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
    let room_paths = state.room_paths.lock().unwrap_or_else(|e| e.into_inner());
    let active_paths = state.active_paths.lock().unwrap_or_else(|e| e.into_inner());
    let rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());

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

    let relays_snapshot = state.relays.lock().unwrap_or_else(|e| e.into_inner());
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
        // 重路由时同样使用智能路径规划
        // 先随机选一个目标中继，由 assign_room_path 内部做最优规划
        let idx = rand::random::<usize>() % fallback_relays.len();
        let client_relay = &fallback_relays[idx];
        log(LogLevel::Warn, &format!("[重路由] 路径 {} 中断，重新规划 {} -> {}", room_name, host_relay, client_relay));
        assign_room_path(state, room_name, host_relay, client_relay, false);
    }
}
