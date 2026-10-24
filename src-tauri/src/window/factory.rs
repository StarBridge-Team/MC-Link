//! 房间窗口的创建与聚焦。
//!
//! 形态与主窗口一致：无边框 + 透明 + 当前材质。**不要**为了"看起来像独立窗口"
//! 改成系统装饰——那会让房间窗口和主窗口出现两种观感。

use std::sync::Arc;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::mgr::AppMgr;

/// 房间窗口 label 前缀。与 `SessionId::window_label` 必须一致。
const ROOM_PREFIX: &str = "room-";

pub fn room_label(session_id: &str) -> String {
    format!("{}{}", ROOM_PREFIX, session_id)
}

/// 打开（或唤出）某个会话的房间窗口。
///
/// 已存在时只做 `show + focus`：重复点击"打开房间"不该把窗口重建一遍，
/// 那会丢掉用户调过的尺寸与位置，也会让正在播放的过渡动画闪一下。
pub fn open_room_window(
    app: &AppHandle,
    session_id: &str,
    room_code: &str,
) -> Result<String, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("会话 ID 为空".to_string());
    }
    let label = room_label(session_id);

    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(label);
    }

    let title = if room_code.trim().is_empty() {
        "MC Link · 房间".to_string()
    } else {
        format!("MC Link · {}", room_code.trim())
    };

    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("room.html".into()))
        .title(title)
        .inner_size(720.0, 560.0)
        .min_inner_size(480.0, 360.0)
        .decorations(false)
        .transparent(true)
        .resizable(true)
        // 先不显示：由 `animation::show` 决定用系统动画还是瞬时显示，
        // 否则会先闪一帧无动画的窗口。
        .visible(false)
        .center()
        .build()
        .map_err(|e| format!("创建房间窗口失败: {}", e))?;

    // 与主窗口共用同一套材质设置：新窗口不继承，必须显式应用一次，
    // 否则房间窗口在深色模式下会是一块不透明的白底。
    if let Some(mgr) = app.try_state::<Arc<AppMgr>>() {
        let pers = mgr.read().personalization().unwrap_or_default();
        let dark = pers.theme_mode == "dark";
        crate::effect::apply_effect(&window, &pers.transparent_effect, pers.effect_tint, dark);
    }

    crate::window::animation::show(&window);
    Ok(label)
}

/// 聚焦已存在的房间窗口；窗口不在时返回错误，由前端决定是否重新创建。
pub fn focus_room_window(app: &AppHandle, session_id: &str) -> Result<(), String> {
    let label = room_label(session_id.trim());
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("房间窗口不存在: {}", label))?;
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

/// 关闭某个会话的房间窗口（不触碰链路）。
pub fn close_room_window(app: &AppHandle, session_id: &str) {
    if let Some(window) = app.get_webview_window(&room_label(session_id.trim())) {
        let _ = window.close();
    }
}
