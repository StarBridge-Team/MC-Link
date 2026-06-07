//! 中央服务器数据结构和共享状态

use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use mc_link_common::log::{log, LogLevel};

pub const RELAYS_FILE: &str = "relays.json";

// ===== 基础数据结构 =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayNode {
    pub id: String,
    pub name: String,
    pub address: String,
    pub last_seen: u64,
    #[serde(default)]
    pub private: bool,
    #[serde(default = "default_true")]
    pub transit: bool,
    #[serde(default)]
    pub service_type: String,
    #[serde(default)]
    pub udp_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkMetric {
    pub node_a: String,
    pub node_b: String,
    pub latency_ms: u16,
    pub packet_loss: f32,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathHop {
    pub node_id: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResult {
    pub path_id: String,
    pub hops: Vec<PathHop>,
    pub total_latency_ms: u64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub name: String,
    pub password_hash: String,
    pub host_relay_id: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub name: String,
    pub role: String,
    pub joined_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyEntry {
    pub from_id: String,
    pub to_id: String,
    pub latency_ms: u64,
    pub samples: Vec<u64>,
}

// ===== 拓扑图 =====

pub struct TopologyGraph {
    pub nodes: HashMap<String, RelayNode>,
    pub edges: HashMap<(String, String), LinkMetric>,
}

impl TopologyGraph {
    pub fn new() -> Self {
        Self { nodes: HashMap::new(), edges: HashMap::new() }
    }

    pub fn add_or_update_node(&mut self, node: RelayNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn remove_node(&mut self, id: &str) {
        self.nodes.remove(id);
        self.edges.retain(|(a, b), _| a != id && b != id);
    }

    pub fn add_or_update_metric(&mut self, a: &str, b: &str, metric: LinkMetric) {
        let key = if a < b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) };
        self.edges.insert(key, metric);
    }

    pub fn get_neighbors(&self, node_id: &str) -> Vec<(&str, &LinkMetric)> {
        self.edges.iter()
            .filter(|((a, b), _)| a == node_id || b == node_id)
            .map(|((a, b), m)| if a == node_id { (b.as_str(), m) } else { (a.as_str(), m) })
            .collect()
    }
}

// ===== 中央服务器共享状态 =====

pub struct CentralState {
    pub relays: Mutex<HashMap<String, RelayNode>>,
    pub rooms: Mutex<HashMap<String, RoomInfo>>,
    pub players: Mutex<HashMap<String, Vec<PlayerInfo>>>,
    pub latencies: Mutex<HashMap<String, LatencyEntry>>,
    pub topology: Mutex<TopologyGraph>,
    pub active_paths: Mutex<HashMap<String, PathResult>>,
    pub room_paths: Mutex<HashMap<String, String>>,
    pub addr_to_id: Mutex<HashMap<String, String>>,
    pub relay_streams: Mutex<HashMap<String, Arc<Mutex<std::net::TcpStream>>>>,
    pub running: Arc<Mutex<bool>>,
}

fn default_true() -> bool { true }

impl CentralState {
    pub fn new() -> Self {
        Self {
            relays: Mutex::new(HashMap::new()),
            rooms: Mutex::new(HashMap::new()),
            players: Mutex::new(HashMap::new()),
            latencies: Mutex::new(HashMap::new()),
            topology: Mutex::new(TopologyGraph::new()),
            active_paths: Mutex::new(HashMap::new()),
            room_paths: Mutex::new(HashMap::new()),
            addr_to_id: Mutex::new(HashMap::new()),
            relay_streams: Mutex::new(HashMap::new()),
            running: Arc::new(Mutex::new(true)),
        }
    }

    pub fn save_relays(&self) {
        let relays = self.relays.lock().unwrap();
        if let Ok(json) = serde_json::to_string_pretty(&*relays) {
            if let Err(e) = fs::write(RELAYS_FILE, json) {
                log(LogLevel::Error, &format!("保存中继列表失败: {}", e));
            }
        }
    }

    pub fn load_relays(&self) {
        if let Ok(content) = fs::read_to_string(RELAYS_FILE) {
            if let Ok(relays) = serde_json::from_str::<HashMap<String, RelayNode>>(&content) {
                let mut current = self.relays.lock().unwrap();
                *current = relays;
                log(LogLevel::Info, &format!("已加载 {} 个中继服务器", current.len()));
            }
        }
    }

    pub fn stop(&self) { *self.running.lock().unwrap() = false; }
    pub fn is_running(&self) -> bool { *self.running.lock().unwrap() }
}

// ===== 请求/响应结构 =====

#[derive(Deserialize)]
pub struct RelayRegisterReq {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default = "default_true")]
    pub transit: bool,
    #[serde(default)]
    pub service_type: String,
    #[serde(default)]
    pub udp_port: u16,
}

#[derive(Deserialize)]
pub struct RelayHeartbeatReq {
    pub id: String,
}

#[derive(Deserialize)]
pub struct LatencyReportReq {
    pub from_id: String,
    pub to_id: String,
    pub latency_ms: u64,
}

#[derive(Deserialize)]
pub struct ProbeReport {
    pub from_id: String,
    pub to_id: String,
    pub latency_ms: u16,
    pub packet_loss: f32,
}

#[derive(Deserialize)]
pub struct PathBrokenReport {
    pub relay_id: String,
    pub path_id: String,
    pub broken_relay_id: String,
}

#[derive(Deserialize)]
pub struct CreateRoomReq {
    pub room_name: String,
    pub password: String,
    pub relay_id: String,
}

#[derive(Deserialize)]
pub struct GetRoomReq {
    pub room_name: String,
    pub client_relay_id: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteRoomReq {
    pub room_name: String,
}

#[derive(Deserialize)]
pub struct JoinRoomReq {
    pub room_name: String,
    pub player_name: String,
    pub role: String,
    pub relay_id: Option<String>,
}

impl Default for TopologyGraph {
    fn default() -> Self {
        Self::new()
    }
}