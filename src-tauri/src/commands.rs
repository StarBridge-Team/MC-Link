use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;
use tauri::Emitter;
use tauri::Manager;
use serde::{Serialize, Deserialize};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use crate::central;
use crate::client::ClientMode;
use crate::host::HostMode;
use crate::lan;
use crate::protocol;
use crate::state::AppState;
use crate::state::DataDir;
use crate::adapter::AdapterManager;
use crate::terracotta_client::TerracottaClient;
use uapi_sdk_rust::services::GetNetworkIpinfoParams;
use uapi_sdk_rust::Client as UapiClient;

#[derive(Serialize, Clone)]
pub(crate) struct LanServerInfo {
    motd: String,
    port: u16,
}

#[tauri::command]
pub(crate) fn get_relays() -> Result<Vec<central::RelayInfo>, String> {
    central::get_relays().ok_or_else(|| "获取中继列表失败".to_string())
}

#[tauri::command]
pub(crate) fn check_room_exists(room_name: String) -> Result<bool, String> {
    central::get_room(&room_name).ok_or_else(|| "网络错误，无法检查房间".to_string())
}

#[tauri::command]
pub(crate) fn scan_lan_servers() -> Result<Vec<LanServerInfo>, String> {
    let servers = lan::scan_lan_servers()?;
    Ok(servers.into_iter().map(|s| LanServerInfo { motd: s.motd, port: s.port }).collect())
}

#[tauri::command]
pub(crate) fn get_latency(state: tauri::State<'_, AppState>) -> u64 {
    state.latency_ms.lock().map(|g| *g).unwrap_or(0)
}

pub(crate) fn latency_monitor(relay_addr: std::net::SocketAddr, state: Arc<AppState>, window: tauri::Window) {
    thread::Builder::new()
        .name("latency-monitor".into())
        .spawn(move || {
            while state.is_running.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(3));
                if !state.is_running.load(Ordering::Relaxed) {
                    break;
                }
                let ping_ok = std::net::TcpStream::connect_timeout(&relay_addr, Duration::from_secs(3))
                    .and_then(|mut stream| {
                        let start = Instant::now();
                        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                        protocol::write_packet(&mut stream, &[0x32])?;
                        protocol::read_packet(&mut stream)?;
                        let ms = start.elapsed().as_millis() as u64;
                        if let Ok(mut g) = state.latency_ms.lock() {
                            *g = ms;
                        }
                        let _ = window.emit("latency-update", ms);
                        Ok::<_, std::io::Error>(())
                    })
                    .is_ok();
                if !ping_ok {
                    if let Ok(mut g) = state.latency_ms.lock() {
                        *g = 999;
                    }
                    let _ = window.emit("latency-update", 999u64);
                }
            }
            if let Ok(mut g) = state.latency_ms.lock() {
                *g = 0;
            }
            let _ = window.emit("latency-update", 0u64);
        })
        .ok();
}

/// 中继服务器选择结果
struct RelaySelection {
    addr: std::net::SocketAddr,
    id: String,
}

/// 选择并解析中继服务器（延迟优先）
fn select_relay(selected_relay: &str, window: &tauri::Window) -> Result<RelaySelection, String> {
    if selected_relay == "__auto__" || selected_relay.is_empty() {
        window.emit("app-log", "[启动] 自动选择中继服务器（延迟优先）...".to_string()).ok();
        let relays = central::get_relays().ok_or("网络错误，请检查网络连接")?;
        if relays.is_empty() {
            return Err("没有可用的中继服务器".to_string());
        }

        // 对每个中继测延迟，选择最低延迟的
        let mut measured: Vec<(usize, u64)> = Vec::new();
        for (i, relay) in relays.iter().enumerate() {
            if let Some(addr) = protocol::resolve_address(&relay.address) {
                let start = Instant::now();
                if std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok() {
                    let ms = start.elapsed().as_millis() as u64;
                    measured.push((i, ms));
                }
            }
        }

        let selected_idx = measured.iter()
            .min_by_key(|(_, ms)| *ms)
            .map(|(i, _)| *i)
            .unwrap_or(0);

        let relay = &relays[selected_idx];
        window.emit("app-log", format!("[启动] 选中中继: {} ({}) 延迟: {}ms",
            relay.name, relay.address,
            measured.iter().find(|(i, _)| *i == selected_idx).map(|(_, ms)| *ms).unwrap_or(0))).ok();
        Ok(RelaySelection {
            addr: protocol::resolve_address(&relay.address).ok_or("网络连接失败")?,
            id: relay.id.clone(),
        })
    } else if selected_relay.contains(':') {
        window.emit("app-log", format!("[启动] 自定义中继: {}", selected_relay)).ok();
        Ok(RelaySelection {
            addr: protocol::resolve_address(selected_relay).ok_or("中继地址解析失败")?,
            id: "custom".to_string(),
        })
    } else {
        let relays = central::get_relays().ok_or("网络错误，请检查网络连接")?;
        let relay = relays.iter().find(|r| r.id == selected_relay).ok_or("未找到选中的中继服务器")?;
        window.emit("app-log", format!("[启动] 选中中继: {} ({})", relay.name, relay.address)).ok();
        Ok(RelaySelection {
            addr: protocol::resolve_address(&relay.address).ok_or("网络连接失败")?,
            id: relay.id.clone(),
        })
    }
}

