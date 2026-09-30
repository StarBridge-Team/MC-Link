use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;
use crate::plugin::manager::PluginManager;

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
pub(crate) fn close_window(window: tauri::Window, plugin_manager: tauri::State<'_, Arc<PluginManager>>) -> Result<String, String> {
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
pub(crate) fn exit_app(plugin_manager: tauri::State<'_, Arc<PluginManager>>, app: tauri::AppHandle) -> Result<String, String> {
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

/// 切换窗口效果（mica / acrylic / hud_window / none）。
///
/// 效果名到平台实现的映射集中在 `effect::apply_effect_by_name`，
/// 与启动时的预应用共用同一份逻辑，避免两处各写一份而漂移。
#[tauri::command]
pub(crate) fn set_window_effect(window: tauri::WebviewWindow, effect: String) -> Result<(), String> {
    crate::effect::apply_effect_by_name(&window, &effect);
    Ok(())
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
