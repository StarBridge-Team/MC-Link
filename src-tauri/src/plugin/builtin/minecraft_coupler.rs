//! 内置耦合类插件：Minecraft Java Edition 耦合器。
//!
//! 复用 WGP Core（`app_adapter/game_tcp.rs`）的局域网广播实现，把 Minecraft Java
//! 接入 MC Link 的本地调度入口：attach 时在局域网广播游戏入口（让同网设备可发现），
//! detach 时停止广播。同样仅依赖标准库，内联于此。
//!
//! # 刻意不实现 `coupler.launch`
//!
//! **MC Link 不启动游戏**（见 `CLAUDE.md` §1）。内置耦合器只负责"让游戏发现我们"，
//! 一旦它也能拉起 `java`，就等于把"联机工具"变成了启动器：
//!
//! - 游戏由用户的启动器启动，生命周期归用户；MC Link 抢这个职责只会和启动器打架；
//! - 拉起进程是 `proc_spawn_game` 这类高风险权限，内置实现不该握着它当基线。
//!
//! `coupler.launch` 这个**协议方法保留**：第三方启动器类插件可以实现它，那是它们与
//! 核心交流的通道（本插件的基线权限因此是 `net_connect_local`，不是 `proc_spawn_game`）。
//!
//! # 广播的端口是"玩家该连的那个端口"
//!
//! 房主侧是自己的游戏端口；**访客侧是引擎把房主端口接出来后的本机端口**。
//! 报文格式与"哪些广播属于我们自己"由 [`super::lan`] 统一裁定——扫描器读的是同一份契约，
//! 否则自己的广播会被自己当成一台局域网游戏发现出来。

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};

use crate::plugin::capability::BuiltinCapability;
use crate::plugin::manifest::{PluginKind, PluginManifest, RuntimeKind};
use crate::plugin::permission::PermissionSet;
use crate::plugin::protocol::{coupler_method, ErrorInfo};

use super::lan;

/// 插件 ID。
pub const PLUGIN_ID: &str = "dev.mclink.builtin.minecraft-java-coupler";

const MULTICAST_ADDR: &str = "224.0.2.60:4445";

/// 端口参数：`u64` → 合法 `u16`，越界视为未提供（不静默截断）。
fn port_from_params(params: &Value) -> Option<u16> {
    let p = params.get("port").and_then(|v| v.as_u64())?;
    (1..=65535).contains(&p).then_some(p as u16)
}

/// 在局域网持续广播游戏入口，直到 `running` 被置否。
fn lan_discovery_broadcaster(running: Arc<AtomicBool>, motd: String, port: u16) {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return,
    };
    if socket.set_broadcast(true).is_err() {
        return;
    }
    let message = lan::format_advert(&motd, port);
    let addr: SocketAddr = match MULTICAST_ADDR.parse() {
        Ok(a) => a,
        Err(_) => return,
    };
    while running.load(Ordering::Relaxed) {
        let _ = socket.send_to(message.as_bytes(), addr);
        std::thread::sleep(Duration::from_millis(1500));
    }
}

pub fn manifest() -> PluginManifest {
    let raw = json!({
        "id": PLUGIN_ID,
        "name": "Minecraft Java 耦合器",
        "version": "0.1.0",
        "kind": "coupler",
        "author": "MC Link 内置",
        "description": "将 Minecraft Java Edition 接入 MC Link 调度：在局域网广播游戏入口，供本机与同网玩家发现。不启动游戏。",
        "games": ["minecraft-java"],
        "priority": 50,
        // 不含 proc_spawn_game：内置耦合器不拉起游戏（见模块注释）
        "permissions": ["net_connect_local", "net_udp"],
        "runtime": { "kind": "builtin" }
    });
    let mut m = PluginManifest::parse(&raw.to_string())
        .expect("内置耦合器清单必须是合法清单（编译期常量）");
    m.runtime.kind = RuntimeKind::Builtin;
    m
}

/// 内置耦合器提供者。
pub struct MinecraftCouplerProvider {
    permissions: PermissionSet,
    running: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl MinecraftCouplerProvider {
    pub fn new() -> Self {
        Self {
            permissions: PermissionSet::from_declared(&manifest().permissions),
            running: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
        }
    }

    fn attach(&self, params: &Value) -> Result<Value, ErrorInfo> {
        let motd = params
            .get("motd")
            .and_then(|v| v.as_str())
            .unwrap_or(lan::MOTD)
            .to_string();
        // 端口缺失/非法时回退到 Minecraft 默认端口。
        //
        // 调用方（`mgr/room`）应当传入**适配器回报的接出端口**：访客侧那不是 25565，
        // 用默认值兜底只会广播出一个连不上的入口。
        let port = port_from_params(params).unwrap_or(25565);

        // 若已在广播，先停掉旧的再起新的。
        let _ = self.detach();

        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);
        let motd_c = motd.clone();
        let handle = std::thread::spawn(move || {
            lan_discovery_broadcaster(running, motd_c, port);
        });
        // 锁中毒不能让整个命令 panic（会打断 invoke）。
        // （`terracotta.rs` 用的是 `lock_or_err`，这里对齐同一约定。）
        *crate::plugin::utils::lock_or_err(&self.handle, "耦合器线程句柄")? = Some(handle);

        Ok(json!({ "attached": true, "motd": motd, "port": port }))
    }

    fn detach(&self) -> Result<Value, ErrorInfo> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = crate::plugin::utils::lock_or_err(&self.handle, "耦合器线程句柄")?.take() {
            let _ = h.join();
        }
        Ok(json!({ "attached": false }))
    }

    fn status(&self) -> Result<Value, ErrorInfo> {
        Ok(json!({ "attached": self.running.load(Ordering::SeqCst) }))
    }
}

impl BuiltinCapability for MinecraftCouplerProvider {
    fn plugin_id(&self) -> &str {
        PLUGIN_ID
    }

    fn kind(&self) -> PluginKind {
        PluginKind::Coupler
    }

    fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }

    fn invoke(&self, method: &str, params: Value) -> Result<Value, ErrorInfo> {
        match method {
            coupler_method::ATTACH => self.attach(&params),
            coupler_method::DETACH => self.detach(),
            coupler_method::STATUS => self.status(),
            // 明确拒绝而不是"不支持"：这条错误会出现在日志里，要说清是谁的选择
            coupler_method::LAUNCH => Err(ErrorInfo::not_supported(
                "MC Link 不启动游戏：内置耦合器只广播局域网入口。\
                 需要托管游戏进程请用实现了 coupler.launch 的第三方插件",
            )),
            other => Err(ErrorInfo::not_supported(format!(
                "Minecraft Java 耦合器不支持方法 {}",
                other
            ))),
        }
    }

    fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
        // 退出路径：锁中毒时无法安全 join，留日志直接返回，绝不 panic
        // （panic 会打断 `RunEvent::Exit` 的关停流程）。
        match self.handle.lock() {
            Ok(mut slot) => {
                if let Some(h) = slot.take() {
                    let _ = h.join();
                }
            }
            Err(_) => eprintln!("[耦合器] 线程句柄锁已中毒，跳过 join"),
        }
    }
}