/// 创建日志回调闭包
fn create_log_callback(window: tauri::Window) -> impl Fn(String) {
    move |msg| {
        let _ = window.emit("app-log", msg);
    }
}

/// 启动公共服务（延迟监控、房间状态更新）
fn start_common_services(
    relay_addr: std::net::SocketAddr,
    app_state: Arc<AppState>,
    window: tauri::Window,
    current_room: Arc<Mutex<Option<(String, String)>>>,
    room_name: String,
    password: String,
) {
    latency_monitor(relay_addr, app_state, window);
    if let Ok(mut room) = current_room.lock() {
        *room = Some((room_name, password));
    }
}

/// 启动房主工作线程
fn spawn_host_worker(
    mut host_mode: HostMode,
    mc_port: u16,
    mc_motd: String,
    stop_signal: Arc<AtomicBool>,
    window: tauri::Window,
) {
    thread::Builder::new()
        .name("host-mode-worker".into())
        .spawn(move || {
            let result = host_mode.start(mc_port, mc_motd, stop_signal);
            match &result {
                Ok(msg) => { let _ = window.emit("app-log", msg); }
                Err(e) => { let _ = window.emit("app-log", format!("[结束] {}", e)); }
            }
        })
        .ok();
}

/// 启动客户端工作线程
fn spawn_client_worker(
    room_name: String,
    password: String,
    relay_addr: std::net::SocketAddr,
    local_port: u16,
    stop_signal: Arc<AtomicBool>,
    window: tauri::Window,
) {
    thread::Builder::new()
        .name("client-mode-worker".into())
        .spawn(move || {
            let mut client_mode = ClientMode::new(local_port, "MC Link".to_string());
            client_mode.set_relay(relay_addr, room_name.clone(), password.clone());
            client_mode.set_log_callback(create_log_callback(window.clone()));
            let result = client_mode.start(stop_signal);
            match &result {
                Ok(msg) => { let _ = window.emit("app-log", msg); }
                Err(e) => { let _ = window.emit("app-log", format!("[结束] {}", e)); }
            }
        })
        .ok();
}

/// 启动房主模式：扫描局域网、注册中继、创建房间、启动工作线程
fn start_host_mode(
    room_name: String,
    password: String,
    player_name: String,
    relay: RelaySelection,
    window: tauri::Window,
    stop_signal: Arc<AtomicBool>,
    app_state: Arc<AppState>,
    current_room: Arc<Mutex<Option<(String, String)>>>,
) -> Result<String, String> {
    window.emit("app-log", "房主模式: 扫描局域网Minecraft服务器...".to_string()).ok();
    let servers = lan::scan_lan_servers()?;
    let mc_port = servers.first().map(|s| s.port).unwrap_or(0);
    let mc_motd = servers.first().map(|s| s.motd.clone()).unwrap_or_default();

    if servers.is_empty() || mc_port == 0 {
        return Err("未找到Minecraft局域网服务器，请先在Minecraft中开启局域网联机".to_string());
    }

    window.emit("app-log", format!("[启动] 发现Minecraft: 端口={}", mc_port)).ok();

    let mut host_mode = HostMode::new();
    host_mode.set_relay(relay.addr, room_name.clone(), password.clone());
    host_mode.set_log_callback(create_log_callback(window.clone()));
    host_mode.connect_and_register().map_err(|e| format!("注册到中继服务器失败: {}", e))?;
    window.emit("app-log", "[启动] 已注册到中继服务器".to_string()).ok();

    central::create_room(&room_name, &password, &relay.id).ok_or("创建房间失败，可能房间名已存在")?;
    window.emit("app-log", format!("[启动] 房间已创建: {}", room_name)).ok();

    central::join_room(&room_name, &player_name, "host", &password, Some(relay.id.as_str()));

    spawn_host_worker(host_mode, mc_port, mc_motd, stop_signal.clone(), window.clone());
    start_common_services(relay.addr, app_state, window, current_room, room_name, password);

    Ok(format!("房主模式已启动，Minecraft端口: {}", mc_port))
}

