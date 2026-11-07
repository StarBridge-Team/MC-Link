//! 插件包安装/卸载的 Tauri 命令。
//!
//! 单独成文件而不是塞进 `commands.rs`：那边是"清单/路由/授权"的查询门面，
//! 这里是"改磁盘"的写操作，两者的失败模式与审计要求完全不同。

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::plugin::install::Installed;
use crate::plugin::manager::PluginManager;

/// 安装结果的对外形状（camelCase，与前端契约一致）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    /// `verified` / `unsigned`。
    pub trust: String,
    /// 非致命提示（未签名、覆盖安装等）。
    pub warnings: Vec<String>,
    /// 解压后的实际体积（字节）。
    pub bytes: u64,
}

impl From<Installed> for InstalledPlugin {
    fn from(value: Installed) -> Self {
        Self {
            id: value.id,
            name: value.name,
            version: value.version,
            trust: match value.trust {
                crate::plugin::permission::TrustLevel::Verified => "verified".to_string(),
                crate::plugin::permission::TrustLevel::Official => "official".to_string(),
                crate::plugin::permission::TrustLevel::Blocked => "blocked".to_string(),
                crate::plugin::permission::TrustLevel::Unsigned => "unsigned".to_string(),
            },
            warnings: value.warnings,
            bytes: value.bytes,
        }
    }
}

/// 从本地文件安装插件包（`.mclink-plugin`，本质是 tar.gz）。
///
/// `allowUnsigned = false` 时未签名包一律拒绝；`true` 表示用户已在界面上确认
/// "我知道它没签名，仍然安装"——安装器据此以未签名身份安装并**显式启用**。
///
/// `async`：解压 + 哈希校验是重 IO，同步命令会占住主线程。
#[tauri::command]
pub(crate) async fn plugin_install_file(
    manager: tauri::State<'_, Arc<PluginManager>>,
    path: String,
    allow_unsigned: Option<bool>,
) -> Result<InstalledPlugin, String> {
    let manager = manager.inner().clone();
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        return Err("插件包路径为空".to_string());
    }
    let installed = manager.install_package(&path, allow_unsigned.unwrap_or(false))?;
    Ok(installed.into())
}

/// 卸载插件。`removeData = false` 时保留 `<data_dir>/PluginData/<id>/`。
#[tauri::command]
pub(crate) async fn plugin_uninstall(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    remove_data: Option<bool>,
) -> Result<(), String> {
    let manager = manager.inner().clone();
    manager.uninstall(plugin_id.trim(), remove_data.unwrap_or(false))
}
