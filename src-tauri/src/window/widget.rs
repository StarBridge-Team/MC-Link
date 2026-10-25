//! 托盘面板：一个**预建常驻**的外部小组件窗口。
//!
//! # 为什么预建
//!
//! Windows 上冷建一个 WebView 是 200–600ms。此前每次右键都新建、失焦就 `close()`，
//! 用户看到的是"点了没反应，然后突然弹出"。现在启动时建好并 `visible(false)`，
//! 之后只做 `show/hide`。
//!
//! # 为什么尺寸与位置在 Rust 算
//!
//! 只有 Rust 拿得到托盘图标的 `rect`、显示器工作区与缩放比例。前端只管内容，
//! 用 `height: 100%` 填满——由它量高度会先出现一帧错误尺寸再跳。

use std::sync::Arc;

use tauri::{AppHandle, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindowBuilder};

use crate::mgr::room::RoomMgr;
use crate::plugin::manager::PluginManager;

pub const TRAY_LABEL: &str = "widget-tray";

/// 面板逻辑宽度。
const PANEL_WIDTH: f64 = 240.0;
/// 单行高度（与 `WidgetApp.vue` 的行高保持一致）。
const ROW_HEIGHT: f64 = 40.0;
/// 面板内边距合计。
const PADDING: f64 = 20.0;
/// 与托盘图标之间的间隙。
const GAP: i32 = 6;

// 这里曾经有 `SHADOW_MARGIN`：窗口比内容大一圈，给 CSS 阴影留位置。
// 现在**不做阴影**（用户明确不要），窗口尺寸与内容严格相等——
// 那圈透明边距会露出 WebView 的白色底，表现为"面板下面多一片白"。

/// 启动时调用一次：建好面板但保持隐藏。
pub fn ensure_tray_panel(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(TRAY_LABEL).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, TRAY_LABEL, WebviewUrl::App("widget.html".into()))
        .title("MC Link")
        .inner_size(PANEL_WIDTH, 200.0)
        // 显式把窗口与 WebView 底色都设成全透明：没有材质兜底时，
        // WebView2 的默认白底会从圆角外露出来。
        .background_color(tauri::window::Color(0, 0, 0, 0))
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| format!("创建托盘面板失败: {}", e))?;

    // **不套窗口材质**：材质画在窗口矩形上，会把 DOM 的圆角与阴影整个盖住，
    // 看起来永远是方角。面板自己画底色 + 圆角 + 阴影（见 `WidgetApp.vue`）。

    // 失焦即收起。动画由 `animation` 负责，失败会退化成瞬时隐藏。
    let w = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Focused(false)) {
            crate::window::animation::hide(&w);
        }
    });

    Ok(())
}

/// 面板高度 = 打开 + 退出 + 进行中的房间 + 最近游戏（最多 3 条）。
pub fn panel_height(app: &AppHandle) -> f64 {
    let rooms = app
        .try_state::<Arc<RoomMgr>>()
        .map(|mgr| mgr.list().len())
        .unwrap_or(0);
    let recent = app
        .try_state::<Arc<PluginManager>>()
        .map(|mgr| mgr.local_games().len().min(3))
        .unwrap_or(0);
    let rows = 2 + rooms + recent;
    (PADDING + ROW_HEIGHT * rows as f64).clamp(120.0, 460.0)
}

/// 在托盘图标附近显示面板；已经显示时收起（当作开关）。
pub fn show_tray_panel(
    app: &AppHandle,
    anchor: PhysicalPosition<f64>,
    tray: Rect,
) -> Result<(), String> {
    ensure_tray_panel(app)?;
    let window = app
        .get_webview_window(TRAY_LABEL)
        .ok_or_else(|| "托盘面板不存在".to_string())?;

    if window.is_visible().unwrap_or(false) {
        crate::window::animation::hide(&window);
        return Ok(());
    }

    let height = panel_height(app);
    let _ = window.set_size(tauri::LogicalSize::new(PANEL_WIDTH, height));
    // 尺寸变了，region 要按新尺寸重算（它记的是像素矩形）。
    apply_round_region(&window);

    let scale = window
        .monitor_from_point(anchor.x, anchor.y)
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    let (x, y) = place(&window, anchor, tray, height, scale);
    let _ = window.set_position(PhysicalPosition::new(x, y));

    crate::window::animation::show(&window);
    let _ = window.set_focus();
    Ok(())
}

