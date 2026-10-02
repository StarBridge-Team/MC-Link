//! 内置适配器插件：陶瓦联机（Terracotta）。
//!
//! # 为什么是"内置插件"而不是"外部插件"
//!
//! 陶瓦是一个第三方 exe，它自己的对外接口是**本地 HTTP**，我们无法在不修改
//! 上游源码的前提下让它去说 WebSocket。硬套外部插件协议只会把适配层写得更脏。
//!
//! 因此做法是：把陶瓦的 exe 生命周期与 HTTP 轮询**收窄**成一个标准的
//! [`BuiltinCapability`]（具备 `adapter.*` 方法集），再由 [`Provider`] 把它
//! 和真正的外部 WebSocket 插件放在同一个调用面上。这样：
//!
//! - 调度层（[`crate::plugin::manager`]）完全不知道陶瓦的存在；
//! - 未来用 WebSocket 插件替换/并存时，上层零改动；
//! - 权限模型对内置插件同样生效，不存在"自己人开后门"的路径。
//!
//! 旧的 `adapter.rs` / `commands/adapter.rs` 面对的 Tauri 命令与磁盘状态文件
//! 保持不变，前端无需改动。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{Duration, Instant};

use crate::adapter::AdapterManager;
use crate::plugin::capability::BuiltinCapability;
use crate::plugin::manifest::{PluginKind, PluginManifest, RuntimeKind};
use crate::plugin::permission::PermissionSet;
use crate::plugin::protocol::{adapter_method, error_code, ErrorInfo};
use crate::plugin::utils::lock_or_err;

/// 插件 ID。
pub const PLUGIN_ID: &str = "dev.mclink.builtin.terracotta";

/// 陶瓦状态机（与上游 `/state` 接口一一对应）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerracottaState {
    Waiting,
    HostScanning,
    HostStarting,
    HostOk,
    GuestConnecting,
    GuestStarting,
    GuestOk,
    Exception,
    Unknown,
}

impl TerracottaState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TerracottaState::HostOk | TerracottaState::GuestOk | TerracottaState::Exception
        )
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "waiting" => TerracottaState::Waiting,
            "host-scanning" => TerracottaState::HostScanning,
            "host-starting" => TerracottaState::HostStarting,
            "host-ok" => TerracottaState::HostOk,
            "guest-connecting" => TerracottaState::GuestConnecting,
            "guest-starting" => TerracottaState::GuestStarting,
            "guest-ok" => TerracottaState::GuestOk,
            "exception" => TerracottaState::Exception,
            _ => TerracottaState::Unknown,
        }
    }
}

/// 内置清单。内置插件的清单来自核心代码，因此天然可信。
pub fn manifest() -> PluginManifest {
    let raw = json!({
        "id": PLUGIN_ID,
        "name": "陶瓦联机适配器",
        "version": "0.4.2",
        "kind": "adapter",
        "author": "BurningTNT / MC Link 内置封装",
        "description": "基于陶瓦联机（Terracotta）的 P2P 打洞适配器，负责房间创建与加入。",
        "games": ["*"],
        "priority": 10,
        "permissions": [
            "net_listen_local",
            "net_connect_local",
            // HOST_START / JOIN / PROBE 在 permission.rs::required_for 中要求 NetConnectAny，
            // 清单缺少该项会导致 start_terracotta_host / start_terracotta_guest 必被核心拒绝。
            "net_connect_any",
            "proc_spawn_self",
            "fs_plugin_data"
        ],
        "runtime": { "kind": "builtin" },
        "limits": {
            "max_rpc_per_sec": 64,
            "heartbeat_ms": 5000,
            "idle_timeout_ms": 20000,
            "rpc_timeout_ms": 120000
        }
    });
    let mut m = PluginManifest::parse(&raw.to_string())
        .expect("内置陶瓦清单必须是合法清单（这是编译期常量）");
    m.runtime.kind = RuntimeKind::Builtin;
    m
}

/// 内置适配器提供者。
pub struct TerracottaProvider {
    manager: Arc<SyncMutex<AdapterManager>>,
    http: reqwest::blocking::Client,
    permissions: PermissionSet,
}

