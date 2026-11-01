//! 覆盖在最大化按钮上的原生子窗口。
//!
//! 它**不画任何像素**，唯一职责是让 Windows 的命中测试在按钮矩形上得到
//! `HTMAXBUTTON` —— 这是系统弹出贴靠布局（Snap Layouts）的必要条件。
//!
//! # 为什么用一个子窗口，而不是拦主窗口的消息
//!
//! 命中测试是**逐窗口**的：鼠标落在 WebView2 的客户区上时，是 WebView2 的子窗口收到
//! `WM_NCHITTEST`，主窗口完全收不到（输入路由由 Chromium 自己接管）。因此改主窗口的
//! 窗口过程无效。这里改成"在按钮位置盖一个自己的窗口"：它的窗口过程由我们注册，
//! 返回 `HTMAXBUTTON` 后系统就把这片区域当成最大化的非客户区按钮对待，
//! 后续 `WM_NCLBUTTON*` 也会发回给它，而不是被 WebView 吃掉。
//!
//! # 为什么不做窗口子类化
//!
//! 子类化是用来**接管别人窗口**的窗口过程的；这里的窗口类是我们自己注册的，
//! 窗口过程从一开始就是我们的，没有任何需要替换的东西。
//!
//! # 尺寸与坐标
//!
//! `SetWindowPos` 只认**物理像素**，而上报的矩形是 CSS 逻辑像素（见 [`super::tracker`]）。
//! 两者之间只差一个 `scale_factor`，换算集中在这里，避免前端再算一遍 DPI。

use std::sync::OnceLock;

use serde_json::json;
use tauri::Emitter;
use windows_sys::Win32::{
    Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::ValidateRect,
    UI::{
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT},
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, GetPropW, GetWindowLongPtrW, LoadCursorW,
            RegisterClassW, SetCursor, SetPropW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
            CREATESTRUCTW, GWLP_HINSTANCE, GWLP_USERDATA, HTMAXBUTTON, IDC_ARROW, SWP_NOACTIVATE,
            SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST,
            WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_PAINT, WM_SETCURSOR, WNDCLASSW,
            WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
        },
    },
};

use crate::core::events::topic;

use super::tracker::SnapBounds;

/// 窗口类名（进程内唯一，注册一次）。
const CLASS_NAME: &str = "McLinkSnapOverlay";
/// 挂在主窗口上的属性名，用来找回"已经建过的那个子窗口"。
///
/// 用 `SetPropW` 而不是自己维护 `HashMap<label, HWND>`：属性归窗口所有，
/// 窗口销毁时系统一并回收，不存在"表里留着已失效的 HWND"这种状态。
const CHILD_PROP: &str = "McLinkSnapOverlayChild";

/// 子窗口自己持有的状态（随窗口销毁一起释放）。
struct OverlayState {
    /// 顶层窗口：用于订阅失效时自检，也方便将来做原生点击兜底。
    #[allow(dead_code)]
    parent: HWND,
    /// 用来发事件的目标窗口（`emit_to` 只发给它自己）。
    window: tauri::WebviewWindow,
    /// 是否已经在悬停态：`WM_NCHITTEST` 每次鼠标移动都来，只在跃变时发事件。
    hovering: bool,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 类名/属性名的宽字符串必须**活到进程结束**：`RegisterClassW`、`SetPropW` 都不复制字符串。
fn class_name() -> &'static [u16] {
    static WIDE: OnceLock<Vec<u16>> = OnceLock::new();
    WIDE.get_or_init(|| wide(CLASS_NAME))
}

fn child_prop() -> &'static [u16] {
    static WIDE: OnceLock<Vec<u16>> = OnceLock::new();
    WIDE.get_or_init(|| wide(CHILD_PROP))
}

/// 应用一次矩形。`None` = 隐藏覆盖层（按钮不存在或上报无效）。
///
/// 必须在主线程调用：Tauri 的同步命令就跑在主线程上，所以这里直接调 Win32。
/// **不要**套一层 `run_on_main_thread` —— 那会变成"主线程等主线程"，直接死锁
/// （同类坑见 `commands/window.rs` 里 `room_window_open` 的说明）。
pub fn apply(window: &tauri::WebviewWindow, bounds: Option<SnapBounds>) -> Result<(), String> {
    let parent = parent_hwnd(window).ok_or_else(|| "拿不到窗口 HWND（非 Win32）".to_string())?;

    unsafe {
        let child = GetPropW(parent, child_prop().as_ptr()) as HWND;

        match bounds {
            None => {
                if !child.is_null() {
                    ShowWindow(child, SW_HIDE);
                    // 藏起来之后不会有 WM_NCMOUSELEAVE，这里手动清掉悬停态，
                    // 否则按钮会带着 hover 高亮直到下次鼠标进入。
                    let state = state_of(child);
                    if !state.is_null() {
                        (*state).hovering = false;
                    }
                }
                Ok(())
            }
            Some(bounds) => {
                let (x, y, width, height) =
                    bounds.to_physical(window.scale_factor().unwrap_or(1.0));

                let child = if child.is_null() {
                    create_child(parent, window, x, y, width, height)?
                } else {
                    child
                };

                ShowWindow(child, SW_SHOWNOACTIVATE);
                // `hwndinsertafter = null` 即 `HWND_TOP`：保证它压在 WebView2 子窗口之上，
                // 否则命中测试还会落到 WebView 上，贴靠布局永远不弹。
                SetWindowPos(
                    child,
                    std::ptr::null_mut(),
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE,
                );
                Ok(())
            }
        }
    }
}

