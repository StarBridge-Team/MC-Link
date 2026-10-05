use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// 安装形态。
///
/// 它决定两件互相关联的事：**用户数据放在哪里**，以及**更新时怎么落地**
/// （便携版直接替换 exe；安装版必须交给安装器，因为文件受注册表/系统目录管理）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMode {
    /// 便携版：可执行文件所在目录即数据目录。
    Portable,
    /// 安装版：数据目录在系统 app_data_dir。
    Installed,
}

impl InstallMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallMode::Portable => "portable",
            InstallMode::Installed => "installed",
        }
    }
}

/// 当前可执行文件的绝对路径。
///
/// 更新落地需要它：便携版要替换这个文件，安装版要据此在安装结束后重新拉起应用。
pub fn exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

fn exe_dir() -> Option<PathBuf> {
    exe_path().and_then(|p| p.parent().map(Path::to_path_buf))
}

/// 判断 Windows 是否以单文件/便携模式运行。
///
/// 约定（保持既有行为不变）：exe 同目录存在 `data/` 目录、`portable.txt`
/// 或 `portable` 文件之一即视为便携版。用户把安装版目录里放一个 `portable.txt`
/// 即可转成便携形态，反之亦然。
#[cfg(windows)]
fn is_portable_mode(exe_dir: &Path) -> bool {
    exe_dir.join("data").is_dir()
        || exe_dir.join("portable.txt").is_file()
        || exe_dir.join("portable").is_file()
}

/// 当前运行形态。非 Windows 一律视为安装版。
pub fn install_mode() -> InstallMode {
    #[cfg(windows)]
    {
        if let Some(dir) = exe_dir() {
            if is_portable_mode(&dir) {
                return InstallMode::Portable;
            }
        }
    }
    InstallMode::Installed
}

/// 解析应用数据目录绝对路径。
///
/// - Windows 便携模式：可执行文件同目录
/// - Windows 安装模式 / Linux / macOS：Tauri app_data_dir
pub fn resolve_data_dir(app: &tauri::App) -> PathBuf {
    if install_mode() == InstallMode::Portable {
        if let Some(dir) = exe_dir() {
            return dir;
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