impl TerracottaProvider {
    pub fn new(manager: Arc<SyncMutex<AdapterManager>>) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("创建陶瓦 HTTP 客户端失败: {}", e))?;
        // 内置插件不走信任度封顶（清单来自核心代码），但仍走统一的权限判定路径。
        let permissions = PermissionSet::from_declared(&manifest().permissions);
        Ok(Self {
            manager,
            http,
            permissions,
        })
    }

    /// 确保陶瓦已安装并运行，返回其本地 HTTP 端口。
    fn port(&self) -> Result<u16, ErrorInfo> {
        let mgr = lock_or_err(&self.manager, "AdapterManager")?;
        mgr.ensure_running()
            .map_err(|e| ErrorInfo::new(error_code::UNAVAILABLE, e))
    }

    fn get_json(&self, path: &str) -> Result<Value, ErrorInfo> {
        let port = self.port()?;
        let url = format!("http://127.0.0.1:{}{}", port, path);
        let resp =
            self.http.get(&url).send().map_err(|e| {
                ErrorInfo::new(error_code::UNAVAILABLE, format!("请求陶瓦失败: {}", e))
            })?;
        if !resp.status().is_success() {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("陶瓦返回 HTTP {}", resp.status()),
            ));
        }
        resp.json::<Value>()
            .map_err(|e| ErrorInfo::new(error_code::INTERNAL, format!("解析陶瓦响应失败: {}", e)))
    }

    /// 轮询到终态或超时。
    fn poll_state(&self, timeout_secs: u64, tick_ms: u64) -> Result<Value, ErrorInfo> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let mut last_error: Option<ErrorInfo> = None;
        loop {
            if start.elapsed() > timeout {
                return Err(last_error.unwrap_or_else(|| {
                    ErrorInfo::new(error_code::TIMEOUT, "等待陶瓦联机状态超时")
                }));
            }
            match self.get_json("/state") {
                Ok(state) => {
                    let ts = TerracottaState::from_str(
                        state.get("state").and_then(|v| v.as_str()).unwrap_or(""),
                    );
                    if ts.is_terminal() || ts == TerracottaState::Unknown {
                        return Ok(state);
                    }
                }
                Err(e) => last_error = Some(e),
            }
            std::thread::sleep(Duration::from_millis(tick_ms));
        }
    }

    /// 把房间/玩家/游戏信息编码成陶瓦的参数。
    fn terracotta_args(params: &Value) -> Vec<(String, String)> {
        let mut args = Vec::new();
        if let Some(code) = params.get("room_code").and_then(|v| v.as_str()) {
            if !code.is_empty() {
                args.push(("room".to_string(), code.to_string()));
            }
        }
        if let Some(player) = params.get("player_name").and_then(|v| v.as_str()) {
            if !player.is_empty() {
                args.push(("player".to_string(), player.to_string()));
            }
        }
        // 游戏信息：陶瓦自身不含游戏概念，但端口信息会影响它的扫描行为。
        if let Some(port) = params
            .get("game")
            .and_then(|g| g.get("port"))
            .and_then(|p| p.as_u64())
        {
            args.push(("port".to_string(), port.to_string()));
        }
        if let Some(nodes) = params.get("public_nodes").and_then(|v| v.as_array()) {
            for node in nodes.iter().filter_map(|n| n.as_str()) {
                args.push(("public_nodes".to_string(), node.to_string()));
            }
        }
        args
    }

    fn start_host(&self, params: Value) -> Result<Value, ErrorInfo> {
        // 进入任何操作前先重置状态，避免上一次联机的残留状态污染判定。
        self.get_json("/state/ide")?;
        let query = Self::terracotta_args(&params);
        let port = self.port()?;
        let url = format!("http://127.0.0.1:{}/state/scanning", port);
        self.http.get(&url).query(&query).send().map_err(|e| {
            ErrorInfo::new(error_code::UNAVAILABLE, format!("启动陶瓦主机失败: {}", e))
        })?;
        self.poll_state(120, 500)
    }

    fn start_guest(&self, params: Value) -> Result<Value, ErrorInfo> {
        let room = params
            .get("room_code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorInfo::new(error_code::BAD_REQUEST, "缺少房间码"))?;
        if room.is_empty() {
            return Err(ErrorInfo::new(error_code::BAD_REQUEST, "房间码不能为空"));
        }
        self.get_json("/state/ide")?;
        let query = Self::terracotta_args(&params);
        let port = self.port()?;
        let url = format!("http://127.0.0.1:{}/state/guesting", port);
        let resp = self.http.get(&url).query(&query).send().map_err(|e| {
            ErrorInfo::new(error_code::UNAVAILABLE, format!("加入陶瓦房间失败: {}", e))
        })?;
        if !resp.status().is_success() {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("加入房间被拒绝（HTTP {}）", resp.status()),
            ));
        }
        self.poll_state(120, 500)
    }

    /// 和平退出当前房间/主机。
    fn panic_peaceful(&self) -> Result<Value, ErrorInfo> {
        let port = self.port()?;
        let url = format!("http://127.0.0.1:{}/panic?peaceful=true", port);
        let _ = self.http.get(&url).send();
        Ok(json!({ "stopped": true }))
    }

    /// 汇总状态：既有供前端直接消费的字段，也带上陶瓦自身状态。
    fn status(&self) -> Result<Value, ErrorInfo> {
        let status = {
            let mgr = lock_or_err(&self.manager, "AdapterManager")?;
            mgr.get_status()
        };

        let state = match status.port {
            Some(_) => self.get_json("/state").unwrap_or(Value::Null),
            None => json!({ "state": "waiting", "index": 0 }),
        };

        Ok(json!({
            "installed": status.installed,
            "running": status.running,
            "starting": status.starting,
            "port": status.port,
            "state": state,
        }))
    }
}

