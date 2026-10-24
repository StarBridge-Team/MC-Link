use crate::plugin::manager::PluginManager;
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;

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

/// 关闭主窗口。
///
/// 只停止适配器（含其托管的第三方进程），**不做**完整插件系统关停：
/// 关闭窗口不等于退出应用（托盘左键仍可隐藏/唤出窗口），
/// 关停网关与远程插件会让仍在运行的应用失效。
#[tauri::command]
pub(crate) fn close_window(
    window: tauri::Window,
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<String, String> {
    plugin_manager.shutdown_adapters()?;
    let _ = window.close();
    Ok("已退出".to_string())
}

#[tauri::command]
pub(crate) fn drag_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

/// 退出应用。
///
/// 这里只停适配器（与改造前行为一致，保证第三方进程先于本进程退出）；
/// 完整关停（关闭远程插件与网关）由 `lib.rs` 的 `RunEvent::Exit` 处理器统一执行，
/// 不在此处重复调用 [`PluginManager::shutdown`]。
#[tauri::command]
pub(crate) fn exit_app(
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    plugin_manager.shutdown_adapters()?;
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
pub(crate) fn resize_window(
    window: tauri::Window,
    width: f64,
    height: f64,
    min_width: Option<f64>,
    min_height: Option<f64>,
    center: bool,
) {
    // 尺寸来自前端：NaN / 负数 / 0 会让窗口变成不可用状态（甚至直接消失），先挡住
    let valid = |v: f64| v.is_finite() && (1.0..=100_000.0).contains(&v);
    if !valid(width) || !valid(height) {
        eprintln!("[窗口] 忽略非法尺寸 {}x{}", width, height);
        return;
    }

    // 最小尺寸支持**单边**设置：此前要求两边同时给出，只传一边会被静默忽略
    let min_w = min_width.filter(|v| valid(*v));
    let min_h = min_height.filter(|v| valid(*v));
    if min_w.is_some() || min_h.is_some() {
        // 未给出的那一边用当前实际尺寸兜底（这才是合理的下限）
        let current = window
            .inner_size()
            .map(|s| (s.width as f64, s.height as f64))
            .unwrap_or((width, height));
        let _ = window.set_min_size(Some(tauri::LogicalSize::new(
            min_w.unwrap_or(current.0),
            min_h.unwrap_or(current.1),
        )));
    }

    let _ = window.set_size(tauri::LogicalSize::new(width, height));
    if center {
        let _ = window.center();
    }
}

/// 切换窗口效果（mica / acrylic / hud_window / none）。
///
/// 效果名到平台实现的映射集中在 `effect::apply_effect`，
/// 与启动时的预应用共用同一份逻辑，避免两处各写一份而漂移。
///
/// `tint` 是材质染色浓度（0–100，越高越实心），`dark` 决定染色往黑还是往白走。
/// 两者都与"当前设置"一起传，而不是在这里回读配置：调用方（前端）手里就有最新值，
/// 回读会多一次磁盘 IO，且拖动滑块时可能与落盘存在时序差。
#[tauri::command]
pub(crate) fn set_window_effect(
    window: tauri::WebviewWindow,
    effect: String,
    tint: f64,
    dark: bool,
) -> Result<(), String> {
    crate::effect::apply_effect(&window, &effect, tint, dark);
    Ok(())
}

/// 打开主窗口并请求它落到「联机」页（托盘面板的「最近游戏」用它）。
///
/// 用事件而不是直接切路由：托盘面板是独立窗口，它无权驱动主窗口的 store。
/// 发一条意图，由主窗口自己决定怎么消费（见 `src/ipc/bridge.ts`）。
#[tauri::command]
pub(crate) fn app_show_connect(app: tauri::AppHandle, process: Option<String>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit(
        crate::core::events::topic::APP_INTENT,
        serde_json::json!({ "action": "connect", "process": process }),
    );
}

/// 打开（或唤出）某个会话的房间窗口。
///
/// 前端在 `room_start_host` / `room_join` 成功后立刻调用它，因此用户看到的是
/// "房间窗口里正在创建房间"，而不是主窗口里转一圈再跳走。
#[tauri::command]
pub(crate) fn room_window_open(
    app: tauri::AppHandle,
    session_id: String,
    room_code: String,
) -> Result<String, String> {
    crate::window::factory::open_room_window(&app, &session_id, &room_code)
}

#[tauri::command]
pub(crate) fn room_window_focus(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<(), String> {
    crate::window::factory::focus_room_window(&app, &session_id)
}

/// 关闭房间窗口但**不断开**链路：房间还在跑，只是不看了。
#[tauri::command]
pub(crate) fn room_window_close(app: tauri::AppHandle, session_id: String) {
    crate::window::factory::close_room_window(&app, &session_id);
}

#[tauri::command]
pub(crate) fn set_window_dark_mode(window: tauri::WebviewWindow, dark: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::effect::set_immersive_dark_mode(&window, dark);
    }
    let _ = (&window, dark);
    Ok(())
}
