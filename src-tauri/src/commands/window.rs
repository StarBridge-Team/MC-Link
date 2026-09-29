use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri::Manager;
use crate::adapter::AdapterManager;

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
pub(crate) fn close_window(window: tauri::Window, adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    if let Ok(manager) = adapter_manager.lock() {
        manager.shutdown_all();
    }
    let _ = window.close();
    Ok("已退出".to_string())
}

#[tauri::command]
pub(crate) fn drag_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn exit_app(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>, app: tauri::AppHandle) -> Result<String, String> {
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
pub(crate) fn set_window_effect(window: tauri::WebviewWindow, effect: String) -> Result<(), String> {
    // macOS: 切换到非 hud_window 效果时先卸载已添加的 vibrancy 视图
    #[cfg(target_os = "macos")]
    {
        if effect != "hud_window" {
            crate::effect::remove_vibrancy_backdrop();
        }
    }
    match effect.as_str() {
        "mica" => {
            #[cfg(windows)]
            {
                crate::effect::apply_mica_backdrop_typed(&window, 2);
            }
        }
        "acrylic" => {
            #[cfg(windows)]
            {
                crate::effect::apply_mica_backdrop_typed(&window, 3);
            }
        }
        "hud_window" => {
            #[cfg(target_os = "macos")]
            {
                crate::effect::apply_vibrancy_backdrop(&window);
            }
        }
        _ => {
            // "none" / "transparent"：显式卸载 DWM backdrop（DWMSBT_NONE = 1），
            // 避免从 mica/acrylic 切换过来时旧材质残留
            #[cfg(windows)]
            {
                crate::effect::apply_mica_backdrop_typed(&window, 1);
            }
        }
    }
    let _ = &window;
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
