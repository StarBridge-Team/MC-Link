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

/// 在 Windows 注册表中注册 mclink:// URL 协议。
///
/// 仅 Windows 便携版需要（安装版由 NSIS 处理，Linux 由 .desktop 处理）。
/// 写入 `HKCU\Software\Classes\mclink`，无需管理员/UAC 权限。
/// 注册结果。
#[derive(Clone, Serialize, Debug)]
pub struct SchemeRegisterResult {
    pub success: bool,
    pub message: String,
}

#[cfg(windows)]
pub fn register_scheme() -> SchemeRegisterResult {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return SchemeRegisterResult { success: false, message: format!("获取可执行文件路径失败: {}", e) },
    };
    let exe_path = exe.to_string_lossy().to_string();
    let quoted = format!("\"{}\"", exe_path);

    let key = match winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey(r"Software\Classes\mclink") {
        Ok((k, _)) => k,
        Err(e) => return SchemeRegisterResult { success: false, message: format!("注册表写入失败（可能权限不足）: {}", e) },
    };

    if let Err(e) = key.set_value("", &"URL:MC Link Protocol") {
        return SchemeRegisterResult { success: false, message: format!("设置默认值失败: {}", e) };
    }
    if let Err(e) = key.set_value("URL Protocol", &"") {
        return SchemeRegisterResult { success: false, message: format!("设置 URL Protocol 失败: {}", e) };
    }

    let shell = match key.create_subkey("shell\\open\\command") {
        Ok((k, _)) => k,
        Err(e) => return SchemeRegisterResult { success: false, message: format!("创建 command 子键失败: {}", e) },
    };
    if let Err(e) = shell.set_value("", &format!("{} --deep-link \"%1\"", quoted)) {
        return SchemeRegisterResult { success: false, message: format!("设置 command 失败: {}", e) };
    }

    SchemeRegisterResult { success: true, message: "mclink:// 协议注册成功".into() }
}

#[cfg(not(windows))]
pub fn register_scheme() -> SchemeRegisterResult {
    SchemeRegisterResult { success: true, message: "当前平台无需注册".into() }
}
