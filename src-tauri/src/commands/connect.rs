//! P2P 连接命令
//!
//! 封装 `wgp_core::connect::connect_peer` 为 Tauri 命令，在独立 tokio 任务中执行，
//! 通过 `p2p-event` 事件向前端推送连接阶段、日志与结果。

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tracing::{info, warn};

use wgp_core::connect::connect_peer;
use wgp_core::defaults;
use wgp_core::protocol::AppType;

use crate::mgr::connection::{ConnectionManager, ConnectionStatus};

/// 事件名
pub const P2P_EVENT: &str = "p2p-event";

/// 前端传入的启动参数
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartP2PArgs {
    /// "create" | "join"
    pub mode: String,
    /// 邀请码 / 房间码
    pub code: String,
    /// 信令服务器地址（host[:port]），留空使用默认
    pub signaling_addr: Option<String>,
    /// 打洞 STUN 地址（host[:port]），留空使用默认
    pub stun_addr: Option<String>,
    /// NAT 检测 STUN 地址列表（逗号分隔或数组），留空使用默认
    pub nat_stun_servers: Option<Vec<String>>,
    /// 应用类型字符串，默认 "GameTcp"
    pub app_type: Option<String>,
    /// 本地监听端口（本期用于桥接预留，可不传）
    #[allow(dead_code)]
    pub listen_port: Option<u16>,
}

/// 推送给前端的事件 payload
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PEvent {
    /// "progress" | "connected" | "error" | "stopped"
    pub stage: String,
    /// 人类可读消息
    pub message: String,
    pub nat_type: Option<String>,
    pub peer_addr: Option<String>,
    pub success_layer: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub error: Option<String>,
    /// 当前连接状态快照
    pub status: ConnectionStatus,
}

impl P2PEvent {
    fn progress(app: &AppHandle, mgr: &ConnectionManager, message: impl Into<String>) {
        let evt = P2PEvent {
            stage: "progress".into(),
            message: message.into(),
            nat_type: None,
            peer_addr: None,
            success_layer: None,
            elapsed_ms: None,
            error: None,
            status: mgr.status_snapshot(),
        };
        let _ = app.emit(P2P_EVENT, evt);
    }

    fn connected(
        app: &AppHandle,
        mgr: &ConnectionManager,
        message: impl Into<String>,
        peer_addr: String,
        local_nat: String,
        peer_nat: String,
        success_layer: String,
        elapsed_ms: u64,
    ) {
        mgr.status_snapshot(); // 触发快照（已由 set_connected 更新）
        let evt = P2PEvent {
            stage: "connected".into(),
            message: message.into(),
            nat_type: Some(format!("本地={} 对端={}", local_nat, peer_nat)),
            peer_addr: Some(peer_addr),
            success_layer: Some(success_layer),
            elapsed_ms: Some(elapsed_ms),
            error: None,
            status: mgr.status_snapshot(),
        };
        let _ = app.emit(P2P_EVENT, evt);
    }

    fn error(app: &AppHandle, mgr: &ConnectionManager, message: impl Into<String>) {
        let msg: String = message.into();
        let evt = P2PEvent {
            stage: "error".into(),
            message: msg.clone(),
            nat_type: None,
            peer_addr: None,
            success_layer: None,
            elapsed_ms: None,
            error: Some(msg),
            status: mgr.status_snapshot(),
        };
        let _ = app.emit(P2P_EVENT, evt);
    }

    fn stopped(app: &AppHandle, mgr: &ConnectionManager, message: impl Into<String>) {
        let evt = P2PEvent {
            stage: "stopped".into(),
            message: message.into(),
            nat_type: None,
            peer_addr: None,
            success_layer: None,
            elapsed_ms: None,
            error: None,
            status: mgr.status_snapshot(),
        };
        let _ = app.emit(P2P_EVENT, evt);
    }
}

/// 解析 `host[:port]` 为 SocketAddr，缺省端口使用 default_port
fn parse_addr(input: &str, default_port: u16) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    let input = input.trim();
    if input.is_empty() {
        return Err("地址为空".into());
    }
    if let Ok(mut addrs) = input.to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            return Ok(addr);
        }
    }
    if input.contains(':') {
        return Err(format!("地址「{}」无法解析", input));
    }
    let with_port = format!("{}:{}", input, default_port);
    let mut addrs = with_port
        .to_socket_addrs()
        .map_err(|e| format!("地址「{}」解析失败: {}", input, e))?;
    addrs
        .next()
        .ok_or_else(|| format!("地址「{}」解析结果为空", input))
}

/// 解析逗号分隔的 STUN 服务器列表
fn parse_stun_list(list: &[String]) -> Result<Vec<SocketAddr>, String> {
    let mut out = Vec::new();
    for s in list {
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        out.push(parse_addr(s, defaults::STUN_PORT)?);
    }
    Ok(out)
}

