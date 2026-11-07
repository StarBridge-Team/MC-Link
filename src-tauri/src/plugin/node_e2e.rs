//! Node 轨道的端到端用例：真的用 Bun 拉起 `sdk/node/examples/echo-adapter`，
//! 走完"握手 → 分发 → 响应"整条链路。
//!
//! # 为什么要真的起进程
//!
//! SDK 的价值在"能连上"，而握手涉及 HMAC 转录串、HKDF 的 salt/info 拼法、
//! AES-GCM 的 AAD 与 tag 布局——任何一处与核心不同都只会表现为"连接失败"，
//! 单测层面很难发现。这里用真实 Bun + 真实网关跑一遍，把这几个坑一次性钉住。
//!
//! # 没有 Bun 时自动跳过
//!
//! 本机没装 Bun 的开发机不应该因此挂 CI；用例会打印 `[skip]` 后直接返回。
//! 想强制跑：`MC_LINK_BUN=<path> cargo test node_e2e -- --nocapture`。

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;

use crate::plugin::gateway::{AuthEntry, AuthTable, Gateway};
use crate::plugin::launcher;
use crate::plugin::manifest::LimitsSpec;
use crate::plugin::peer::PeerHandle;
use crate::plugin::permission::{Permission, PermissionSet};
use crate::plugin::session::Inbound;

const PLUGIN_ID: &str = "dev.mclink.example.echo";

/// 子进程守卫：用例提前返回或 panic 时也要收掉进程，避免残留。
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mclink-node-e2e-{}",
        crate::plugin::crypto::random_hex(6)
    ))
}

/// 找到可用的 Bun。
///
/// 顺序：`MC_LINK_BUN` → PATH（真实可执行文件）→ 官方安装器位置
/// （`~/.bun/bin/bun`）→ npm 全局安装位置（`%APPDATA%\npm\node_modules\bun\bin`）。
///
/// 后两个是**测试专用**的便利：Windows 上 `npm i -g bun` 生成的是 `bun.cmd`/`bun.ps1`
/// shim，`Command::new("bun")` 找不到它（CreateProcess 不认 `.cmd`），所以这里直接探
/// 真实 `bun.exe` 的位置。生产代码不做这种探测（见 `plugin::runtime`）。
fn find_bun() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("MC_LINK_BUN") {
        let path = PathBuf::from(raw.trim());
        if path.is_file() {
            return Some(path);
        }
    }
    let ok = Command::new("bun")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        return Some(PathBuf::from("bun"));
    }

    let name = crate::plugin::runtime::bun_file_name();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        candidates.push(PathBuf::from(&home).join(".bun").join("bin").join(name));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(
            PathBuf::from(appdata)
                .join("npm")
                .join("node_modules")
                .join("bun")
                .join("bin")
                .join(name),
        );
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[tokio::test]
async fn node_plugin_completes_handshake_and_serves_calls() {
    let Some(bun) = find_bun() else {
        eprintln!("[skip] 未找到 bun，跳过 Node 插件端到端用例");
        return;
    };

    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sdk")
        .join("node")
        .join("examples")
        .join("echo-adapter");
    assert!(
        example.join("main.js").is_file(),
        "示例插件不存在: {}",
        example.display()
    );

    let dir = temp_dir();
    let runtime_dir = dir.join("Plugins").join("runtime");
    std::fs::create_dir_all(&runtime_dir).unwrap();

    let gateway = Arc::new(Gateway::bind(0).await.unwrap());
    let psk = [9u8; 32];
    let plugin_data = crate::plugin::plugin_data::ensure(&dir, PLUGIN_ID).unwrap();
    let session = launcher::write_session_file(
        &runtime_dir,
        PLUGIN_ID,
        &gateway.endpoint(),
        gateway.token(),
        &hex::encode(psk),
        &plugin_data,
        5000,
    )
    .unwrap();

    // 与正式启动完全同形的命令行：`bun <entry> --mclink-session <file>`
    let child = Command::new(&bun)
        .arg(example.join("main.js"))
        .arg("--mclink-session")
        .arg(&session)
        .current_dir(&example)
        .stdout(Stdio::null())
        .spawn()
        .expect("启动 bun 失败");
    let _guard = ChildGuard(child);

    let mut table = HashMap::new();
    table.insert(
        PLUGIN_ID.to_string(),
        AuthEntry {
            plugin_id: PLUGIN_ID.to_string(),
            psk,
            permissions: PermissionSet::from_declared(&[
                Permission::NetListenLocal,
                Permission::NetConnectAny,
                Permission::PeerMessage,
            ]),
            limits: LimitsSpec::default(),
            max_auth_failures: 5,
        },
    );
    let auth: AuthTable = Arc::new(RwLock::new(table));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let shutdown = Arc::new(AtomicBool::new(false));

    // 本用例不测 peer 中转（那部分在中枢的单测里），给一个无人消费的句柄即可
    let (peer, _peer_rx) = PeerHandle::channel();
    tokio::spawn(crate::plugin::gateway::serve(
        gateway.clone(),
        auth,
        peer,
        tx,
        shutdown.clone(),
    ));

    let connected = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match rx.recv().await {
                Some(Inbound::Connected(handle)) => break handle,
                Some(Inbound::Disconnected { reason, .. }) => {
                    panic!("插件在握手阶段断开: {reason}")
                }
                Some(_) => continue,
                None => panic!("事件通道已关闭"),
            }
        }
    })
    .await
    .expect("等待 Node 插件握手超时（检查 bun 是否可执行、示例路径是否正确）");

    let init = connected.call("adapter.init", json!({})).await.unwrap();
    assert_eq!(init["ready"], true, "adapter.init 应回 ready=true: {init}");
    assert!(
        init["join_fields"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "adapter.init 应返回表单字段: {init}"
    );

    let host = connected
        .call(
            "adapter.host.start",
            json!({ "game_port": 25565, "player_name": "Steve" }),
        )
        .await
        .unwrap();
    assert_eq!(host["port"], 25565);
    assert!(
        host["room"].as_str().unwrap_or("").starts_with("ECHO-"),
        "应返回房间码: {host}"
    );

    let status = connected.call("adapter.status", json!({})).await.unwrap();
    assert_eq!(status["phase"], "host-ok", "phase 必须以 -ok 结尾: {status}");

    // 插件间通信的接收侧：目标插件收到的固定是 `peer.call`，方法名在载荷里
    let peer_reply = connected
        .call(
            "peer.call",
            json!({
                "from": "dev.example.caller",
                "method": "echo.room",
                "params": null,
                "depth": 1,
                "trace": "t1"
            }),
        )
        .await
        .unwrap();
    assert!(
        peer_reply["room"].as_str().unwrap_or("").starts_with("ECHO-"),
        "peer 方法应能读到房间状态: {peer_reply}"
    );

    // 未注册的 peer 方法必须是 not_supported，而不是静默成功
    let missing = connected
        .call(
            "peer.call",
            json!({ "from": "dev.example.caller", "method": "nope", "params": null }),
        )
        .await
        .unwrap_err();
    assert_eq!(missing.code, crate::plugin::protocol::error_code::NOT_SUPPORTED);

    shutdown.store(true, Ordering::SeqCst);
    let _ = std::fs::remove_dir_all(&dir);
}
