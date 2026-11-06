//! 内置适配器：**陶瓦联机（Terracotta）**。
//!
//! # 它不再拉起 `terracotta.exe`
//!
//! 旧实现把陶瓦的 exe 当成黑盒：起进程、轮询它的本地 HTTP、读 `/state`。那条路
//! 有三个治不好的问题——(1) 多一个要下载、要校验、要防替换的第三方可执行文件；
//! (2) 房间语义全在 exe 里，客户端无法解释状态；(3) 轮询式 HTTP 与我们的异步编排
//! 天生别扭。
//!
//! 现在的实现是**按上游协议原生重写**：自己起 EasyTier（[`crate::engine::easytier`]）、
//! 自己实现房间码、Scaffolding 握手与成员同步。协议语义来自上游源码
//! （`Terracotta/src/controller/rooms/scaffolding`），字段名与帧格式逐字节对齐，
//! 因此**与真实的陶瓦客户端互通**——这是本适配器存在的意义。
//!
//! # 与上游的已知差异
//!
//! 只有一条：不做本机游戏端口的存活轮询（上游每 5 秒 ping 一次，连续三次失败就判异常）。
//! 这一条不影响能否联机，只影响"游戏关掉后房间多久显示已断开"。
//!
//! 上游的另外两件事**在本项目里由别的插件负责**，不属于适配器：
//!
//! - 局域网 MOTD（"双击进入"入口）→ 耦合器（[`super::minecraft_coupler`]）；
//! - 局域网游戏发现 → 扫描器（[`super::minecraft_scanner`]）。
//!
//! 两者的报文格式与归属判定统一在 [`super::lan`] 里，适配器只需保证
//! `adapter.status` 回报的 `port` 是"玩家该连的那个端口"（房主=游戏端口，访客=本机接出端口）。
//!
//! # 一次只能有一个房间
//!
//! 与本项目其它内置适配器一致：适配器内部只持有一条链路。真正需要多房间时，
//! 每个房间会各自调一次 `host.start` / `join`，适配器会先停掉上一条——
//! 这一点写在这里，是因为它**不是**显而易见的（房间表允许多会话）。

mod guest;
mod host;
mod members;
mod room_code;
mod scaffold;
mod shared;
mod state;

use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

use crate::engine::easytier::{self, ProvisionState};
use crate::plugin::capability::BuiltinCapability;
use crate::plugin::manifest::{PluginKind, PluginManifest, RuntimeKind};
use crate::plugin::permission::PermissionSet;
use crate::plugin::protocol::{adapter_method, error_code, ErrorInfo};

pub use room_code::RoomCode;
use shared::Shared;
use state::SessionState;

/// 插件 ID。
pub const PLUGIN_ID: &str = "dev.mclink.builtin.terracotta";

/// 内置陶瓦适配器。
pub struct TerracottaProvider {
    shared: Arc<Shared>,
    permissions: PermissionSet,
}

impl TerracottaProvider {
    /// 构造。`data_dir` 用于落地机器标识与引擎工作目录。
    pub fn new(data_dir: &Path) -> Result<Self, String> {
        let machine_id = members::machine_id(data_dir);
        let shared = Shared::new(data_dir, SessionState::idle(machine_id));
        Ok(Self {
            shared,
            permissions: PermissionSet::from_declared(&manifest().permissions),
        })
    }

