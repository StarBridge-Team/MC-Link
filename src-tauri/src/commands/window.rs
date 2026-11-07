use crate::plugin::manager::PluginManager;
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;

#[tauri::command]
pub(crate) fn drag_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

/// 从窗口边缘发起缩放拖动。方向是 `n` / `s` / `e` / `w` / `ne` / `nw` / `se` / `sw`。
///
/// # 为什么需要它
///
/// 无边框窗口在 Linux 上**没有可拖的边缘**：tao 里实现这一手的是 Windows 的
/// `WM_NCHITTEST` 缩放分支，GTK 侧不会自己给窗口边缘留热区。前端因此在窗口四边
/// 放了 8 条透明热区（`ui/WindowResizeEdges.vue`），按下时把方向传过来。
///
/// # 为什么是自定义命令，而不是 JS 的 `startResizeDragging`
///
/// 那条 API 走 `plugin:window|start_resize_dragging` 权限，得给**每一个**窗口的
/// capability 都配上（主窗口、房间窗口、托盘面板、各类小组件），漏配时没有任何
/// 报错，表现只是"边缘拖不动"。自己的命令不受 ACL 限制，与 `drag_window` 同一取舍
/// （见 `RoomApp.vue` 里那段说明）。
///
/// # 为什么方向是字符串
///
/// 前端只该表达"哪条边"，不该知道 `ResizeDirection`。这里集中做一次校验，
/// 非法值报错而不是悄悄当成某个方向缩放。
#[tauri::command]
pub(crate) fn resize_drag(window: tauri::Window, direction: String) -> Result<(), String> {
    use tauri_runtime::ResizeDirection;

    let direction = match direction.as_str() {
        "n" => ResizeDirection::North,
        "s" => ResizeDirection::South,
        "e" => ResizeDirection::East,
        "w" => ResizeDirection::West,
        "ne" => ResizeDirection::NorthEast,
        "nw" => ResizeDirection::NorthWest,
        "se" => ResizeDirection::SouthEast,
        "sw" => ResizeDirection::SouthWest,
        other => return Err(format!("未知的缩放方向: {other}")),
    };

    window
        .start_resize_dragging(direction)
        .map_err(|e| e.to_string())
}

/// 退出应用。
///
/// 这里只停适配器（保证第三方进程先于本进程退出）；完整关停（关闭远程插件与网关）
/// 由 `lib.rs` 的 `RunEvent::Exit` 处理器统一执行，不在此处重复调用
/// [`PluginManager::shutdown`]。
///
/// **适配器停不掉也要退出**：用户点的是"退出"，卡在收尾上不退出只会让人去任务管理器。
/// 失败留痕即可——进程退出后系统会回收残留子进程（Windows 上子进程不会自动结束，
/// 但 `shutdown` 里的 `kill` 才是主路径，这里是它的兜底）。
#[tauri::command]
pub(crate) fn exit_app(
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    if let Err(e) = plugin_manager.shutdown_adapters() {
        eprintln!("[退出] 停止适配器失败（仍然退出）: {}", e);
    }
    app.exit(0);
    Ok("已退出".to_string())
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

/// 关闭**调用方窗口**。
///
/// 房间窗口的自绘关闭按钮用它。**不要**复用 `close_window`：那个命令会先停适配器，
/// 语义是"关主窗口 = 结束联机"，在房间窗口上调用会把正在跑的房间一起掐掉。
#[tauri::command]
pub(crate) fn close_self_window(window: tauri::WebviewWindow) {
    let _ = window.close();
}

/// 隐藏**调用方窗口**（小组件面板收起用）。
#[tauri::command]
pub(crate) fn hide_self_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
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
/// # 为什么必须是 `async fn`
///
/// `WebviewWindowBuilder::build()` 内部要 `run_on_main_thread(...)` 并**阻塞等待**它执行完。
/// 同步命令本身就跑在主线程上，于是变成"主线程等主线程"——直接死锁：
/// 房间窗口永远建不完，主窗口也跟着失去响应（最小化/关闭/托盘全部无反应）。
/// 异步命令跑在运行时的线程池里，这条等待才有意义。
#[tauri::command]
pub(crate) async fn room_window_open(
    app: tauri::AppHandle,
    session_id: String,
    room_code: String,
) -> Result<String, String> {
    crate::window::factory::open_room_window(&app, &session_id, &room_code)
}

#[tauri::command]
pub(crate) async fn room_window_focus(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<(), String> {
    crate::window::factory::focus_room_window(&app, &session_id)
}

/// 关闭房间窗口但**不断开**链路：房间还在跑，只是不看了。
#[tauri::command]
pub(crate) async fn room_window_close(app: tauri::AppHandle, session_id: String) {
    crate::window::factory::close_room_window(&app, &session_id);
}

/// 打开（或唤出）设置独立窗口。
///
/// 与 `room_window_open` 同理必须 `async`：建窗内部要 `run_on_main_thread` 并阻塞等待，
/// 同步命令会变成"主线程等主线程"直接死锁。
#[tauri::command]
pub(crate) async fn settings_window_open(app: tauri::AppHandle) -> Result<(), String> {
    crate::window::factory::open_settings_window(&app)
}

/// 上报最大化按钮矩形（CSS 逻辑像素），供 Windows 贴靠布局覆盖层跟随。
///
/// `bounds` 为 `None` 表示按钮不在了（例如窗口切到没有最大化键的布局），覆盖层隐藏。
///
/// # 为什么是**同步**命令
///
/// 贴靠覆盖层是原生子窗口，创建/移动都必须在主线程上做。Tauri 的同步命令本身
/// 就跑在主线程（这正是 `room_window_open` 必须写成 `async` 的原因），所以这里
/// 直接调 Win32 即可 —— **不能**再套 `run_on_main_thread`，那会变成"主线程等主线程"
/// 直接死锁。
#[tauri::command]
pub(crate) fn snap_set_bounds(
    window: tauri::WebviewWindow,
    tracker: tauri::State<'_, Arc<crate::window::snap::SnapTracker>>,
    bounds: Option<crate::window::snap::SnapBounds>,
) -> Result<(), String> {
    use crate::window::snap::SnapChange;

    // 缩放比例一起进比较：跨屏拖动时 CSS 矩形不变但物理位置变了（见 `SnapTracker`）。
    let scale = window.scale_factor().unwrap_or(1.0);

    match tracker.update(window.label(), bounds, scale) {
        // 与上次完全一样：拖动窗口时 ResizeObserver 会高频重复上报，这里直接短路，
        // 免得每次都 SetWindowPos 让 DWM 重新合成一遍。
        SnapChange::Unchanged => Ok(()),
        SnapChange::Hide => crate::window::snap::update(&window, None),
        SnapChange::Set(bounds) => crate::window::snap::update(&window, Some(bounds)),
    }
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