/// 启动成员模式：加入房间、启动本地代理、连接中继
fn start_client_mode(
    room_name: String,
    password: String,
    player_name: String,
    relay: RelaySelection,
    window: tauri::Window,
    stop_signal: Arc<AtomicBool>,
    app_state: Arc<AppState>,
    current_room: Arc<Mutex<Option<(String, String)>>>,
) -> Result<String, String> {
    window.emit("app-log", "成员模式: 连接到中继服务器...".to_string()).ok();
    let local_port = 25565u16;

    central::join_room(&room_name, &player_name, "member", &password, Some(relay.id.as_str()));

    spawn_client_worker(room_name.clone(), password.clone(), relay.addr, local_port, stop_signal.clone(), window.clone());
    start_common_services(relay.addr, app_state, window, current_room, room_name, password);

    Ok(format!("成员模式已启动\n请在Minecraft中连接 127.0.0.1:{}", local_port))
}

#[tauri::command]
pub(crate) async fn start_online(
    state: tauri::State<'_, AppState>,
    room_name: String,
    password: String,
    window: tauri::Window,
    selected_relay: String,
    adapters: Vec<String>,
    is_host: bool,
    player_name: String,
) -> Result<String, String> {
    if state.is_running.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        return Err("联机功能已在运行中".to_string());
    }

    let result = (|| -> Result<String, String> {
        window.emit("app-log", format!("[启动] 适配器: {:?}", adapters)).ok();

        let relay = select_relay(&selected_relay, &window)?;

        let stop_signal = Arc::new(AtomicBool::new(false));
        if let Ok(mut sig) = state.stop_signal.lock() {
            *sig = Some(stop_signal.clone());
        }

        let app_state = Arc::new(AppState {
            current_room: state.current_room.clone(),
            is_running: state.is_running.clone(),
            latency_ms: state.latency_ms.clone(),
            stop_signal: state.stop_signal.clone(),
        });

        if is_host {
            start_host_mode(
                room_name, password, player_name, relay, window,
                stop_signal, app_state, state.current_room.clone(),
            )
        } else {
            start_client_mode(
                room_name, password, player_name, relay, window,
                stop_signal, app_state, state.current_room.clone(),
            )
        }
    })();

    if result.is_err() {
        state.is_running.store(false, Ordering::Relaxed);
    }
    result
}

fn stop_online_inner(state: &AppState) {
    if let Ok(mut sig) = state.stop_signal.lock() {
        if let Some(s) = sig.take() {
            s.store(true, Ordering::Relaxed);
        }
    }
    thread::sleep(Duration::from_millis(500));
    state.is_running.store(false, Ordering::Relaxed);
    let room = state.current_room.lock().ok().and_then(|g| g.clone());
    if let Some((ref room_name, _)) = room {
        central::delete_room(room_name);
    }
    if let Ok(mut room) = state.current_room.lock() {
        *room = None;
    }
    if let Ok(mut lat) = state.latency_ms.lock() {
        *lat = 0;
    }
}

#[tauri::command]
pub(crate) fn stop_online(state: tauri::State<'_, AppState>, adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    stop_online_inner(&state);
    if let Ok(manager) = adapter_manager.lock() {
        manager.stop_terracotta().ok();
    }
    Ok("联机已停止".to_string())
}

#[tauri::command]
pub(crate) fn minimize_window(window: tauri::Window) {
    window.minimize().ok();
}

#[tauri::command]
pub(crate) fn maximize_window(window: tauri::Window) {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().ok();
    } else {
        window.maximize().ok();
    }
}

#[tauri::command]
pub(crate) fn close_window(_window: tauri::Window, state: tauri::State<'_, AppState>, adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>, app: tauri::AppHandle) -> Result<String, String> {
    stop_online_inner(&state);
    if let Ok(manager) = adapter_manager.lock() {
        manager.shutdown_all();
    }
    app.exit(0);
    Ok("已退出".to_string())
}

#[tauri::command]
pub(crate) fn drag_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn exit_app(state: tauri::State<'_, AppState>, adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>, app: tauri::AppHandle) -> Result<String, String> {
    stop_online_inner(&state);
    if let Ok(manager) = adapter_manager.lock() {
        manager.shutdown_all();
    }
    app.exit(0);
    Ok("已退出".to_string())
}

