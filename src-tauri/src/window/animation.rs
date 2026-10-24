//! 窗口级显隐动画：**交给系统渲染**。
//!
//! # 为什么不用 CSS
//!
//! CSS 方案要求"先播退场动画，再 `hide()`"：于是必须有前端 ack + 看门狗兜底，
//! 且动画期间窗口仍在吃点击（表现为"要点两次才关得掉"）。系统动画由合成器完成，
//! 不需要前后端握手，也不改变窗口形态（无边框 / 透明 / 材质都保持原样）。
//!
//! # 失败必须静默降级
//!
//! Windows 的透明窗口可能是 layered window，`AnimateWindow` 在这种窗口上不保证成功。
//! 返回 0 时**直接退回瞬时显隐**：动画是锦上添花，绝不能让它挡住"窗口能不能打开"。
//! macOS 与 Linux 目前是 no-op——macOS 需要 `NSWindow.animationBehavior`（objc2），
//! Linux 由合成器自带，两者都不必也不能在 Rust 侧硬做。

use tauri::WebviewWindow;

/// 动画时长。与 `--motion-medium` 同量级，足够被感知又不拖慢操作。
const DURATION_MS: u32 = 140;

/// 显示窗口（带动画）。
pub fn show(window: &WebviewWindow) {
    if platform_animate(window, true) {
        return;
    }
    let _ = window.show();
    let _ = window.set_focus();
}

/// 隐藏窗口（带动画）。
pub fn hide(window: &WebviewWindow) {
    if platform_animate(window, false) {
        return;
    }
    let _ = window.hide();
}

#[cfg(windows)]
fn platform_animate(window: &WebviewWindow, showing: bool) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AnimateWindow, AW_ACTIVATE, AW_BLEND, AW_HIDE,
    };

    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return false;
    };

    let flags = if showing {
        AW_BLEND | AW_ACTIVATE
    } else {
        AW_BLEND | AW_HIDE
    };
    // 同步阻塞约 DURATION_MS：调用方都是命令线程/事件回调，不在渲染线程上。
    let ok = unsafe { AnimateWindow(win32.hwnd.get() as _, DURATION_MS, flags) };
    ok != 0
}

#[cfg(not(windows))]
fn platform_animate(_window: &WebviewWindow, _showing: bool) -> bool {
    false
}
