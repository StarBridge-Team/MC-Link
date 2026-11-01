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
/// 面板圆角半径（逻辑像素）。必须与 `WidgetApp.vue` 的 `border-radius` 一致。
const PANEL_RADIUS: f64 = 30.0;

/// 主窗口的 label（Wayland 降级定位时用作锚点）。
const MAIN_LABEL: &str = "main";

// 这里曾经有 `SHADOW_MARGIN`：窗口比内容大一圈，给 CSS 阴影留位置。
// 现在**不做阴影**（用户明确不要），窗口尺寸与内容严格相等——
// 那圈透明边距会露出 WebView 的白色底，表现为"面板下面多一片白"。

/// 当前是否运行在 Wayland 会话下。
///
/// # 为什么必须区分 Wayland 与 X11
///
/// Wayland 的 xdg-shell 协议**不允许客户端自行设置顶层窗口位置**：位置由合成器管理，
/// `set_position` 会被静默忽略。X11 则允许自由定位。
///
/// GNOME / KDE 默认走 Wayland，所以不做这个判断的话，托盘面板会落到合成器给的位置
/// （常见的是屏幕中部），而不是托盘图标正下方。
///
/// **注意**：这是平台协议层面的判断（依据 xdg-shell 规范），本机是 Windows，
/// **无法在开发机上实测**；实际表现需要在真实 Wayland 会话里验证。
fn is_wayland_session() -> bool {
    std::env::var("WAYLAND_DISPLAY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// 启动时调用一次：建好面板但保持隐藏。
pub fn ensure_tray_panel(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(TRAY_LABEL).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, TRAY_LABEL, WebviewUrl::App("widget.html".into()))
        .title("MC Link")
        .inner_size(PANEL_WIDTH, 200.0)
        // 显式把窗口与 WebView 底色都设成全透明：没有材质兜底时，
        // WebView 的默认白底会从圆角外露出来（Linux 上**永远**没有材质兜底，
        // 透明能否生效完全取决于 WebKitGTK/合成器是否支持，详见 `crate::effect`）。
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

    // 材质兜底**只存在于 Windows / macOS**（Linux 上 `set_effects` 是空操作），
    // 平台差异与排查线索都写在 `crate::effect::apply_tray_menu_effect` 的注释里。
    crate::effect::apply_tray_menu_effect(&window);

    // 失焦即收起；尺寸变化后重算圆角裁剪。
    //
    // **Resized 这一条不能省**：`set_size` 是异步投递的，紧接着调 `apply_round_region`
    // 时 `inner_size()` 往往还是旧值，region 按旧尺寸算 → 窗口变大后右下角露白。
    // 挂到 Resized 上，等尺寸真正生效后再裁。
    let w = window.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Focused(false) => crate::window::animation::hide(&w),
        tauri::WindowEvent::Resized(_) => {
            crate::window::region::apply_round_region(&w, PANEL_RADIUS)
        }
        _ => {}
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
    crate::window::region::apply_round_region(&window, PANEL_RADIUS);

    if is_wayland_session() {
        // Wayland：xdg-shell 不允许客户端设置顶层窗口位置，**贴着托盘图标是做不到的**。
        // 退而求其次——以**主窗口**为锚（`outer_position()` / `outer_size()` 是读操作，
        // Wayland 下可用），让面板出现在主窗口下方、用户看得见的地方。
        //
        // 仍然调 `set_position`：它是唯一能用的定位 API，有些环境会采纳；
        // 但 Wayland 下**不能指望它生效**，所以下面必须留一行日志，
        // 否则"面板没贴着托盘图标"这个现象会变成查不到线索的灵异事件。
        let scale = window.scale_factor().unwrap_or(1.0);
        match place_below_main_window(app, scale) {
            Some((x, y)) => {
                eprintln!("[托盘] Wayland 会话：窗口位置由合成器接管，无法贴着托盘图标定位；已改为定位到主窗口下方，实际位置可能仍有偏差");
                let _ = window.set_position(PhysicalPosition::new(x, y));
            }
            None => {
                eprintln!("[托盘] Wayland 会话：读不到主窗口位置，已跳过定位；面板位置完全由合成器决定，不会贴着托盘图标");
            }
        }
    } else {
        // X11 / Windows：可以自由定位，沿用"贴着托盘图标"的位置计算。
        let scale = window
            .monitor_from_point(anchor.x, anchor.y)
            .ok()
            .flatten()
            .map(|m| m.scale_factor())
            .unwrap_or(1.0);
        let (x, y) = place(&window, anchor, tray, height, scale);
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }

    crate::window::animation::show(&window);
    let _ = window.set_focus();
    Ok(())
}

/// Wayland 降级用：面板贴到**主窗口**下方（水平居中）。
///
/// 用主窗口的 `outer_position()` + `outer_size()`：这两个是**读操作**，Wayland 下可用
/// （虽然坐标由合成器给出、可能与像素级预期有偏差，但至少是"用户看得见"的位置）。
fn place_below_main_window(app: &AppHandle, scale: f64) -> Option<(i32, i32)> {
    let main = app.get_webview_window(MAIN_LABEL)?;
    let pos = main.outer_position().ok()?;
    let size = main.outer_size().ok()?;

    let panel_w = (PANEL_WIDTH * scale).round() as i32;
    let x = pos.x + (size.width as i32 - panel_w) / 2;
    let y = pos.y + size.height as i32 + GAP;
    Some((x, y))
}

pub fn hide_tray_panel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TRAY_LABEL) {
        crate::window::animation::hide(&window);
    }
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