#[tauri::command]
pub(crate) fn show_window(window: tauri::Window) {
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub(crate) fn show_main_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub(crate) fn set_tray_size(window: tauri::Window, width: f64, height: f64) {
    let _ = window.emit("tray-resize", serde_json::json!({"width": width, "height": height}));
}

#[tauri::command]
pub(crate) fn resize_window(window: tauri::Window, width: f64, height: f64, min_width: Option<f64>, min_height: Option<f64>, center: bool) {
    if let (Some(mw), Some(mh)) = (min_width, min_height) {
        let _ = window.set_min_size(Some(tauri::LogicalSize::new(mw, mh)));
    }
    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    if center {
        let _ = window.center();
    }
}

#[tauri::command]
pub(crate) async fn ping_relay(address: String) -> Result<u64, String> {
    let addr = protocol::resolve_address(&address).ok_or("地址解析失败")?;
    let start = Instant::now();
    let mut stream = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| format!("连接失败: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    protocol::write_packet(&mut stream, &[0x32]).map_err(|_| "发送失败".to_string())?;
    protocol::read_packet(&mut stream).map_err(|_| "无响应".to_string())?;
    Ok(start.elapsed().as_millis() as u64)
}

#[derive(Serialize)]
pub(crate) struct RoomCheckResult {
    exists: bool,
    room_type: String, // "mc_link", "revamp", or "none"
}

#[tauri::command]
pub(crate) async fn check_room_full(room_name: String) -> Result<RoomCheckResult, String> {
    // 并行检查 MC Link 房间和 revamp 房间
    let (mc_link_result, revamp_result) = tokio::join!(
        tokio::task::spawn_blocking({
            let rn = room_name.clone();
            move || central::get_room(&rn)
        }),
        crate::revamp::room_exists(&room_name)
    );

    if let Ok(Some(true)) = mc_link_result {
        return Ok(RoomCheckResult { exists: true, room_type: "mc_link".to_string() });
    }
    if let Ok(true) = revamp_result {
        return Ok(RoomCheckResult { exists: true, room_type: "revamp".to_string() });
    }
    Ok(RoomCheckResult { exists: false, room_type: "none".to_string() })
}

#[derive(Serialize)]
pub(crate) struct IpInfo {
    region: String,
    isp: String,
}

#[tauri::command]
pub(crate) async fn get_ip_info(host: String) -> Result<IpInfo, String> {
    let client = UapiClient::builder().build().map_err(|e| format!("创建客户端失败: {}", e))?;
    let params = GetNetworkIpinfoParams::new(&host);
    let resp = client.network().get_network_ipinfo(params).await.map_err(|e| {
        let msg = e.to_string();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&msg);
        if let Ok(val) = parsed {
            val.get("message").and_then(|m| m.as_str()).unwrap_or(&msg).to_string()
        } else {
            msg
        }
    })?;
    Ok(IpInfo {
        region: resp.region.unwrap_or_default(),
        isp: resp.isp.unwrap_or_default(),
    })
}

#[tauri::command]
pub(crate) async fn download_adapter(window: tauri::Window, adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    let adapter_dir = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        mgr.adapter_dir.join("Terracotta")
    };
    std::fs::create_dir_all(&adapter_dir)
        .map_err(|e| format!("创建目录失败: {}", e))?;

    let url = "https://gitee.com/burningtnt/Terracotta/releases/download/v0.4.2/terracotta-0.4.2-windows-x86_64-pkg.tar.gz";
    let filename = "terracotta-0.4.2-windows-x86_64-pkg.tar.gz";
    let archive_path = adapter_dir.join(filename);

    window.emit("app-log", "[下载] 开始下载陶瓦联机...").ok();
    window.emit("download-progress", 0u8).ok();

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;

    let response = client.get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载失败 (HTTP {})", response.status()));
    }

    // 创建临时文件用于流式写入
    let tmp_path = adapter_dir.join(format!("{}.tmp", filename));
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| format!("创建临时文件失败: {}", e))?;

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取数据失败: {}", e))?;
        downloaded += chunk.len() as u64;
        file.write_all(&chunk).await.map_err(|e| format!("写入文件失败: {}", e))?;
        if total > 0 {
            let pct = (downloaded as f64 / total as f64 * 100.0) as u8;
            let _ = window.emit("download-progress", pct);
        }
    }

    file.flush().await.map_err(|e| format!("刷新文件失败: {}", e))?;
    drop(file);

    // 重命名临时文件为正式文件
    std::fs::rename(&tmp_path, &archive_path)
        .map_err(|e| format!("重命名文件失败: {}", e))?;

    window.emit("app-log", format!("[下载] 下载完成 ({} bytes)，开始解压...", downloaded)).ok();

    let file = std::fs::File::open(&archive_path)
        .map_err(|e| format!("打开下载文件失败: {}", e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(&adapter_dir)
        .map_err(|e| format!("解压失败: {}", e))?;

    std::fs::remove_file(&archive_path)
        .map_err(|e| format!("删除安装包失败: {}", e))?;

    window.emit("app-log", "[下载] 陶瓦联机已安装完成，正在启动...".to_string()).ok();

    let manager = match adapter_manager.lock() {
        Ok(m) => m,
        Err(e) => return Err(format!("内部错误: {}", e)),
    };
    manager.launch_terracotta_after_download();

    Ok("陶瓦联机已安装并启动".to_string())
}

#[tauri::command]
pub(crate) fn adapter_startup_init(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    let manager = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
    manager.launch_all();
    let status = manager.get_status();
    if status.running {
        Ok("适配器已就绪".to_string())
    } else if status.installed {
        Ok("适配器已安装，正在启动...".to_string())
    } else {
        Ok("适配器正在安装...".to_string())
    }
}

#[tauri::command]
pub(crate) fn get_adapter_status(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<crate::adapter::AdapterStatus, String> {
    let manager = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
    Ok(manager.get_status())
}

#[tauri::command]
pub(crate) async fn get_terracotta_state(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        let p = *mgr.terracotta_port.lock().map_err(|e| format!("内部错误: {}", e))?;
        p
    };
    match port {
        Some(p) => {
            let client = TerracottaClient::new(p);
            let state = client.get_state().await?;
            serde_json::to_value(&state).map_err(|e| format!("序列化失败: {}", e))
        }
        None => {
            Ok(serde_json::json!({"state": "waiting", "index": 0}))
        }
    }
}

#[tauri::command]
pub(crate) async fn start_terracotta_host(
    adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>,
    window: tauri::Window,
    room_code: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        mgr.ensure_running()?
    };
    let client = TerracottaClient::new(port);

    window.emit("app-log", "[陶瓦] 重置状态...".to_string()).ok();
    client.set_ide().await?;

    window.emit("app-log", "[陶瓦] 开始扫描本地Minecraft服务器...".to_string()).ok();
    let room_opt = if room_code.is_empty() { None } else { Some(room_code.as_str()) };
    let player_opt = if player_name.is_empty() { None } else { Some(player_name.as_str()) };
    client.start_host(room_opt, player_opt, &[]).await?;

    window.emit("app-log", "[陶瓦] 等待房间创建...".to_string()).ok();
    let result = client.poll_state(60, 500).await?;

    let state_str = &result.state;
    if state_str == "host-ok" {
        let room_code = result.room.as_deref().unwrap_or("未知");
        window.emit("app-log", format!("[陶瓦] 房间已创建! 房间码: {}", room_code)).ok();
    } else if state_str == "exception" {
        let excode = result.exception_type.unwrap_or(99);
        window.emit("app-log", format!("[陶瓦] 异常退出 (代码: {})", excode)).ok();
    } else {
        window.emit("app-log", format!("[陶瓦] 未知状态: {}", state_str)).ok();
    }

    serde_json::to_value(&result).map_err(|e| format!("序列化失败: {}", e))
}

