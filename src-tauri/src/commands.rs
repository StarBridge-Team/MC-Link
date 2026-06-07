use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;
use tauri::Emitter;
use tauri::Manager;
use serde::Serialize;
use futures_util::StreamExt;
use crate::central;
use crate::client::ClientMode;
use crate::host::HostMode;
use crate::lan;
use crate::protocol;
use crate::state::AppState;
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
    *state.latency_ms.lock().unwrap()
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
                        *state.latency_ms.lock().unwrap() = ms;
                        let _ = window.emit("latency-update", ms);
                        Ok::<_, std::io::Error>(())
                    })
                    .is_ok();
                if !ping_ok {
                    *state.latency_ms.lock().unwrap() = 999;
                    let _ = window.emit("latency-update", 999u64);
                }
            }
            *state.latency_ms.lock().unwrap() = 0;
            let _ = window.emit("latency-update", 0u64);
        })
        .ok();
}

/// 中继服务器选择结果
struct RelaySelection {
    addr: std::net::SocketAddr,
    id: String,
}

/// 选择并解析中继服务器
fn select_relay(selected_relay: &str, window: &tauri::Window) -> Result<RelaySelection, String> {
    if selected_relay == "__auto__" || selected_relay.is_empty() {
        window.emit("app-log", "[启动] 自动选择中继服务器...".to_string()).ok();
        let relays = central::get_relays().ok_or("网络错误，请检查网络连接")?;
        if relays.is_empty() {
            return Err("没有可用的中继服务器".to_string());
        }
        let relay = &relays[0];
        window.emit("app-log", format!("[启动] 选中中继: {} ({})", relay.name, relay.address)).ok();
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
    *current_room.lock().unwrap() = Some((room_name, password));
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

    central::join_room(&room_name, &player_name, "host", Some(relay.id.as_str()));

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

    central::join_room(&room_name, &player_name, "member", Some(relay.id.as_str()));

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
        *state.stop_signal.lock().unwrap() = Some(stop_signal.clone());

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
    if let Some(sig) = state.stop_signal.lock().unwrap().take() {
        sig.store(true, Ordering::Relaxed);
    }
    thread::sleep(Duration::from_millis(500));
    state.is_running.store(false, Ordering::Relaxed);
    let room = state.current_room.lock().unwrap().clone();
    if let Some((ref room_name, _)) = room {
        central::delete_room(room_name);
    }
    *state.current_room.lock().unwrap() = None;
    *state.latency_ms.lock().unwrap() = 0;
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
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("获取可执行文件路径失败: {}", e))?
        .parent()
        .ok_or("无法获取可执行文件目录")?
        .to_path_buf();

    let adapter_dir = exe_dir.join("Adapter").join("Terracotta");
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

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut buffer = Vec::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取数据失败: {}", e))?;
        downloaded += chunk.len() as u64;
        buffer.extend_from_slice(&chunk);
        if total > 0 {
            let pct = (downloaded as f64 / total as f64 * 100.0) as u8;
            let _ = window.emit("download-progress", pct);
        }
    }

    if buffer.is_empty() {
        return Err("下载失败: 文件为空".to_string());
    }

    std::fs::write(&archive_path, &buffer)
        .map_err(|e| format!("写入文件失败: {}", e))?;

    window.emit("app-log", format!("[下载] 下载完成 ({} bytes)，开始解压...", buffer.len())).ok();

    let file = std::fs::File::open(&archive_path)
        .map_err(|e| format!("打开下载文件失败: {}", e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(&adapter_dir)
        .map_err(|e| format!("解压失败: {}", e))?;

    std::fs::remove_file(&archive_path)
        .map_err(|e| format!("删除安装包失败: {}", e))?;

    window.emit("app-log", "[下载] 陶瓦联机已安装完成，正在启动...".to_string()).ok();

    let manager = adapter_manager.lock().unwrap();
    manager.launch_terracotta_after_download();

    Ok("陶瓦联机已安装并启动".to_string())
}

#[tauri::command]
pub(crate) fn get_adapter_status(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<crate::adapter::AdapterStatus, String> {
    let manager = adapter_manager.lock().unwrap();
    Ok(manager.get_status())
}

#[tauri::command]
pub(crate) fn start_adapter(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    let manager = adapter_manager.lock().unwrap();
    manager.start_terracotta()
}

#[tauri::command]
pub(crate) fn stop_adapter(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    let manager = adapter_manager.lock().unwrap();
    manager.stop_terracotta()
}

#[tauri::command]
pub(crate) async fn get_terracotta_state(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().unwrap();
        let p = *mgr.terracotta_port.lock().unwrap();
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
        let mgr = adapter_manager.lock().unwrap();
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

// ===== OAuth2 命令 =====

#[tauri::command]
pub(crate) fn get_oauth_user(oauth_state: tauri::State<'_, Arc<crate::oauth::OAuthState>>) -> Result<Option<crate::oauth::OAuthUser>, String> {
    Ok(oauth_state.get_user())
}

#[tauri::command]
pub(crate) fn is_oauth_logged_in(oauth_state: tauri::State<'_, Arc<crate::oauth::OAuthState>>) -> bool {
    oauth_state.is_logged_in()
}

#[tauri::command]
pub(crate) fn get_oauth_config(oauth_state: tauri::State<'_, Arc<crate::oauth::OAuthState>>) -> Result<Option<crate::oauth::OAuthProvider>, String> {
    Ok(oauth_state.get_config())
}

#[tauri::command]
pub(crate) fn save_oauth_config(
    oauth_state: tauri::State<'_, Arc<crate::oauth::OAuthState>>,
    config: crate::oauth::OAuthProvider,
) -> Result<(), String> {
    oauth_state.save_config(config);
    Ok(())
}

#[tauri::command]
pub(crate) async fn oauth_login(
    oauth_state: tauri::State<'_, Arc<crate::oauth::OAuthState>>,
    window: tauri::Window,
) -> Result<crate::oauth::OAuthUser, String> {
    window.emit("app-log", "[OAuth] 正在启动登录流程...".to_string()).ok();
    window.emit("app-log", "[OAuth] 将在浏览器中打开授权页面".to_string()).ok();

    let state = oauth_state.inner().clone();
    let result = tokio::task::spawn_blocking(move || {
        state.login()
    })
    .await
    .map_err(|e| format!("登录线程异常: {}", e))??;

    window.emit("app-log", format!("[OAuth] 登录成功: {}", result.name)).ok();
    Ok(result)
}

#[tauri::command]
pub(crate) fn oauth_logout(
    oauth_state: tauri::State<'_, Arc<crate::oauth::OAuthState>>,
    window: tauri::Window,
) -> Result<(), String> {
    oauth_state.logout();
    window.emit("app-log", "[OAuth] 已登出".to_string()).ok();
    Ok(())
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

fn setting_dir() -> Result<std::path::PathBuf, String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("获取可执行文件路径失败: {}", e))?
        .parent()
        .ok_or("无法获取可执行文件目录")?
        .to_path_buf();
    let dir = exe_dir.join("Setting");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建设置目录失败: {}", e))?;
    Ok(dir)
}

#[tauri::command]
pub(crate) fn get_setting(section: String) -> Result<String, String> {
    let section_file = section.replace(" ", "_").replace("/", "_").replace("\\", "_");
    let path = setting_dir()?.join(format!("{}.yml", section_file));
    if !path.exists() {
        return Ok(format!("# {}\n", section));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取设置文件失败: {}", e))
}

#[tauri::command]
pub(crate) fn save_setting(section: String, content: String) -> Result<(), String> {
    let section_file = section.replace(" ", "_").replace("/", "_").replace("\\", "_");
    let path = setting_dir()?.join(format!("{}.yml", section_file));
    std::fs::write(&path, &content).map_err(|e| format!("保存设置文件失败: {}", e))
}

#[tauri::command]
pub(crate) async fn start_terracotta_guest(
    adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>,
    window: tauri::Window,
    room_code: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().unwrap();
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