/// 解析 AppType 字符串，默认 GameTcp
fn parse_app_type(s: &Option<String>) -> Result<AppType, String> {
    match s {
        Some(s) => AppType::from_str(s).map_err(|e| format!("应用类型无效: {}", e)),
        None => Ok(AppType::GameTcp),
    }
}

/// 启动 P2P 连接（后台任务，立即返回）
#[tauri::command]
pub(crate) async fn start_p2p_connection(
    app: AppHandle,
    mgr: State<'_, Arc<ConnectionManager>>,
    args: StartP2PArgs,
) -> Result<String, String> {
    let mode = if args.mode.eq_ignore_ascii_case("join") {
        "join"
    } else {
        "create"
    };
    let is_creator = mode == "create";

    // 参数解析与默认值
    let signaling_input = args
        .signaling_addr
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("mk.aini2.cn:51080");
    let stun_input = args
        .stun_addr
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("mk.aini2.cn:51090");
    let nat_stun_inputs: Vec<String> = args
        .nat_stun_servers
        .clone()
        .unwrap_or_else(|| {
            defaults::NAT_DETECTION_STUN_SERVERS
                .iter()
                .map(|s| s.to_string())
                .collect()
        });
    let app_type = parse_app_type(&args.app_type)?;

    let signaling_addr = parse_addr(signaling_input, defaults::SIGNALING_PORT)?;
    let stun_addr = parse_addr(stun_input, defaults::STUN_PORT)?;
    let nat_stun_servers = parse_stun_list(&nat_stun_inputs)?;

    // 占用连接槽位
    let (stop_flag, start) = mgr.begin(mode, &args.code)?;
    let mgr = mgr.inner().clone();

    P2PEvent::progress(&app, &mgr, format!("开始 P2P 连接（角色: {}）", if is_creator { "创建者" } else { "加入者" }));
    P2PEvent::progress(&app, &mgr, format!("邀请码: {}", args.code));
    P2PEvent::progress(&app, &mgr, format!("信令: {} -> {}", signaling_input, signaling_addr));
    P2PEvent::progress(&app, &mgr, format!("打洞 STUN: {}", stun_addr));
    if !nat_stun_servers.is_empty() {
        P2PEvent::progress(
            &app,
            &mgr,
            format!(
                "NAT 检测 STUN: {}",
                nat_stun_servers
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    P2PEvent::progress(&app, &mgr, format!("应用类型: {}", app_type.name()));

    let app_clone = app.clone();
    let code = args.code.clone();

    tauri::async_runtime::spawn(async move {
        // 前置检查：是否在启动前就被停止
        if stop_flag.load(std::sync::atomic::Ordering::Acquire) {
            P2PEvent::stopped(&app_clone, &mgr, "连接已取消");
            mgr.task_ended(false);
            return;
        }

        P2PEvent::progress(&app_clone, &mgr, "步骤1: 连接信令服务器并握手…");
        let result = connect_peer(
            signaling_addr,
            &code,
            stun_addr,
            &nat_stun_servers,
            is_creator,
            app_type,
        )
        .await;

        if stop_flag.load(std::sync::atomic::Ordering::Acquire) {
            P2PEvent::stopped(&app_clone, &mgr, "连接已停止");
            mgr.task_ended(false);
            return;
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(conn) => {
                let peer_addr = conn.peer_addr.to_string();
                let local_nat = conn.local_nat.name().to_string();
                let peer_nat = conn.peer_nat.name().to_string();
                let success_layer = conn.success_layer.name().to_string();
                P2PEvent::progress(&app_clone, &mgr, format!("✅ P2P 通道建立成功（耗时 {}ms）", elapsed_ms));
                mgr.set_connected(conn, elapsed_ms);
                P2PEvent::connected(
                    &app_clone,
                    &mgr,
                    "P2P 通道已就绪",
                    peer_addr,
                    local_nat,
                    peer_nat,
                    success_layer,
                    elapsed_ms,
                );
                // 本期不实现端口转发桥接，保持连接态等待 stop
                P2PEvent::progress(&app_clone, &mgr, "通道已就绪，等待停止…");
                mgr.task_ended(true);
            }
            Err(e) => {
                let msg = format!("P2P 连接失败: {}", e);
                warn!("[p2p] {}", msg);
                mgr.set_failed(&msg, Some(elapsed_ms));
                P2PEvent::error(&app_clone, &mgr, msg);
                mgr.task_ended(false);
            }
        }
    });

    info!("[p2p] 连接任务已派发 (mode={})", mode);
    Ok(format!("p2p-{}", mode))
}

/// 停止当前 P2P 连接
#[tauri::command]
pub(crate) fn stop_p2p_connection(
    app: AppHandle,
    mgr: State<'_, Arc<ConnectionManager>>,
) -> Result<(), String> {
    mgr.request_stop();
    P2PEvent::stopped(&app, mgr.inner(), "已请求停止连接");
    Ok(())
}

/// 查询当前连接状态
#[tauri::command]
pub(crate) fn get_p2p_status(mgr: State<'_, Arc<ConnectionManager>>) -> ConnectionStatus {
    mgr.status_snapshot()
}
