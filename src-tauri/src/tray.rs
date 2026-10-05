use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

// 这里曾经有一个 `get_cursor_pos()`：Windows 上走 user32 的 `GetCursorPos` FFI，
// 其它平台返回 `(0, 0)`。**已整体删除**。
//
// 为什么要删：`TrayIconEvent::Click` 事件本身就带 `position: PhysicalPosition<f64>`
// （触发事件的光标位置）与 `rect: Rect`（托盘图标的位置与尺寸），两者都由 Tauri
// 在所有平台上提供 —— 我们的 FFI 是在重复实现一个已经存在的值。
//
// 顺带解决了两件事：
// 1. **跨平台无分支**：不必为每个平台各维护一份实现（Linux 还要面对
//    原生 Wayland 下 X11 API 拿不到坐标的问题）；
// 2. **Flatpak 可用**：沙箱里拿不到 `user32`/X11 的全局指针位置，而事件里的
//    `position` 由 Tauri 随事件一起交付，不受沙箱限制。

pub fn setup_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = TrayIconBuilder::new().tooltip("MC Link");
    // 缺图标时不要 unwrap：那是**启动路径**上的 panic（打包配置漏了图标就变成开机崩溃），
    // 退化成"没有图标的托盘"要好得多。
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    } else {
        eprintln!("[托盘] 未找到应用图标，托盘将不显示图标");
    }

    builder
        .on_tray_icon_event(|tray, event| {
            let app_handle = tray.app_handle();
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    position,
                    ..
                } => {
                    if let Some(existing) = app_handle.get_webview_window("tray-menu") {
                        let _ = existing.close();
                    }

                    // 位置直接取事件自带的 `position`（本次点击的光标物理坐标，
                    // 跨平台由 Tauri 提供）——不再自己查全局光标位置。
                    let (x, y) = (position.x, position.y);

                    if let Ok(window) = tauri::WebviewWindowBuilder::new(
                        app_handle,
                        "tray-menu",
                        tauri::WebviewUrl::App("tray-menu.html".into()),
                    )
                    .position(x, y)
                    .inner_size(180.0, 125.0)
                        .resizable(false)
                        .decorations(false)
                        .always_on_top(true)
                        .skip_taskbar(true)
                        .transparent(true)
                        .build()
                    {
                        // 材质交给官方 API（见 effect::apply_tray_menu_effect），
                        // 这里不再自己写 DwmSetWindowAttribute
                        crate::effect::apply_tray_menu_effect(&window);
                        let w = window.clone();
                        window.on_window_event(move |event| {
                            if let tauri::WindowEvent::Focused(false) = event {
                                let _ = w.close();
                            }
                        });
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}
