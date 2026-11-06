//! 内置适配器：**通用 EasyTier 房间**。
//!
//! # 它是什么
//!
//! 把一个 EasyTier 虚拟网络当作"房间"：房主生成房间码（网络名 + 密钥 + 游戏端口），
//! 访客凭码加入，自己这边得到一个指向房主游戏端口的本地入口。
//! 不含任何游戏协议、不依赖陶瓦的那套 Scaffolding 握手——这是"引擎级"的最小联机。
//!
//! # 与陶瓦适配器的分工
//!
//! | | 通用 EasyTier | 陶瓦适配器 |
//! |---|---|---|
//! | 房间码 | 本项目自定义（含端口） | `U/XXXX-XXXX-XXXX-XXXX`（与上游互通） |
//! | 与上游陶瓦客户端互通 | 不能 | 能 |
//! | 端口/成员发现 | 不需要（端口在码里） | 需要 Scaffolding 协议 |
//!
//! 两者都只是同一个引擎之上的策略，因此路由里陶瓦排在前（`priority` 更高）：
//! 能与上游互通的那条链路应该是默认选择。

use serde_json::{json, Value};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::engine::easytier::{self, Forward, LaunchSpec, Link, Proto, ProvisionState};
use crate::plugin::capability::BuiltinCapability;
use crate::plugin::manifest::{PluginKind, PluginManifest, RuntimeKind};
use crate::plugin::permission::PermissionSet;
use crate::plugin::protocol::{adapter_method, error_code, ErrorInfo};

use super::support;

mod room_code;
mod status;
pub use room_code::RoomCode;

/// 插件 ID。
pub const PLUGIN_ID: &str = "dev.mclink.builtin.easytier";

/// 网络名前缀。带前缀是为了在 EasyTier 的公共节点上不至于和别人的网络撞名。
const NET_PREFIX: &str = "mclink-";

/// 房间码各段长度（字符数）。
const NET_CHARS: usize = 8;
const SECRET_CHARS: usize = 8;
const PORT_CHARS: usize = 4;

/// 访客等房主上线的上限。打洞与公共节点发现都可能慢，60 秒是"用户还愿意等"的量级。
const HOST_WAIT: Duration = Duration::from_secs(60);


/// 适配器当前持有的一条链路。
struct Active {
    link: Link,
    room: RoomCode,
    role: &'static str,
    player_name: String,
    /// 访客侧：本机接出端口。
    local_port: Option<u16>,
    workdir: PathBuf,
}

/// 内置适配器提供者。
pub struct EasyTierProvider {
    data_dir: PathBuf,
    active: Mutex<Option<Active>>,
    permissions: PermissionSet,
}

impl EasyTierProvider {
    pub fn new(data_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            active: Mutex::new(None),
            permissions: PermissionSet::from_declared(&manifest().permissions),
        })
    }

    /// 当前链路（锁中毒时按"没有链路"处理）。
    ///
    /// 传入的是 `&mut Option<Active>` 而不是 `Option<&mut Active>`：调用方既要读
    /// 也要整体替换（起新链路 / 停掉旧链路），只给 `Option<&mut _>` 就换不了槽位。
    fn with_active<T>(&self, f: impl FnOnce(&mut Option<Active>) -> T) -> T {
        let mut guard = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    /// 结束现有链路（幂等）。
    fn stop(&self) {
        self.with_active(|slot| {
            if let Some(mut active) = slot.take() {
                active.link.stop();
                // 工作目录只服务这一次建链，进程停了就可以整个丢掉
                let _ = std::fs::remove_dir_all(&active.workdir);
            }
        });
    }

    /// 起一条新链路（先停旧的）。
    fn launch(
        &self,
        room: &RoomCode,
        role: &'static str,
        player_name: &str,
        ipv4: Ipv4Addr,
        hostname: &str,
        whitelist: &[u16],
        forwards: Vec<Forward>,
    ) -> Result<Option<u16>, ErrorInfo> {
        self.stop();

        let bins = support::ensure_engine(&self.data_dir)
            .map_err(|e| ErrorInfo::new(error_code::UNAVAILABLE, e))?;

        let spec = LaunchSpec {
            network_name: room.network_name.clone(),
            network_secret: room.network_secret.clone(),
            hostname: hostname.to_string(),
            instance_name: "mclink".to_string(),
            ipv4,
            peers: easytier::PUBLIC_NODES
                .iter()
                .map(|node| node.to_string())
                .collect(),
            tcp_whitelist: whitelist.to_vec(),
            udp_whitelist: whitelist.to_vec(),
            forwards: forwards.clone(),
            rpc_port: easytier::free_loopback_port(),
            // 通用适配器允许中继兜底：P2P 打不通时宁可慢一点也要连上
            p2p_only: false,
        };

        let workdir = self
            .data_dir
            .join("Engine")
            .join("run")
            .join(format!("easytier-{}", crate::plugin::crypto::random_hex(4)));
        let link = Link::spawn(&bins, &spec, &workdir)
            .map_err(|e| ErrorInfo::new(error_code::UNAVAILABLE, e))?;

        // 访客：等到真的看见房主再报"已连接"。
        //
        // 不等的话，房间会在房主还没上线（或房间码打错）时显示"已连接"，
        // 而游戏怎么都连不上——这比直接说"还在等待"难排查得多。
        if role == "guest" {
            wait_for_host(&link).map_err(|e| {
                // 失败时 link 在这里被 drop，Drop 会杀掉进程
                e
            })?;
        }

        let local_port = forwards.first().map(|forward| forward.bind.port());

        self.with_active(|slot| {
            *slot = Some(Active {
                link,
                room: room.clone(),
                role,
                player_name: player_name.to_string(),
                local_port,
                workdir,
            });
        });

        Ok(local_port)
    }

    /// 汇总状态（前端契约：`phase` 以 `-ok` 结尾表示可用）。
    fn status_value(&self) -> Value {
        let engine = engine_state(&self.data_dir);
        self.with_active(|slot| {
            let Some(active) = slot.as_mut() else {
                return status::idle_value(engine);
            };
            let alive = active.link.is_alive();
            let peers = if alive {
                active.link.peers().unwrap_or_default()
            } else {
                Vec::new()
            };
            status::status_value(status::View {
                engine,
                alive,
                role: active.role,
                player_name: &active.player_name,
                room: active.room.display(),
                local_port: active.local_port,
                peers: &peers,
            })
        })
    }
}