    /// 引擎状态（字符串形式，直接进 `adapter.status`）。
    fn engine_state(&self) -> &'static str {
        match easytier::state(&self.shared.data_dir) {
            ProvisionState::Ready => "ready",
            ProvisionState::Downloading => "downloading",
            ProvisionState::Missing => "missing",
        }
    }

    /// 当前状态投影。
    fn status(&self) -> Value {
        self.shared.snapshot().status_value(self.engine_state())
    }

    /// 结束当前房间（幂等）。
    fn stop(&self) {
        self.shared.stop_all();
        let machine_id = self.shared.snapshot().machine_id;
        self.shared.update(|session| *session = SessionState::idle(machine_id));
    }

    /// 取游戏端口（房主侧必需）。
    fn game_port(&self, params: &Value) -> Result<u16, ErrorInfo> {
        let raw = params
            .get("game")
            .and_then(|game| game.get("port"))
            .and_then(|port| port.as_u64())
            .unwrap_or(0);
        if (1..=65535).contains(&raw) {
            Ok(raw as u16)
        } else {
            Err(ErrorInfo::new(
                error_code::BAD_REQUEST,
                "缺少游戏端口：请确认本机游戏已启动，或在联机页重新选择游戏",
            ))
        }
    }

    /// 取玩家名。
    fn player_name(params: &Value) -> String {
        params
            .get("player_name")
            .and_then(|value| value.as_str())
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Player".to_string())
    }

    /// 建房。
    fn start_host(&self, params: &Value) -> Result<Value, ErrorInfo> {
        let game_port = self.game_port(params)?;
        let room = RoomCode::create();
        let player = Self::player_name(params);

        self.shared.begin();
        host::start(self.shared.clone(), &room, player, game_port).map_err(|e| {
            // 建链失败时引擎/服务端可能已经起来了：必须收掉，否则下一次建房会撞端口，
            // 而且用户会看到一个"明明失败了却占着网络"的残留进程
            self.stop();
            ErrorInfo::new(error_code::UNAVAILABLE, format!("创建房间失败：{}", e))
        })?;
        Ok(self.status())
    }

    /// 加入。
    fn start_guest(&self, params: &Value) -> Result<Value, ErrorInfo> {
        let text = params
            .get("room_code")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let room = RoomCode::parse(text).ok_or_else(|| {
            ErrorInfo::new(
                error_code::BAD_REQUEST,
                format!("房间码格式不正确（应形如 U/XXXX-XXXX-XXXX-XXXX）: {}", text),
            )
        })?;
        let player = Self::player_name(params);

        self.shared.begin();
        guest::join(self.shared.clone(), &room, player).map_err(|e| {
            // 同 start_host：加入失败也要把已拉起的引擎收掉（失败常发生在等对端超时，
            // 那时进程已经跑了两分钟）
            self.stop();
            ErrorInfo::new(error_code::UNAVAILABLE, format!("加入房间失败：{}", e))
        })?;
        Ok(self.status())
    }
}

