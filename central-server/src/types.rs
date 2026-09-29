//! 中央服务器数据结构和共享状态

use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use mc_link_common::log::{log, LogLevel};
pub use mc_link_common::types::{PathHop, RoomInfo, PlayerInfo};

pub const RELAYS_FILE: &str = "relays.json";

// ===== 基础数据结构 =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayNode {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(default)]
    pub address_v6: Option<String>,
    pub last_seen: u64,
    #[serde(default)]
    pub private: bool,
    #[serde(default = "default_true")]
    pub transit: bool,
    /// 闲置模式：不在列表显示，但可作第一跳（与 private 互斥）
    #[serde(default)]
    pub idle: bool,
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

/// 客户端中继容量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCaps {
    #[serde(default)]
    pub max_connections: u32,
    #[serde(default)]
    pub max_bandwidth_bps: u64,
}

/// 客户端中继限速配置（由中央服务器管理员配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRelayLimits {
    pub max_bandwidth_bps: u64,
    pub max_connections: u32,
}

impl Default for ClientRelayLimits {
    fn default() -> Self {
        Self { max_bandwidth_bps: 5_000_000, max_connections: 8 }
    }
}

impl From<crate::config::ClientRelayConfig> for ClientRelayLimits {
    fn from(c: crate::config::ClientRelayConfig) -> Self {
        Self {
            max_bandwidth_bps: c.max_bandwidth_bps,
            max_connections: c.max_connections,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResult {
    pub path_id: String,
    pub hops: Vec<PathHop>,
    pub total_latency_ms: u64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyEntry {
    pub from_id: String,
    pub to_id: String,
    pub latency_ms: u64,
    pub samples: Vec<u64>,
}

// ===== 流量统计 =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficReport {
    pub relay_id: String,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub connections: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficStats {
    pub bytes_sent_total: u64,
    pub bytes_recv_total: u64,
    pub last_report: u64,
    pub current_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsSnapshot {
    pub timestamp: u64,
    // 中继
    pub relay_count: usize,
    pub online_relay_count: usize,
    pub client_relay_count: usize,
    pub online_client_relay_count: usize,
    // 房间
    pub room_count: usize,
    pub player_count: usize,
    pub player_roles: HashMap<String, usize>,
    // 路径
    pub path_count: usize,
    // 流量（字节）
    pub total_traffic_bytes: u64,
    pub traffic_in: u64,
    pub traffic_out: u64,
    // 拓扑
    pub topology_ipv4_nodes: usize,
    pub topology_mixed_nodes: usize,
    // 当前连接数
    pub active_connections: usize,
    // 房间认证
    pub room_auth_count: usize,
    // 运行时间（秒）
    pub uptime_secs: u64,
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

/// 拓扑管理器 — 统一管理纯 IPv4 和混合两张拓扑表
/// 读操作远多于写操作，故使用 RwLock
pub struct TopologyManager {
    /// 纯 IPv4 拓扑
    pub ipv4: TopologyGraph,
    /// 混合拓扑：官方中继 + 有 IPv6 的客户端中继
    pub mixed: TopologyGraph,
}

impl TopologyManager {
    pub fn new() -> Self {
        Self { ipv4: TopologyGraph::new(), mixed: TopologyGraph::new() }
    }
}

// ===== 中央服务器共享状态 =====

pub struct CentralState {
    pub relays: Mutex<HashMap<String, RelayNode>>,
    pub rooms: Mutex<HashMap<String, RoomInfo>>,
    pub players: Mutex<HashMap<String, Vec<PlayerInfo>>>,
    pub latencies: Mutex<HashMap<String, LatencyEntry>>,
    /// 拓扑管理器（RwLock 读写分离）
    pub topology_manager: RwLock<TopologyManager>,
    pub active_paths: Mutex<HashMap<String, PathResult>>,
    pub room_paths: Mutex<HashMap<String, String>>,
    pub addr_to_id: Mutex<HashMap<String, String>>,
    pub relay_streams: Mutex<HashMap<String, Arc<Mutex<std::net::TcpStream>>>>,
    /// 使用单调时钟跟踪心跳时间，不受系统时间跳变影响
    pub heartbeat_instants: Mutex<HashMap<String, std::time::Instant>>,
    pub running: Arc<Mutex<bool>>,
    pub traffic_reports: Mutex<HashMap<String, TrafficStats>>,
    pub stats_history: Mutex<VecDeque<StatsSnapshot>>,
    /// 房间认证状态: room_name → is_authenticated
    pub room_auth: Mutex<HashMap<String, bool>>,
    /// 待处理的重路由请求（断线 debounce + 批量处理）
    pub pending_reroute: Mutex<Vec<String>>,
    /// 客户端中继限速配置（由中央服务器管理员设定）
    pub client_relay_limits: Mutex<ClientRelayLimits>,
    /// 服务器启动时间（用于统计运行时长）
    pub server_start_time: Instant,
}

fn default_true() -> bool { true }

impl CentralState {
    pub fn new() -> Self {
        Self {
            relays: Mutex::new(HashMap::new()),
            rooms: Mutex::new(HashMap::new()),
            players: Mutex::new(HashMap::new()),
            latencies: Mutex::new(HashMap::new()),
            topology_manager: RwLock::new(TopologyManager::new()),
            active_paths: Mutex::new(HashMap::new()),
            room_paths: Mutex::new(HashMap::new()),
            addr_to_id: Mutex::new(HashMap::new()),
            relay_streams: Mutex::new(HashMap::new()),
            heartbeat_instants: Mutex::new(HashMap::new()),
            running: Arc::new(Mutex::new(true)),
            traffic_reports: Mutex::new(HashMap::new()),
            stats_history: Mutex::new(VecDeque::new()),
            room_auth: Mutex::new(HashMap::new()),
            pending_reroute: Mutex::new(Vec::new()),
            client_relay_limits: Mutex::new(ClientRelayLimits::default()),
            server_start_time: Instant::now(),
        }
    }

    pub fn save_relays(&self) {
        let relays = self.relays.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(json) = serde_json::to_string_pretty(&*relays) {
            if let Err(e) = fs::write(RELAYS_FILE, json) {
                log(LogLevel::Error, &format!("保存中继列表失败: {}", e));
            }
        }
    }

    pub fn load_relays(&self) {
        if let Ok(content) = fs::read_to_string(RELAYS_FILE) {
            if let Ok(relays) = serde_json::from_str::<HashMap<String, RelayNode>>(&content) {
                let mut current = self.relays.lock().unwrap_or_else(|e| e.into_inner());
                *current = relays;
                log(LogLevel::Info, &format!("已加载 {} 个中继服务器", current.len()));
            }
        }
    }

    pub fn stop(&self) { *self.running.lock().unwrap_or_else(|e| e.into_inner()) = false; }
    pub fn is_running(&self) -> bool { *self.running.lock().unwrap_or_else(|e| e.into_inner()) }
}

// ===== 请求/响应结构 =====

#[derive(Deserialize)]
pub struct RelayRegisterReq {
    pub id: String,
    pub name: String,
    pub address: String,
    #[serde(default)]
    pub address_v6: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default = "default_true")]
    pub transit: bool,
    /// 闲置模式：不在列表显示，但可作第一跳
    #[serde(default)]
    pub idle: bool,
    #[serde(default)]
    pub service_type: String,
    #[serde(default)]
    pub udp_port: u16,
    /// 客户端中继容量（仅 client-relay 使用）
    #[serde(default)]
    pub client_caps: Option<ClientCaps>,
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
pub struct RelayModeSwitch {
    pub id: String,
    pub mode: String, // "idle" | "private" | "normal"
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
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub has_ipv6: bool,
}

#[derive(Deserialize)]
pub struct GetRoomReq {
    pub room_name: String,
    pub client_relay_id: Option<String>,
    #[serde(default)]
    pub has_ipv6: bool,
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
    pub password: Option<String>,
    pub relay_id: Option<String>,
    #[serde(default)]
    pub has_ipv6: bool,
}

impl Default for TopologyGraph {
    fn default() -> Self {
        Self::new()
    }
}