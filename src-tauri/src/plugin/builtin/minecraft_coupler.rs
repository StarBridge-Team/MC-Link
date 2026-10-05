//! 内置耦合类插件：Minecraft Java Edition 耦合器。
//!
//! 复用 WGP Core（`app_adapter/game_tcp.rs`）的局域网广播实现，把 Minecraft Java
//! 接入 MC Link 的本地调度入口：attach 时在局域网广播游戏入口（让同网设备可发现），
//! detach 时停止广播，launch 时按需拉起游戏进程。同样仅依赖标准库，内联于此。

use std::net::{SocketAddr, UdpSocket};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};

use crate::plugin::capability::BuiltinCapability;
use crate::plugin::manifest::{PluginKind, PluginManifest, RuntimeKind};
use crate::plugin::permission::PermissionSet;
use crate::plugin::protocol::{coupler_method, error_code, ErrorInfo};

/// 插件 ID。
pub const PLUGIN_ID: &str = "dev.mclink.builtin.minecraft-java-coupler";

const MULTICAST_ADDR: &str = "224.0.2.60:4445";

/// 允许 `coupler.launch` 拉起的可执行文件名（Minecraft 启动器用的 java 运行时）。
///
/// # 为什么必须白名单
///
/// `coupler.launch` 的 `path` 由调用方给定。此前直接 `Command::new(path)` 执行任意
/// 路径，等于把"以核心进程权限运行任意程序"开成一个能力方法——当前虽无命令暴露它
/// （`connect.rs` 只用 attach/detach），但接口已经就位，任何后续接线都会立刻变成
/// 本地代码执行。这里收紧到按文件名白名单，且**只接受纯文件名**（拒绝路径分隔符），
/// 由系统按 PATH / 注册表解析，与用户双击启动等价。
const ALLOWED_GAME_EXECUTABLES: &[&str] = &["javaw.exe", "java.exe", "javaw", "java"];

/// 从 `path` 参数取出允许拉起的可执行文件名。
fn allowed_executable(path: &str) -> Result<String, ErrorInfo> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(ErrorInfo::not_supported("游戏可执行文件路径为空"));
    }
    // 只接受纯文件名：带分隔符说明调用方想指定任意位置，一律拒绝
    if std::path::Path::new(trimmed).components().count() != 1 {
        return Err(ErrorInfo::not_supported(format!(
            "只接受可执行文件名，不接受路径: {}",
            trimmed
        )));
    }
    let name = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ErrorInfo::not_supported("非法的可执行文件路径"))?;
    if !ALLOWED_GAME_EXECUTABLES
        .iter()
        .any(|a| a.eq_ignore_ascii_case(name))
    {
        return Err(ErrorInfo::new(
            error_code::FORBIDDEN,
            format!(
                "不允许拉起 {}：只允许 Minecraft 启动器使用的 java 可执行文件",
                name
            ),
        ));
    }
    Ok(name.to_string())
}

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
    let message = format!("[MOTD]{}[/MOTD][AD]{}[/AD]", motd, port);
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
        "description": "将 Minecraft Java Edition 接入 MC Link 调度：在局域网广播游戏入口，并按需拉起游戏进程。",
        "games": ["minecraft-java"],
        "priority": 50,
        "permissions": ["proc_spawn_game", "net_connect_local", "net_udp"],
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
            .unwrap_or("MC Link")
            .to_string();
        // 越界端口不静默截断，回退到默认值
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

    fn launch(&self, params: &Value) -> Result<Value, ErrorInfo> {
        let raw = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorInfo::not_supported("未提供游戏可执行文件路径（参数 path）"))?;
        // 白名单 + 只接受纯文件名：绝不执行调用方指定的任意路径
        let exe = allowed_executable(raw)?;

        let mut cmd = Command::new(&exe);
        if let Some(args) = params.get("args").and_then(|v| v.as_array()) {
            for a in args {
                if let Some(s) = a.as_str() {
                    cmd.arg(s);
                }
            }
        }
        cmd.spawn()
            .map(|_| json!({ "launched": true, "exe": exe }))
            .map_err(|e| ErrorInfo::new(error_code::INTERNAL, format!("拉起游戏失败: {}", e)))
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
            coupler_method::LAUNCH => self.launch(&params),
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