/// 主窗口 HWND。
fn parent_hwnd(window: &tauri::WebviewWindow) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as HWND),
        _ => None,
    }
}

/// 取子窗口里存的状态指针（未初始化时为空）。
unsafe fn state_of(hwnd: HWND) -> *mut OverlayState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState
}

/// 注册窗口类（进程内一次）。失败返回 `false`，调用方转成错误信息。
unsafe fn ensure_class(instance: windows_sys::Win32::Foundation::HINSTANCE) -> bool {
    static READY: OnceLock<bool> = OnceLock::new();

    *READY.get_or_init(|| {
        let class = WNDCLASSW {
            lpfnWndProc: Some(overlay_proc),
            hInstance: instance,
            lpszClassName: class_name().as_ptr(),
            // 背景刷置空 + `WM_ERASEBKGND` 直接返回 1：这个窗口**永远不画**，
            // 按钮的外观完全由下面的 WebView 提供。
            ..Default::default()
        };
        RegisterClassW(&class) != 0
    })
}

/// 建子窗口并把自己挂到主窗口的属性上。
unsafe fn create_child(
    parent: HWND,
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND, String> {
    let instance =
        GetWindowLongPtrW(parent, GWLP_HINSTANCE) as windows_sys::Win32::Foundation::HINSTANCE;
    if !ensure_class(instance) {
        return Err("注册贴靠覆盖窗口类失败".to_string());
    }

    let state = Box::into_raw(Box::new(OverlayState {
        parent,
        window: window.clone(),
        hovering: false,
    }));

    let child = CreateWindowExW(
        0,
        class_name().as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
        x,
        y,
        width,
        height,
        parent,
        std::ptr::null_mut(),
        instance,
        state as *const core::ffi::c_void,
    );

    if child.is_null() {
        // `WM_NCCREATE` 没跑过（窗口根本没建起来）时，状态由这里释放；
        // 跑过则由 `WM_NCDESTROY` 释放，两者互斥。
        drop(Box::from_raw(state));
        return Err("创建贴靠覆盖窗口失败".to_string());
    }

    SetPropW(parent, child_prop().as_ptr(), child as HANDLE);
    Ok(child)
}

fn emit(state: &OverlayState, payload: serde_json::Value) {
    let label = state.window.label().to_string();
    let _ = state.window.emit_to(label, topic::WINDOW, payload);
}

/// 窗口过程：**整个特性的核心**就是 `WM_NCHITTEST` 返回 `HTMAXBUTTON`。
unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = lparam as *const CREATESTRUCTW;
            if !create.is_null() {
                let state = (*create).lpCreateParams as *mut OverlayState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_NCDESTROY => {
            let state = state_of(hwnd);
            if !state.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(state));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_NCHITTEST => {
            let state = state_of(hwnd);
            if state.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let state = &mut *state;

            // 只在"进入"这一跃变时发事件：WM_NCHITTEST 每次鼠标移动都会来。
            if !state.hovering {
                state.hovering = true;
                emit(state, json!({ "action": "snap-hover", "hovering": true }));

                // 子窗口不占焦点、也不走常规的 WM_MOUSELEAVE（那是客户区的），
                // 必须显式登记 NONCLIENT 的离开通知，否则 hover 状态永远清不掉。
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
            }

            HTMAXBUTTON as LRESULT
        }

        WM_NCMOUSELEAVE => {
            let state = state_of(hwnd);
            if !state.is_null() && (*state).hovering {
                (*state).hovering = false;
                emit(
                    &*state,
                    json!({ "action": "snap-hover", "hovering": false }),
                );
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        // 按下不交给系统自己处理：否则它会开始一轮"最大化按钮按下态"的跟踪，
        // 而按钮的样子是 WebView 画的，跟踪态没有视觉反馈只会闪烁。
        WM_NCLBUTTONDOWN if wparam == HTMAXBUTTON as usize => 0,

        // 抬起时只回传意图：最大化/还原仍走前端那条唯一的路径（`useWindowControls`），
        // 不在这里另开一套，避免"原生一套、JS 一套"两条状态机。
        WM_NCLBUTTONUP if wparam == HTMAXBUTTON as usize => {
            let state = state_of(hwnd);
            if !state.is_null() {
                emit(&*state, json!({ "action": "snap-click" }));
            }
            0
        }

        WM_SETCURSOR => {
            SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
            1
        }

        // 这个窗口永远不画：吞掉擦除与绘制，否则会盖出一块底色。
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            ValidateRect(hwnd, std::ptr::null());
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