/// 等房主出现在对端列表里（最多 [`HOST_WAIT`]）。
///
/// 判定依据是"有一个非本机对端占着固定的房主虚拟地址"——通用适配器的房主地址是常量，
/// 因此不需要问任何元数据。
fn wait_for_host(link: &Link) -> Result<(), ErrorInfo> {
    let started = Instant::now();
    loop {
        let peers = link.peers().unwrap_or_default();
        let found = peers
            .iter()
            .any(|peer| !peer.is_local() && peer.ipv4_addr() == Some(support::HOST_IPV4));
        if found {
            return Ok(());
        }
        if started.elapsed() > HOST_WAIT {
            return Err(ErrorInfo::new(
                error_code::TIMEOUT,
                format!(
                    "等待房主上线超时（{} 秒）。请确认房间码正确、房主仍在房间里，\
                     且双方都能连上 EasyTier 公共节点。",
                    HOST_WAIT.as_secs()
                ),
            ));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// 引擎状态 → `adapter.status` 里的字符串取值。
fn engine_state(data_dir: &Path) -> &'static str {
    match easytier::state(data_dir) {
        ProvisionState::Ready => "ready",
        ProvisionState::Downloading => "downloading",
        ProvisionState::Missing => "missing",
    }
}

/// 从调用参数里取游戏端口。
fn game_port(params: &Value) -> Option<u16> {
    let raw = params.get("game")?.get("port")?.as_u64()?;
    (1..=65535).contains(&raw).then_some(raw as u16)
}

/// 内置清单。
pub fn manifest() -> PluginManifest {
    let raw = json!({
        "id": PLUGIN_ID,
        "name": "EasyTier 联机引擎",
        "version": "0.6.0",
        "kind": "adapter",
        "author": "MC Link 内置",
        "description": "基于 EasyTier 的通用 P2P 组网适配器：房主生成房间码，访客凭码接入房主的游戏端口。",
        "games": ["*"],
        // 低于陶瓦：能与上游陶瓦客户端互通的那条链路应是默认选择
        "priority": 5,
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
            "rpc_timeout_ms": 180000
        }
    });
    let mut manifest = PluginManifest::parse(&raw.to_string())
        .expect("内置 EasyTier 清单必须是合法清单（编译期常量）");
    manifest.runtime.kind = RuntimeKind::Builtin;
    manifest
}

impl BuiltinCapability for EasyTierProvider {
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
                // 联机页一打开就开始准备引擎：真正点"开始联机"时通常已经装好
                easytier::ensure_background(&self.data_dir);
                let engine = engine_state(&self.data_dir);
                Ok(json!({
                    "ready": engine == "ready",
                    "protocol": "easytier",
                    "version": env!("CARGO_PKG_VERSION"),
                    "engine": {
                        "name": "easytier",
                        "version": easytier::artifact::VERSION,
                        "state": engine,
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
                            "placeholder": "XXXX-XXXX-XXXX-XXXX"
                        }
                    ]
                }))
            }
            adapter_method::STATUS | adapter_method::PROBE => Ok(self.status_value()),
            adapter_method::HOST_START => {
                let port = game_port(&params).ok_or_else(|| {
                    ErrorInfo::new(
                        error_code::BAD_REQUEST,
                        "缺少游戏端口：请确认本机游戏已启动，或在联机页重新选择游戏",
                    )
                })?;
                let room = RoomCode::generate(port);
                let player = player_name(&params);
                self.launch(
                    &room,
                    "host",
                    &player,
                    support::HOST_IPV4,
                    "mclink-host",
                    &[port],
                    Vec::new(),
                )?;
                Ok(self.status_value())
            }
            adapter_method::JOIN => {
                let text = params
                    .get("room_code")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let room = RoomCode::parse(text).ok_or_else(|| {
                    ErrorInfo::new(
                        error_code::BAD_REQUEST,
                        format!("房间码格式不正确（应为 XX-XX-XX-XX 形式）: {}", text),
                    )
                })?;

                // 本机端口优先与主机端口同号（模组常按端口 +1 用 UDP），被占则换
                let local_port = support::pick_local_port(room.port);
                let forward = Forward {
                    proto: Proto::Tcp,
                    bind: SocketAddr::from((Ipv4Addr::LOCALHOST, local_port)),
                    dst: SocketAddr::from((support::HOST_IPV4, room.port)),
                };
                let player = player_name(&params);
                self.launch(
                    &room,
                    "guest",
                    &player,
                    support::guest_ipv4(),
                    "mclink-guest",
                    &[],
                    vec![forward],
                )?;
                Ok(self.status_value())
            }
            adapter_method::HOST_STOP | adapter_method::LEAVE => {
                self.stop();
                Ok(self.status_value())
            }
            adapter_method::INSTALL => {
                support::ensure_engine(&self.data_dir)
                    .map_err(|e| ErrorInfo::new(error_code::UNAVAILABLE, e))?;
                Ok(self.status_value())
            }
            other => Err(ErrorInfo::not_supported(format!(
                "EasyTier 适配器不支持方法 {}",
                other
            ))),
        }
    }

    fn shutdown(&self) {
        self.stop();
    }
}