impl BuiltinCapability for TerracottaProvider {
    fn plugin_id(&self) -> &str {
        PLUGIN_ID
    }

    fn kind(&self) -> PluginKind {
        PluginKind::Adapter
    }

    fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }

    fn invoke(&self, method: &str, params: Value) -> Result<Value, ErrorInfo> {
        match method {
            adapter_method::INIT => {
                let mgr = lock_or_err(&self.manager, "AdapterManager")?;
                Ok(json!({
                    "ready": mgr.get_status().running,
                    "protocol": "terracotta",
                    "version": env!("CARGO_PKG_VERSION"),
                }))
            }
            adapter_method::STATUS | adapter_method::PROBE => self.status(),
            adapter_method::HOST_START => self.start_host(params),
            adapter_method::JOIN => self.start_guest(params),
            adapter_method::HOST_STOP | adapter_method::LEAVE => self.panic_peaceful(),
            adapter_method::INSTALL => {
                let mgr = lock_or_err(&self.manager, "AdapterManager")?;
                mgr.launch_terracotta_after_download();
                self.status()
            }
            other => Err(ErrorInfo::not_supported(format!(
                "陶瓦适配器不支持方法 {}",
                other
            ))),
        }
    }

    fn shutdown(&self) {
        if let Ok(mgr) = self.manager.lock() {
            mgr.shutdown_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_valid_and_builtin() {
        let m = manifest();
        assert_eq!(m.id, PLUGIN_ID);
        assert_eq!(m.kind, PluginKind::Adapter);
        assert_eq!(m.runtime.kind, RuntimeKind::Builtin);
        assert!(m.covers_game("minecraft-java"));
        assert!(m.covers_game("terraria"));
    }

    #[test]
    fn state_parsing_matches_upstream_strings() {
        assert_eq!(
            TerracottaState::from_str("host-ok"),
            TerracottaState::HostOk
        );
        assert_eq!(
            TerracottaState::from_str("guest-ok"),
            TerracottaState::GuestOk
        );
        assert_eq!(
            TerracottaState::from_str("exception"),
            TerracottaState::Exception
        );
        assert_eq!(TerracottaState::from_str("???"), TerracottaState::Unknown);
        assert!(TerracottaState::HostOk.is_terminal());
        assert!(!TerracottaState::HostScanning.is_terminal());
    }

    #[test]
    fn terracotta_args_skip_empty_values() {
        let params = json!({
            "room_code": "",
            "player_name": "Steve",
            "game": { "port": 25565 },
            "public_nodes": ["node1", "node2"]
        });
        let args = TerracottaProvider::terracotta_args(&params);
        assert!(!args.iter().any(|(k, _)| k == "room"));
        assert!(args.iter().any(|(k, v)| k == "player" && v == "Steve"));
        assert!(args.iter().any(|(k, v)| k == "port" && v == "25565"));
        assert_eq!(args.iter().filter(|(k, _)| k == "public_nodes").count(), 2);
    }
}