// ===== 合并命令 =====

#[derive(Serialize)]
pub(crate) struct PrepareAppData {
    personalization: PersonalizationSettings,
    default_effect: String,
    relays: Vec<crate::central::RelayInfo>,
    app_version: String,
    tauri_version: String,
    google_fonts_css: Option<String>,
    bootstrap_icons_css: Option<String>,
}

async fn download_google_fonts_css() -> Option<String> {
    let url = "https://fonts.googleapis.com/css2?family=Poppins:wght@300;400;500;600;700&display=swap";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build().ok()?;
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() { return None; }
    resp.text().await.ok()
}

async fn download_bootstrap_icons() -> Option<String> {
    let bi_url = "https://cdn.jsdelivr.net/npm/bootstrap-icons@1.13.1/font/bootstrap-icons.css";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build().ok()?;

    // 下载 CSS
    let css_resp = client.get(bi_url).send().await.ok()?;
    if !css_resp.status().is_success() { return None; }
    let css = css_resp.text().await.ok()?;

    // 提取 woff2 字体 URL
    let marker = r#"url(""#;
    let start = css.find(marker)?;
    let from = start + marker.len();
    let end = css[from..].find(r#"")"#)?;
    let font_url = &css[from..from + end];

    // 下载字体文件
    let base = "https://cdn.jsdelivr.net/npm/bootstrap-icons@1.13.1/font/";
    let full_url = if font_url.starts_with("http") {
        font_url.to_string()
    } else {
        format!("{}{}", base, font_url.trim_start_matches("./"))
    };
    let font_resp = client.get(&full_url).send().await.ok()?;
    if !font_resp.status().is_success() { return None; }
    let font_bytes = font_resp.bytes().await.ok()?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&font_bytes);
    let data_uri = format!("data:application/x-woff2;base64,{}", b64);

    // 替换 CSS 中的字体 URL 为 data URI
    let rewritten = css.replace(font_url, &data_uri);
    Some(rewritten)
}