/// 取玩家名（缺省时给一个可辨认的占位名）。
fn player_name(params: &Value) -> String {
    params
        .get("player_name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Player".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_code_roundtrips_through_display() {
        let room = RoomCode::generate(25565);
        let text = room.display();
        let parsed = RoomCode::parse(&text).expect("自己生成的码必须能解析");
        assert_eq!(parsed.network_name, room.network_name);
        assert_eq!(parsed.network_secret, room.network_secret);
        assert_eq!(parsed.port, 25565);
    }

    #[test]
    fn parse_tolerates_case_and_separators() {
        let room = RoomCode::generate(30000);
        let text = room.display().to_lowercase().replace('-', " ");
        let parsed = RoomCode::parse(&text).expect("应容忍小写与空格");
        assert_eq!(parsed.port, 30000);
        assert_eq!(parsed.network_name, room.network_name);
    }

    #[test]
    fn parse_rejects_wrong_length_or_garbage() {
        assert!(RoomCode::parse("").is_none());
        assert!(RoomCode::parse("ABCD-EFGH").is_none());
        // 长度对但端口段越界（34^4 超出 u16）
        let too_big = format!("{}-{}", "0".repeat(16), "ZZZZ");
        assert!(RoomCode::parse(&too_big).is_none());
    }

    #[test]
    fn network_name_carries_prefix() {
        let room = RoomCode::generate(25565);
        assert!(room.network_name.starts_with(NET_PREFIX));
        assert_eq!(
            room.network_name.trim_start_matches(NET_PREFIX).len(),
            NET_CHARS
        );
        assert_eq!(room.network_secret.len(), SECRET_CHARS);
    }

    #[test]
    fn display_shape_is_four_char_groups() {
        let room = RoomCode::generate(25565);
        let text = room.display();
        let parts: Vec<&str> = text.split('-').collect();
        assert_eq!(parts.len(), 5, "形如 XXXX-XXXX-XXXX-XXXX-PPPP: {}", text);
        assert_eq!(parts[0].len(), 4);
        assert_eq!(parts[4].len(), PORT_CHARS);
    }

    #[test]
    fn manifest_is_valid_and_builtin() {
        let manifest = manifest();
        assert_eq!(manifest.id, PLUGIN_ID);
        assert_eq!(manifest.kind, PluginKind::Adapter);
        assert_eq!(manifest.runtime.kind, RuntimeKind::Builtin);
        assert!(manifest.covers_game("minecraft-java"));
    }

    #[test]
    fn game_port_is_range_checked() {
        assert_eq!(game_port(&json!({ "game": { "port": 25565 } })), Some(25565));
        assert_eq!(game_port(&json!({ "game": { "port": 0 } })), None);
        assert_eq!(game_port(&json!({ "game": { "port": 70000 } })), None);
        assert_eq!(game_port(&json!({})), None);
    }
}
