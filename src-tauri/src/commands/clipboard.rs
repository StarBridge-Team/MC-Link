//! 系统剪贴板：只提供「写文本」。
//!
//! # 为什么放在后端而不是前端
//!
//! 前端要把内容放进剪贴板，走 `navigator.clipboard` 需要 webview 授予剪贴板权限，
//! 走 tauri 插件则需要给前端开 `clipboard-manager:*` 权限——两种都等于把「读写系统剪贴板」
//! 的能力暴露给整个前端。而这里的实际需求只有一处：把房间码/邀请链接复制出去。
//!
//! 因此改为后端提供命令、前端只传文本，前端始终拿不到剪贴板能力。
//!
//! # 为什么不用 `tauri-plugin-clipboard-manager`
//!
//! 它在 Windows 上会经 `arboard` 的 `wayland-data-control` 特性把 gtk 依赖树
//! （gtk / gdk / glib / cairo / pango…）拉进 `Cargo.lock`，而这里只需要写文本。
//! 直接依赖 arboard 即可，体积与依赖面都小得多。

/// 把文本写入系统剪贴板。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub(crate) fn write_clipboard_text(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("打开剪贴板失败: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("写入剪贴板失败: {}", e))
}

// 移动端没有可写的系统剪贴板：保留同名命令，让前端调用处不必按平台分叉。
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub(crate) fn write_clipboard_text(_text: String) -> Result<(), String> {
    Err("当前平台不支持写入剪贴板".to_string())
}
