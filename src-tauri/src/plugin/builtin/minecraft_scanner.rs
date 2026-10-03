//! 内置检测类插件：Minecraft Java Edition 局域网扫描器。
//!
//! 复用 WGP Core（`app_adapter/game_tcp.rs`）里扫描 Minecraft Java Edition 局域网
//! 服务器的实现：通过 UDP 多播 MOTD 协议（224.0.2.60:4445）发现本网内开启的
//! Java 版服务器。这段代码自包含，仅依赖标准库 + serde，因此直接内联于此，
//! 不引入整个 wgp-core 依赖（也规避其可能带入的 eframe/egui feature 冲突）。

use std::collections::HashMap;
use std::net::UdpSocket;
use std::time::Duration;

use serde_json::{json, Value};

use crate::plugin::capability::BuiltinCapability;
use crate::plugin::manifest::{PluginKind, PluginManifest, RuntimeKind};
use crate::plugin::permission::PermissionSet;
use crate::plugin::protocol::{detector_method, error_code, ErrorInfo};

/// 插件 ID。
pub const PLUGIN_ID: &str = "dev.mclink.builtin.minecraft-java-scanner";

const MULTICAST_IP: &str = "224.0.2.60";
const MULTICAST_PORT: u16 = 4445;

/// 一个被发现的服务器的最小信息。
#[derive(Clone, Debug)]
struct LanServer {
    motd: String,
    port: u16,
}

fn extract_tag(s: &str, start: &str, end: &str) -> String {
    if let Some(start_pos) = s.find(start) {
        if let Some(end_pos) = s.find(end) {
            return s[start_pos + start.len()..end_pos].to_string();
        }
    }
    String::new()
}

/// 通过 UDP 多播协议扫描局域网内开启的 Minecraft Java Edition 服务器。
fn scan_lan_servers() -> Result<Vec<LanServer>, String> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", MULTICAST_PORT))
        .map_err(|e| format!("绑定多播端口失败: {}", e))?;
    socket
        .join_multicast_v4(&MULTICAST_IP.parse().unwrap(), &"0.0.0.0".parse().unwrap())
        .map_err(|e| e.to_string())?;
    socket.set_read_timeout(Some(Duration::from_secs(3))).ok();

    let mut servers: HashMap<u16, LanServer> = HashMap::new();
    let mut buf = [0u8; 1024];
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok((len, _)) = socket.recv_from(&mut buf) {
            if let Ok(data) = String::from_utf8(buf[..len].to_vec()) {
                if data.contains("[MOTD]") && data.contains("[AD]") {
                    let motd = extract_tag(&data, "[MOTD]", "[/MOTD]");
                    let port_str = extract_tag(&data, "[AD]", "[/AD]");
                    if let Ok(port) = port_str.parse::<u16>() {
                        servers.insert(port, LanServer { motd, port });
                    }
                }
            }
        }
    }
    Ok(servers.into_values().collect())
}

pub fn manifest() -> PluginManifest {
    let raw = json!({
        "id": PLUGIN_ID,
        "name": "Minecraft Java 局域网扫描器",
        "version": "0.1.0",
        "kind": "detector",
        "author": "MC Link 内置",
        "description": "通过 UDP 多播 MOTD 协议扫描局域网内开启的 Minecraft Java Edition 服务器。",
        "games": ["minecraft-java"],
        "priority": 50,
        "permissions": ["game_scan", "net_udp"],
        "runtime": { "kind": "builtin" }
    });
    let mut m = PluginManifest::parse(&raw.to_string())
        .expect("内置扫描器清单必须是合法清单（编译期常量）");
    m.runtime.kind = RuntimeKind::Builtin;
    m
}

/// 内置检测器提供者。
pub struct MinecraftScannerProvider {
    permissions: PermissionSet,
}

impl MinecraftScannerProvider {
    pub fn new() -> Self {
        Self {
            permissions: PermissionSet::from_declared(&manifest().permissions),
        }
    }

    fn scan(&self) -> Result<Value, ErrorInfo> {
        let servers = scan_lan_servers().map_err(|e| ErrorInfo::new(error_code::UNAVAILABLE, e))?;
        // 返回全部发现的服务器（可能为空数组）。联机页房主模式据此展示卡片，
        // 空数组表示"没扫到游戏"，由前端呈现空状态，而非报错。
        let list: Vec<Value> = servers
            .into_iter()
            .map(|s| {
                json!({
                    "id": "minecraft-java",
                    "process": s.motd,
                    "name": "Minecraft Java Edition",
                    "port": s.port,
                })
            })
            .collect();
        Ok(json!(list))
    }
}

impl BuiltinCapability for MinecraftScannerProvider {
    fn plugin_id(&self) -> &str {
        PLUGIN_ID
    }

    fn kind(&self) -> PluginKind {
        PluginKind::Detector
    }

    fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }

    fn invoke(&self, method: &str, _params: Value) -> Result<Value, ErrorInfo> {
        match method {
            detector_method::SCAN => self.scan(),
            detector_method::WATCH_START | detector_method::WATCH_STOP => Err(ErrorInfo::not_supported(
                "当前扫描器仅支持一次性扫描（detector.scan）",
            )),
            other => Err(ErrorInfo::not_supported(format!(
                "Minecraft Java 扫描器不支持方法 {}",
                other
            ))),
        }
    }
}
