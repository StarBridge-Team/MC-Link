use std::fs;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::{exe_dir, resolve_address_srv};

// ===== 常量 =====

pub const DEFAULT_CENTRAL_SERVER: &str = "mk.aini2.cn:8878";
pub const DEFAULT_RELAY_PORT: u16 = 57894;
pub const DEFAULT_HEARTBEAT_INTERVAL: u64 = 5;
pub const DEFAULT_LATENCY_CHECK_INTERVAL: u64 = 300;
pub const DEFAULT_PROBE_INTERVAL: u64 = 180;
pub const MIN_BANDWIDTH_MBPS: f64 = 10.0;
pub const FALLBACK_CENTRAL_SERVER: &str = "127.0.0.1:8878";

// ===== 配置结构 =====

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_central_server")]
    pub central_server: String,
    #[serde(default = "default_relay_port")]
    pub relay_port: u16,
    #[serde(default = "default_report_address")]
    pub report_address: Option<String>,
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    #[serde(default = "default_latency_check_interval")]
    pub latency_check_interval: u64,
    #[serde(default = "default_probe_interval")]
    pub probe_interval: u64,
    #[serde(default = "default_bandwidth_limit")]
    pub bandwidth_limit_mbps: Option<f64>,
    #[serde(default = "default_relay_name")]
    pub relay_name: Option<String>,
    #[serde(default = "default_private_mode")]
    pub private_mode: bool,
    #[serde(default = "default_transit_mode")]
    pub transit_mode: bool,
    /// Illusion 内网穿透令牌（空字符串 = 不启用 Illusion）
    #[serde(default)]
    pub illusion_token: Option<String>,
}

fn default_relay_name() -> Option<String> {
    None
}
fn default_central_server() -> String {
    DEFAULT_CENTRAL_SERVER.to_string()
}
fn default_relay_port() -> u16 {
    DEFAULT_RELAY_PORT
}
fn default_heartbeat_interval() -> u64 {
    DEFAULT_HEARTBEAT_INTERVAL
}
fn default_latency_check_interval() -> u64 {
    DEFAULT_LATENCY_CHECK_INTERVAL
}
fn default_probe_interval() -> u64 {
    DEFAULT_PROBE_INTERVAL
}
fn default_bandwidth_limit() -> Option<f64> {
    None
}
fn default_report_address() -> Option<String> {
    None
}
fn default_private_mode() -> bool {
    true
}
fn default_transit_mode() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            central_server: DEFAULT_CENTRAL_SERVER.to_string(),
            relay_port: DEFAULT_RELAY_PORT,
            report_address: None,
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,
            latency_check_interval: DEFAULT_LATENCY_CHECK_INTERVAL,
            probe_interval: DEFAULT_PROBE_INTERVAL,
            bandwidth_limit_mbps: None,
            relay_name: None,
            private_mode: true,
            transit_mode: true,
            illusion_token: None,
        }
    }
}

// ===== 配置文件路径 =====

pub fn config_path() -> String {
    exe_dir().join("config.yml").to_string_lossy().to_string()
}

// ===== 配置加载/保存 =====

pub fn load_config() -> Config {
    let config_path = config_path();
    if !fs::metadata(&config_path).is_ok() {
        let default_config = serde_yaml::to_string(&Config::default()).unwrap();
        fs::write(&config_path, default_config).expect("无法创建配置文件");
        log(LogLevel::Info, &format!("已创建默认配置文件: {}", config_path));
        return Config::default();
    }
    load_config_inner(&config_path).unwrap_or_else(|| {
        log(LogLevel::Error, "配置文件解析失败，使用默认配置");
        Config::default()
    })
}

pub fn load_config_inner(path: &str) -> Option<Config> {
    let content = fs::read_to_string(path).ok()?;
    serde_yaml::from_str::<Config>(&content).ok().map(|mut config| {
        if let Some(limit) = config.bandwidth_limit_mbps {
            if limit < MIN_BANDWIDTH_MBPS {
                log(
                    LogLevel::Warn,
                    &format!(
                        "带宽限制 {} MB/s 低于最小值 {} MB/s，自动调整",
                        limit, MIN_BANDWIDTH_MBPS
                    ),
                );
                config.bandwidth_limit_mbps = Some(MIN_BANDWIDTH_MBPS);
            }
        }
        config
    })
}

pub fn save_config(path: &str, config: &Config) {
    if let Ok(yaml) = serde_yaml::to_string(config) {
        if fs::write(path, yaml).is_ok() {
            log(LogLevel::Info, &format!("配置已保存到 {}", path));
        } else {
            log(LogLevel::Error, &format!("保存配置到 {} 失败", path));
        }
    }
}

// ===== 网络辅助 =====

pub fn local_ip() -> String {
    match local_ip_address::local_ip() {
        Ok(ip) => ip.to_string(),
        Err(_) => "127.0.0.1".to_string(),
    }
}

pub fn resolve_server(addr_str: &str) -> SocketAddr {
    // 先尝试 SRV 解析（支持不带端口的域名，查询 _mclink._tcp.{domain}）
    if let Some(addr) = resolve_address_srv(addr_str, DEFAULT_RELAY_PORT) {
        log(
            LogLevel::Info,
            &format!("服务器地址解析成功: {} -> {}", addr_str, addr),
        );
        return addr;
    }
    log(
        LogLevel::Error,
        &format!("服务器地址解析失败: {}", addr_str),
    );
    FALLBACK_CENTRAL_SERVER.parse().unwrap()
}

pub fn is_bandwidth_allowed(
    total_bytes_sent: &mut u64,
    last_bandwidth_check: &mut u64,
    bytes: usize,
    bandwidth_limit_mbps: Option<f64>,
    now_secs: u64,
) -> bool {
    if bandwidth_limit_mbps.is_none() {
        return true;
    }
    let limit = bandwidth_limit_mbps.unwrap();
    if now_secs - *last_bandwidth_check >= 1 {
        *total_bytes_sent = bytes as u64;
        *last_bandwidth_check = now_secs;
        return true;
    }
    *total_bytes_sent += bytes as u64;
    let current_mbps =
        (*total_bytes_sent * 8) as f64 / ((now_secs - *last_bandwidth_check + 1) as f64 * 1_000_000.0);
    current_mbps <= limit
}