#[tauri::command]
pub(crate) async fn prepare_app(
    data_dir: tauri::State<'_, DataDir>,
) -> Result<PrepareAppData, String> {
    let pers = load_personalization(&data_dir.0).unwrap_or_default();
    let default_effect = get_default_effect();
    let relays = crate::central::get_relays().unwrap_or_default();
    let app_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let tauri_version = get_tauri_version();

    // 后端并行下载字体/图标资源
    let (gfx_css, bi_css) = tokio::join!(
        download_google_fonts_css(),
        download_bootstrap_icons(),
    );

    Ok(PrepareAppData {
        personalization: pers,
        default_effect,
        relays,
        app_version,
        tauri_version,
        google_fonts_css: gfx_css,
        bootstrap_icons_css: bi_css,
    })
}

#[derive(Serialize)]
pub(crate) struct InitAppData {
    personalization: PersonalizationSettings,
    default_effect: String,
    relays: Vec<crate::central::RelayInfo>,
    app_version: String,
    tauri_version: String,
}

/// 仅加载配置（资源已缓存时使用，轻量快速）
#[tauri::command]
pub(crate) fn init_app(data_dir: tauri::State<'_, DataDir>) -> Result<InitAppData, String> {
    let pers = load_personalization(&data_dir.0).unwrap_or_default();
    let default_effect = get_default_effect();
    let relays = crate::central::get_relays().unwrap_or_default();
    let app_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let tauri_version = get_tauri_version();
    Ok(InitAppData { personalization: pers, default_effect, relays, app_version, tauri_version })
}

// ===== MC Link revamp 命令 =====

#[tauri::command]
pub(crate) async fn revamp_ping() -> Result<bool, String> {
    crate::revamp::ping_central().await
}

#[tauri::command]
pub(crate) async fn revamp_get_nodes() -> Result<Vec<crate::revamp::RevampNode>, String> {
    crate::revamp::get_nodes().await
}

#[tauri::command]
pub(crate) async fn revamp_get_rooms() -> Result<Vec<crate::revamp::RevampRoom>, String> {
    crate::revamp::get_rooms().await
}

#[tauri::command]
pub(crate) async fn revamp_room_exists(room_id: String) -> Result<bool, String> {
    crate::revamp::room_exists(&room_id).await
}

#[tauri::command]
pub(crate) async fn revamp_create_room(
    room_id: String,
    password: String,
    node_ip: String,
    node_port: String,
    creator_name: String,
) -> Result<serde_json::Value, String> {
    crate::revamp::create_room(&room_id, &password, &node_ip, &node_port, &creator_name).await
}

#[tauri::command]
pub(crate) async fn revamp_join_room(
    room_id: String,
    password: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    crate::revamp::join_room(&room_id, &password, &player_name).await
}

#[tauri::command]
pub(crate) async fn revamp_leave_room(
    room_id: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    crate::revamp::leave_room(&room_id, &player_name).await
}

#[tauri::command]
pub(crate) async fn revamp_get_version() -> Result<crate::revamp::RevampVersion, String> {
    crate::revamp::get_version().await
}

#[tauri::command]
pub(crate) async fn revamp_register(username: String, password: String) -> Result<crate::revamp::AccountResult, String> {
    crate::revamp::register_account(&username, &password).await
}

#[tauri::command]
pub(crate) async fn revamp_login(username: String, password: String) -> Result<crate::revamp::AccountResult, String> {
    crate::revamp::login_account(&username, &password).await
}

// ===== Revamp 中继命令 =====

#[tauri::command]
pub(crate) async fn revamp_start_host(
    state: tauri::State<'_, AppState>,
    local_port: u16,
    node_ip: String,
    node_port: String,
    room_id: String,
    password: String,
    player_name: String,
    window: tauri::Window,
) -> Result<String, String> {
    if state.is_running.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        return Err("联机功能已在运行中".to_string());
    }

    let stop_signal = Arc::new(AtomicBool::new(false));

    match crate::revamp::create_room(&room_id, &password, &node_ip, &node_port, &player_name).await {
        Ok(room) => {
            if room.get("status").and_then(|s| s.as_str()) != Some("ok") {
                state.is_running.store(false, Ordering::Relaxed);
                return Err(format!(
                    "创建房间失败: {}",
                    room.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误")
                ));
            }

            if let Ok(mut sig) = state.stop_signal.lock() {
                *sig = Some(stop_signal.clone());
            }

            let mut relay = crate::revamp_relay::RevampHostRelay::new();
            match relay.start(local_port, &node_ip, &node_port, stop_signal) {
                Ok(msg) => {
                    if let Ok(mut room) = state.current_room.lock() {
                        *room = Some((room_id.clone(), password.clone()));
                    }
                    window.emit("app-log", format!("[revamp] {}", msg)).ok();
                    Ok(msg)
                }
                Err(e) => {
                    // 中继启动失败，清理已创建的房间
                    let _ = crate::revamp::leave_room(&room_id, &player_name).await;
                    state.is_running.store(false, Ordering::Relaxed);
                    Err(e)
                }
            }
        }
        Err(e) => {
            state.is_running.store(false, Ordering::Relaxed);
            Err(format!("创建房间失败: {}", e))
        }
    }
}

