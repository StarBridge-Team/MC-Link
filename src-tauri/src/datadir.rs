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

/// 判断某个平台的便携标记是否成立。
///
/// # 各平台的处理方式与理由
///
/// - **Windows**：exe 同目录存在 `data/`、`portable.txt` 或 `portable` 之一即为便携版。
///   用户把安装版目录里放一个 `portable.txt` 即可转成便携形态，反之亦然。
///
/// - **Linux / macOS**：**刻意不提供便携模式**。这不是"还没做"，而是当前形态下做不到：
///
///   Tauri 在 Linux/macOS 上使用**系统 WebView**，产物是动态链接的。裸二进制
///   （`tauri build --no-bundle`，即"没有后缀的可执行文件"）运行时需要目标机已装
///   `webkit2gtk-4.1` + `gtk3` + `libsoup-3.0` + `librsvg2`，以及 WebKit 自己的
///   子进程（`WebKitWebProcess`/`WebKitNetworkProcess`）—— 这些来自系统包，
///   **无法随程序一起"独立"提供**。此外 glibc 下限由构建机决定，比构建机老的发行版
///   会直接报 `GLIBC_x.xx not found`。
///
///   因此"exe 同目录即全部"这个便携语义在 Linux 上不成立：把数据目录放到二进制旁边，
///   程序照样依赖系统库，并不能做到"拷走就能跑"。真正自带运行时的只有 Flatpak/Snap，
///   而那是系统级安装的沙箱形态，与便携版不是一回事（需要另一套 manifest 与工具链）。
///
///   macOS 更彻底：`/Applications` 对普通用户不可写，且会有 App Translocation
///   从只读随机路径运行；`.app` 的数据目录语义由 Gatekeeper 与签名共同决定。
///
///   **将来若要支持**（例如 AppImage 同目录），只需在这里加一个分支返回 `true`，
///   其余代码（`resolve_data_dir` / 更新落地 / 深链登记）都会顺着走。
fn portable_marker_present(exe_dir: &Path) -> bool {
    #[cfg(windows)]
    {
        exe_dir.join("data").is_dir()
            || exe_dir.join("portable.txt").is_file()
            || exe_dir.join("portable").is_file()
    }
    #[cfg(not(windows))]
    {
        let _ = exe_dir;
        false
    }
}

/// 当前运行形态。非 Windows 一律视为安装版（理由见 [`portable_marker_present`]）。
pub fn install_mode() -> InstallMode {
    if let Some(dir) = exe_dir() {
        if portable_marker_present(&dir) {
            return InstallMode::Portable;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclink-datadir-{}-{}",
            tag,
            crate::plugin::crypto::random_hex(5)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Windows 上三种标记任一存在即便携；都没有则为安装版。
    #[test]
    fn portable_marker_detection() {
        let dir = temp_dir("marker");

        // 都没有 → 不是便携
        assert!(!portable_marker_present(&dir));

        // `portable.txt`
        std::fs::write(dir.join("portable.txt"), b"").unwrap();
        assert!(portable_marker_present(&dir));

        #[cfg(not(windows))]
        {
            // 非 Windows 必须恒为 false：Linux/macOS 刻意不支持便携形态，
            // 哪怕目录里真放了 portable.txt 也不能据此改变数据目录位置。
            std::fs::remove_file(dir.join("portable.txt")).unwrap();
            std::fs::create_dir_all(dir.join("data")).unwrap();
            assert!(
                !portable_marker_present(&dir),
                "非 Windows 平台不得判定为便携版"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn data_dir_marker_and_plain_marker_are_equivalent() {
        let dir = temp_dir("data-marker");
        std::fs::create_dir_all(dir.join("data")).unwrap();
        assert!(portable_marker_present(&dir));
        let _ = std::fs::remove_dir_all(&dir);

        let dir2 = temp_dir("plain-marker");
        std::fs::write(dir2.join("portable"), b"").unwrap();
        assert!(portable_marker_present(&dir2));
        let _ = std::fs::remove_dir_all(&dir2);
    }
}