pub fn hide_tray_panel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TRAY_LABEL) {
        crate::window::animation::hide(&window);
    }
}

/// 把窗口裁成圆角矩形。
///
/// # 为什么必须裁窗口，而不是只写 CSS
///
/// 窗口是**矩形**的。CSS 的 `border-radius` 只裁剪 DOM 的绘制，裁不掉窗口本身的底色——
/// 去掉材质后那层底色就是 WebView2 的白，于是圆角外面会露出四个白角 / 一条白边。
/// `SetWindowRgn` 直接把窗口形状改成圆角矩形，白底连同一并消失。
///
/// 圆角半径与 `WidgetApp.vue` 的 `border-radius` 必须一致（30px）。
/// 窗口尺寸变化后要重新应用：region 是按当时的像素尺寸算的。
#[cfg(windows)]
fn apply_round_region(window: &tauri::WebviewWindow) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};

    /// 与前端 `border-radius` 一致的逻辑像素值。
    const RADIUS: f64 = 30.0;

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let radius = (RADIUS * scale).round() as i32;

    // 椭圆宽高是**直径**；右/下边界是开区间，+1 才覆盖到最后一个像素。
    let region = unsafe {
        CreateRoundRectRgn(
            0,
            0,
            size.width as i32 + 1,
            size.height as i32 + 1,
            radius * 2,
            radius * 2,
        )
    };
    if region.is_null() {
        return;
    }
    // 成功后 region 归系统所有，**不要**再 DeleteObject。
    unsafe {
        SetWindowRgn(win32.hwnd.get() as _, region, 1);
    }
}

#[cfg(not(windows))]
fn apply_round_region(_window: &tauri::WebviewWindow) {
    // macOS / Linux 由窗口自身（或合成器）处理圆角；CSS 的 border-radius 足够。
}

/// 位置计算：优先在图标下方，放不下就翻到上方；最后一律夹进工作区。
///
/// 夹取不是保险措施而是必需：任务栏可能在任意一条边上，面板宽度也可能超出
/// 图标所在位置能容纳的范围——不夹就会出现"点托盘什么都不显示"（窗口在屏幕外）。
fn place(
    window: &tauri::WebviewWindow,
    anchor: PhysicalPosition<f64>,
    tray: Rect,
    height: f64,
    scale: f64,
) -> (i32, i32) {
    let monitor = window
        .monitor_from_point(anchor.x, anchor.y)
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    let (left, top, width, height_avail) = match monitor {
        Some(m) => {
            let area = m.work_area();
            (
                area.position.x,
                area.position.y,
                area.size.width as i32,
                area.size.height as i32,
            )
        }
        // 拿不到显示器信息时用一个保守的默认框，总比把窗口放到屏幕外好。
        None => (0, 0, 1920, 1080),
    };

    let panel_w = (PANEL_WIDTH * scale).round() as i32;
    let panel_h = (height * scale).round() as i32;

    // `Rect` 的位置/尺寸是 `Position`/`Size` 枚举（可能是 Logical），
    // 统一换算成物理像素再算，否则高 DPI 下会偏。
    let icon = tray.position.to_physical::<i32>(scale);
    let icon_size = tray.size.to_physical::<u32>(scale);

    // 面板水平居中对齐托盘图标；下方放不下就翻到上方。
    let center_x = icon.x + icon_size.width as i32 / 2;
    let mut x = center_x - panel_w / 2;

    let below = icon.y + icon_size.height as i32 + GAP;
    let above = icon.y - panel_h - GAP;
    let mut y = if below + panel_h <= top + height_avail {
        below
    } else {
        above
    };

    // 夹取进工作区：任务栏可能在任意一条边上，不夹就会"点托盘什么都不显示"。
    x = x.clamp(left + GAP, left + width - panel_w - GAP);
    y = y.clamp(top + GAP, top + height_avail - panel_h - GAP);
    (x, y)
}