/// 内置清单。
pub fn manifest() -> PluginManifest {
    let raw = json!({
        "id": PLUGIN_ID,
        "name": "陶瓦联机适配器",
        "version": "0.6.0",
        "kind": "adapter",
        "author": "BurningTNT / MC Link 内置实现",
        "description": "按陶瓦（Terracotta）协议原生实现的 P2P 联机适配器，可与上游陶瓦客户端互通。",
        "games": ["*"],
        // 高于通用 EasyTier 适配器：能与上游互通的链路应是默认选择
        "priority": 10,
        "permissions": [
            "net_listen_local",
            "net_connect_local",
            "net_connect_any",
            "net_udp",
            "proc_spawn_self",
            "fs_plugin_data"
        ],
        "runtime": { "kind": "builtin" },
        "limits": {
            "max_rpc_per_sec": 64,
            "heartbeat_ms": 5000,
            "idle_timeout_ms": 20000,
            // 建链要等对端上线，超时上限比其它适配器宽
            "rpc_timeout_ms": 240000
        }
    });
    let mut manifest = PluginManifest::parse(&raw.to_string())
        .expect("内置陶瓦清单必须是合法清单（编译期常量）");
    manifest.runtime.kind = RuntimeKind::Builtin;
    manifest
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
                easytier::ensure_background(&self.shared.data_dir);
                let engine = self.engine_state();
                Ok(json!({
                    "ready": engine == "ready",
                    "protocol": "terracotta",
                    "version": env!("CARGO_PKG_VERSION"),
                    "engine": {
                        "name": "easytier",
                        "version": easytier::artifact::VERSION,
                        "state": engine
                    },
                    "host_fields": [
                        {
                            "key": "room_code",
                            "label": "房间码",
                            "type": "text",
                            "generated": true,
                            "note": "由适配器自动生成，建好后回传给你分享"
                        }
                    ],
                    "join_fields": [
                        {
                            "key": "room_code",
                            "label": "房间码 / 邀请码",
                            "type": "text",
                            "required": true,
                            "autofill_from_invite": true,
                            "placeholder": "U/XXXX-XXXX-XXXX-XXXX"
                        }
                    ]
                }))
            }
            adapter_method::STATUS | adapter_method::PROBE => Ok(self.status()),
            adapter_method::HOST_START => self.start_host(&params),
            adapter_method::JOIN => self.start_guest(&params),
            adapter_method::HOST_STOP | adapter_method::LEAVE => {
                self.stop();
                Ok(self.status())
            }
            adapter_method::INSTALL => {
                easytier::ensure_blocking(&self.shared.data_dir, |_| {})
                    .map_err(|e| ErrorInfo::new(error_code::UNAVAILABLE, e))?;
                Ok(self.status())
            }
            other => Err(ErrorInfo::not_supported(format!(
                "陶瓦适配器不支持方法 {}",
                other
            ))),
        }
    }

    fn shutdown(&self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclink-terracotta-entry-{}-{}",
            name,
            crate::plugin::crypto::random_hex(4)
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn manifest_is_valid_and_builtin() {
        let manifest = manifest();
        assert_eq!(manifest.id, PLUGIN_ID);
        assert_eq!(manifest.kind, PluginKind::Adapter);
        assert_eq!(manifest.runtime.kind, RuntimeKind::Builtin);
        assert!(manifest.covers_game("minecraft-java"));
        assert!(manifest.covers_game("terraria"));
    }

    #[test]
    fn init_declares_generated_host_field_and_invite_field() {
        let dir = temp_dir("init");
        let provider = TerracottaProvider::new(&dir).unwrap();
        let value = provider.invoke(adapter_method::INIT, json!({})).unwrap();

        let host_fields = value["host_fields"].as_array().unwrap();
        assert_eq!(host_fields[0]["key"], "room_code");
        assert_eq!(host_fields[0]["generated"], true);

        let join_fields = value["join_fields"].as_array().unwrap();
        assert_eq!(join_fields[0]["key"], "room_code");
        assert_eq!(join_fields[0]["autofill_from_invite"], true);
        assert_eq!(join_fields[0]["required"], true);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_before_any_link_is_waiting() {
        let dir = temp_dir("status");
        let provider = TerracottaProvider::new(&dir).unwrap();
        let value = provider.invoke(adapter_method::STATUS, json!({})).unwrap();
        assert_eq!(value["phase"], "waiting");
        assert_eq!(value["running"], false);
        assert_eq!(value["port"], Value::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn join_rejects_malformed_room_code() {
        let dir = temp_dir("bad-code");
        let provider = TerracottaProvider::new(&dir).unwrap();
        let err = provider
            .invoke(adapter_method::JOIN, json!({ "room_code": "nonsense" }))
            .unwrap_err();
        assert_eq!(err.code, error_code::BAD_REQUEST);
        assert!(err.message.contains("房间码格式不正确"), "{}", err.message);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_start_requires_game_port() {
        let dir = temp_dir("no-port");
        let provider = TerracottaProvider::new(&dir).unwrap();
        let err = provider
            .invoke(adapter_method::HOST_START, json!({}))
            .unwrap_err();
        assert_eq!(err.code, error_code::BAD_REQUEST);
        assert!(err.message.contains("缺少游戏端口"), "{}", err.message);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn leave_resets_state_to_idle() {
        let dir = temp_dir("leave");
        let provider = TerracottaProvider::new(&dir).unwrap();
        provider.invoke(adapter_method::LEAVE, json!({})).unwrap();
        let value = provider.invoke(adapter_method::STATUS, json!({})).unwrap();
        assert_eq!(value["phase"], "waiting");
        assert_eq!(value["room"], "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
