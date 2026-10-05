//! 内置插件登记与会话摘要。

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::plugin::manifest::PluginManifest;
use crate::plugin::permission::{PermissionSet, TrustLevel};
use crate::plugin::registry::{PluginRecord, PluginSource};
use crate::plugin::session::SessionHandle;

/// 内置插件清单与注册记录。
///
/// 内置插件的清单直接来自核心代码，因此信任度是 `Official`；但它们仍然以标准
/// 能力方法被调度，不走特例。
pub(super) fn builtin_records() -> Result<Vec<PluginRecord>, String> {
    let mut records = Vec::new();
    for manifest in [
        crate::plugin::builtin::terracotta::manifest(),
        crate::plugin::builtin::minecraft_scanner::manifest(),
        crate::plugin::builtin::minecraft_coupler::manifest(),
    ] {
        let permissions = PermissionSet::from_declared(&manifest.permissions);
        records.push(PluginRecord {
            manifest,
            source: PluginSource::Builtin,
            dir: PathBuf::new(),
            trust: TrustLevel::Official,
            enabled: true,
            permissions,
        });
    }
    Ok(records)
}

/// 供诊断与命令层使用：把会话句柄转成可读状态。
pub fn session_summary(handle: &SessionHandle) -> Value {
    json!({
        "pluginId": handle.plugin_id,
        "sessionId": handle.session_id,
        "connectedAt": handle.connected_at,
        "permissions": handle
            .permissions
            .allowed()
            .iter()
            .map(|p| crate::plugin::permission::perm_name(*p))
            .collect::<Vec<_>>(),
    })
}
