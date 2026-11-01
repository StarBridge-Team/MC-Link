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
                        // 最小化时窗口**仍然 visible**：只看 `is_visible` 会把"还原"
                        // 误判成"收起"，用户点了托盘反而把窗口藏起来。
                        let visible = window.is_visible().unwrap_or(false);
                        let minimized = window.is_minimized().unwrap_or(false);
                        if visible && !minimized {
                            let _ = window.hide();
                        } else {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    position,
                    rect,
                    ..
                } => {
                    // 位置与尺寸由事件自带的 `position` / `rect` 决定，不自己查光标。
                    // 面板是**预建常驻**的（见 `window::widget`），这里只负责摆放与显隐。
                    //
                    // # 必须离开主线程
                    //
                    // `show_tray_panel` 在面板不存在时会去建窗口，而
                    // `WebviewWindowBuilder::build()` 内部要 `run_on_main_thread` 并
                    // **阻塞等待**它执行完。托盘事件回调跑在主线程上 → 主线程等主线程
                    // → 死锁：托盘面板永远出不来，整个应用失去响应。
                    // 放到运行时线程上，这次等待才有意义（同 `room_window_open` 必须
                    // 写成 `async` 命令是同一个道理）。
                    let app_handle = app_handle.clone();
                    let position = position.clone();
                    let rect = rect.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) =
                            crate::window::widget::show_tray_panel(&app_handle, position, rect)
                        {
                            eprintln!("[托盘] 显示面板失败: {}", e);
                        }
                    });
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}