#[tauri::command]
pub(crate) async fn revamp_join_room_cmd(
    room_id: String,
    password: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    crate::revamp::join_room(&room_id, &password, &player_name).await
}

#[tauri::command]
pub(crate) fn get_players(room_name: String) -> Result<Vec<central::PlayerInfo>, String> {
    central::get_players(&room_name).ok_or("获取玩家列表失败".to_string())
}

#[tauri::command]
pub(crate) fn get_app_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// 获取 Tauri 框架版本（从 Cargo.lock 编译时读取）
#[tauri::command]
pub(crate) fn get_tauri_version() -> String {
    let lock = include_str!("../Cargo.lock");
    let mut in_tauri = false;
    for line in lock.lines() {
        let t = line.trim();
        if t == "name = \"tauri\"" {
            in_tauri = true;
        } else if in_tauri && t.starts_with("version = ") {
            return t.trim_start_matches("version = ")
                .trim_matches('"')
                .to_string();
        } else if in_tauri && t.starts_with('[') {
            break;
        }
    }
    "2.x".to_string()
}

fn setting_dir(data_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dir = data_dir.join("Setting");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建设置目录失败: {}", e))?;
    Ok(dir)
}

#[tauri::command]
pub(crate) fn get_setting(data_dir: tauri::State<'_, DataDir>, section: String) -> Result<String, String> {
    let section_file = section.replace(" ", "_").replace("/", "_").replace("\\", "_");
    let path = setting_dir(&data_dir.0)?.join(format!("{}.yml", section_file));
    if !path.exists() {
        return Ok(format!("# {}\n", section));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取设置文件失败: {}", e))
}

#[tauri::command]
pub(crate) fn save_setting(data_dir: tauri::State<'_, DataDir>, section: String, content: String) -> Result<(), String> {
    let section_file = section.replace(" ", "_").replace("/", "_").replace("\\", "_");
    let path = setting_dir(&data_dir.0)?.join(format!("{}.yml", section_file));
    std::fs::write(&path, &content).map_err(|e| format!("保存设置文件失败: {}", e))
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct PersonalizationSettings {
    pub(crate) theme_color: String,
    pub(crate) theme_mode: String,
    pub(crate) animation_enabled: bool,
    pub(crate) animation_speed: f64,
    pub(crate) transparent_effect: String,
    pub(crate) background_type: String,
    pub(crate) background_value: String,
    pub(crate) background_fit: String,
    pub(crate) background_overlay: bool,
    pub(crate) background_overlay_opacity: f64,
    pub(crate) music_mode: String,
    pub(crate) music_value: String,
    pub(crate) homepage_mode: String,
    pub(crate) homepage_value: String,
}

impl Default for PersonalizationSettings {
    fn default() -> Self {
        Self {
            theme_color: "#0066cc".to_string(),
            theme_mode: "system".to_string(),
            animation_enabled: true,
            animation_speed: 1.0,
            transparent_effect: "none".to_string(),
            background_type: "default".to_string(),
            background_value: String::new(),
            background_fit: "scale-to-fill".to_string(),
            background_overlay: false,
            background_overlay_opacity: 30.0,
            music_mode: "none".to_string(),
            music_value: String::new(),
            homepage_mode: "default".to_string(),
            homepage_value: String::new(),
        }
    }
}

fn personalization_path(data_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dir = setting_dir(data_dir)?;
    Ok(dir.join("personalization.yml"))
}

fn load_personalization(data_dir: &std::path::Path) -> Result<PersonalizationSettings, String> {
    let path = personalization_path(data_dir)?;
    if !path.exists() {
        return Ok(PersonalizationSettings::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取个性化设置失败: {}", e))?;
    serde_yaml::from_str(&content)
        .map_err(|e| format!("解析个性化设置失败: {}", e))
}

#[tauri::command]
pub(crate) fn get_personalization(data_dir: tauri::State<'_, DataDir>) -> Result<PersonalizationSettings, String> {
    load_personalization(&data_dir.0)
}

#[tauri::command]
pub(crate) fn get_default_effect() -> String {
    #[cfg(target_os = "macos")]
    { "hud_window".to_string() }
    #[cfg(windows)]
    {
        // Windows 11+ = 10.0.22000, Windows 10 = 10.0.10240
        // 简单起见：Win10 以上用 mica，否则用 acrylic
        "mica".to_string()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    { "none".to_string() }
}

#[tauri::command]
pub(crate) fn save_personalization(data_dir: tauri::State<'_, DataDir>, settings: PersonalizationSettings) -> Result<(), String> {
    let content = serde_yaml::to_string(&settings)
        .map_err(|e| format!("序列化个性化设置失败: {}", e))?;
    let path = personalization_path(&data_dir.0)?;
    std::fs::write(&path, &content)
        .map_err(|e| format!("保存个性化设置失败: {}", e))
}

#[derive(Serialize)]
pub(crate) struct BackgroundFile {
    name: String,
    is_video: bool,
}

fn read_bg_files(bg_dir: &std::path::Path, files: &mut Vec<BackgroundFile>, seen: &mut std::collections::HashSet<String>) {
    if !bg_dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(bg_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    let is_video = matches!(ext.as_str(), "mp4" | "webm" | "avi" | "mov" | "mkv" | "flv");
                    let is_image = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg");
                    if is_image || is_video {
                        if let Some(name) = path.file_name() {
                            let name = name.to_string_lossy().to_string();
                            if seen.insert(name.clone()) {
                                files.push(BackgroundFile { name, is_video });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn old_background_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("Background"))
}

#[tauri::command]
pub(crate) fn get_background_files(data_dir: tauri::State<'_, DataDir>) -> Result<Vec<BackgroundFile>, String> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Primary: data_dir/Background
    let bg_dir = data_dir.0.join("Background");
    read_bg_files(&bg_dir, &mut files, &mut seen);

    // Fallback: old exe_dir/Background (backward compatibility)
    if let Some(old_bg) = old_background_dir() {
        if old_bg != bg_dir {
            read_bg_files(&old_bg, &mut files, &mut seen);
        }
    }

    Ok(files)
}

#[tauri::command]
pub(crate) fn get_background_file_url(data_dir: tauri::State<'_, DataDir>, filename: String) -> Result<String, String> {
    // Primary: data_dir/Background
    let bg_dir = data_dir.0.join("Background");
    let full_path = bg_dir.join(&filename);
    if full_path.exists() {
        return Ok(full_path.to_string_lossy().to_string());
    }

    // Fallback: old exe_dir/Background (backward compatibility)
    if let Some(old_bg) = old_background_dir() {
        let old_path = old_bg.join(&filename);
        if old_path.exists() {
            return Ok(old_path.to_string_lossy().to_string());
        }
    }

    Err(format!("文件不存在: {}", filename))
}

#[tauri::command]
pub(crate) fn set_window_effect(window: tauri::WebviewWindow, effect: String) -> Result<(), String> {
    match effect.as_str() {
        "transparent" => Ok(()),
        "mica" => {
            #[cfg(windows)]
            {
                crate::apply_mica_backdrop_typed(&window, 2);
            }
            Ok(())
        }
        "acrylic" => {
            #[cfg(windows)]
            {
                crate::apply_mica_backdrop_typed(&window, 3);
            }
            Ok(())
        }
        "hud_window" => {
            #[cfg(target_os = "macos")]
            {
                crate::apply_vibrancy_backdrop(&window);
            }
            Ok(())
        }
        _ => {
            // "none" - reset to default backdrop
            #[cfg(windows)]
            {
                crate::apply_mica_backdrop_typed(&window, 0);
            }
            Ok(())
        }
    }
}

#[tauri::command]
pub(crate) async fn start_terracotta_guest(
    adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>,
    window: tauri::Window,
    room_code: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        mgr.ensure_running()?
    };
    let client = TerracottaClient::new(port);

    window.emit("app-log", "[陶瓦] 重置状态...".to_string()).ok();
    client.set_ide().await?;

    window.emit("app-log", format!("[陶瓦] 加入房间 {}...", room_code)).ok();
    let player_opt = if player_name.is_empty() { None } else { Some(player_name.as_str()) };
    client.start_guest(&room_code, player_opt, &[]).await?;

    window.emit("app-log", "[陶瓦] 等待连接...".to_string()).ok();
    let result = client.poll_state(60, 500).await?;

    let state_str = &result.state;
    if state_str == "guest-ok" {
        let url = result.url.as_deref().unwrap_or("未知");
        window.emit("app-log", format!("[陶瓦] 已连接! 本地地址: {}", url)).ok();
    } else if state_str == "exception" {
        let excode = result.exception_type.unwrap_or(99);
        window.emit("app-log", format!("[陶瓦] 异常退出 (代码: {})", excode)).ok();
    } else {
        window.emit("app-log", format!("[陶瓦] 未知状态: {}", state_str)).ok();
    }

    serde_json::to_value(&result).map_err(|e| format!("序列化失败: {}", e))
}