use std::path::{Path, PathBuf};
use tauri::Manager;

/// 判断 Windows 是否以单文件/便携模式运行。
#[cfg(windows)]
fn is_portable_mode(exe_dir: &Path) -> bool {
    exe_dir.join("data").is_dir() || exe_dir.join("portable.txt").is_file() || exe_dir.join("portable").is_file()
}

/// 解析应用数据目录绝对路径。
///
/// - Windows 便携模式：可执行文件同目录
/// - Windows 安装模式 / Linux / macOS：Tauri app_data_dir
pub fn resolve_data_dir(app: &tauri::App) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(exe_dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        {
            if is_portable_mode(&exe_dir) {
                return exe_dir;
            }
        }
    }

    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

pub fn setting_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let dir = data_dir.join("Setting");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建设置目录失败: {}", e))?;
    Ok(dir)
}

pub fn assets_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let dir = data_dir.join("Assets");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 Assets 目录失败: {}", e))?;
    Ok(dir)
}

pub fn background_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("Background")
}

/// 旧版兼容：可执行文件同目录的 Background
pub fn old_background_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("Background"))
}
