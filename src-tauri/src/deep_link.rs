use tauri::{AppHandle, Emitter};
use serde::Serialize;

#[derive(Clone, Serialize, Debug)]
struct DeepLinkEvent {
    action: String,
    params: Vec<String>,
}

/// 解析 mclink:// 深度链接并向前端发送事件。
///
/// 格式：`mclink://<action>/<param1>/<param2>/...`
/// 例如：`mclink://plugin/abc123/download`
pub fn handle_deep_link(app: &AppHandle, urls: &[tauri::Url]) {
    for url in urls {
        if url.scheme() != "mclink" {
            continue;
        }
        let path = url.path().trim_start_matches('/');
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        let action = parts[0].to_string();
        let params = parts[1..].iter().map(|s| s.to_string()).collect();
        let event = DeepLinkEvent { action, params };
        let _ = app.emit("deep-link", &event);
    }
}

/// 是否需要由程序自己登记 `mclink://` 协议。
///
/// 登记动作交给官方插件（`DeepLinkExt::register_all`），这里只回答"该不该登记"。
/// 为什么必须显式判定形态：安装版由安装器登记协议，程序再自己写一遍会与安装器
/// 打架，且卸载或挪动安装目录后会**残留一个指向失效路径的协议项**。
///
/// - **Linux**：插件用 xdg-mime 写 `.desktop`，是唯一可行途径（AppImage 尤其需要）
/// - **Windows 便携版**：没有安装器可用，只能由程序自己登记
/// - **Windows 安装版 / macOS**：交给安装包与系统，程序不插手
pub fn should_register_scheme() -> bool {
    if cfg!(target_os = "linux") {
        return true;
    }
    #[cfg(windows)]
    if crate::datadir::install_mode() == crate::datadir::InstallMode::Portable {
        return true;
    }
    false
}
