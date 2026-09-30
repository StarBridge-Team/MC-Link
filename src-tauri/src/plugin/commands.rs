//! 插件子系统的 Tauri 命令。
//!
//! 这些命令面向**后续的插件管理界面**。当前前端尚未改动，因此它们只新增、
//! 不修改既有命令签名，旧页面完全不受影响。

use serde_json::{json, Value};
use std::sync::Arc;

use crate::plugin::manifest::PluginKind;
use crate::plugin::manager::PluginManager;
use crate::plugin::permission::Permission;

/// 列出所有插件（含内置）。
#[tauri::command]
pub(crate) fn plugin_list(manager: tauri::State<'_, Arc<PluginManager>>) -> Result<Value, String> {
    let items: Vec<Value> = manager
        .list()
        .iter()
        .map(|record| {
            let manifest = &record.manifest;
            json!({
                "id": manifest.id,
                "name": manifest.name,
                "version": manifest.version,
                "kind": manifest.kind.as_str(),
                "author": manifest.author,
                "description": manifest.description,
                "games": manifest.games,
                "source": match record.source {
                    crate::plugin::registry::PluginSource::Builtin => "builtin",
                    crate::plugin::registry::PluginSource::External => "external",
                },
                "trust": record.trust,
                "enabled": record.enabled,
                "runnable": record.trust.is_runnable(),
                "permissions": record.permissions.allowed(),
                "deniedByCeiling": record.permissions.denied_by_ceiling(),
                "declaredPermissions": manifest.permissions,
                "permissionDescriptions": manifest.permissions.iter()
                    .map(|p| json!({ "name": p, "description": p.describe(), "highRisk": p.is_high_risk() }))
                    .collect::<Vec<_>>(),
                "priority": manifest.priority,
                "fallback": manifest.fallback,
            })
        })
        .collect();

    Ok(json!({
        "plugins": items,
        "warnings": manager.warnings(),
    }))
}

/// 查询插件网关状态。网关**只监听回环地址**。
#[tauri::command]
pub(crate) fn plugin_gateway(manager: tauri::State<'_, Arc<PluginManager>>) -> Value {
    let mut info = manager.gateway_info();
    if let Some(map) = info.as_object_mut() {
        map.insert("loopbackOnly".to_string(), json!(true));
        map.insert(
            "protocolVersion".to_string(),
            json!(crate::plugin::protocol::PROTOCOL_VERSION),
        );
        map.insert(
            "subprotocol".to_string(),
            json!(crate::plugin::protocol::WS_SUBPROTOCOL),
        );
    }
    info
}

/// 列出已登记的游戏画像与标签。
#[tauri::command]
pub(crate) fn game_list(manager: tauri::State<'_, Arc<PluginManager>>) -> Vec<Value> {
    manager
        .games()
        .iter()
        .map(|g| {
            json!({
                "id": g.id,
                "name": g.name,
                "aliases": g.aliases,
                "port": g.default_port,
                "transport": g.transport.as_str(),
                "traits": g.traits,
                "processNames": g.process_names,
                "requiresCoupler": g.requires_coupler,
                "preferredAdapter": g.preferred_adapter,
                "fallbackAdapters": g.fallback_adapters,
            })
        })
        .collect()
}

/// 计算某能力在某游戏下的路由计划（用于诊断"为什么用这个插件"）。
#[tauri::command]
pub(crate) fn plugin_route_plan(
    manager: tauri::State<'_, Arc<PluginManager>>,
    kind: String,
    game_id: Option<String>,
) -> Result<Value, String> {
    let kind = parse_kind(&kind)?;
    let plan = manager.plan(kind, game_id.as_deref());
    Ok(json!({
        "kind": kind.as_str(),
        "gameId": plan.game_id,
        "primary": plan.primary(),
        "candidates": plan.candidates.iter()
            .map(|c| json!({ "pluginId": c.plugin_id, "reason": c.reason, "score": c.score }))
            .collect::<Vec<_>>(),
    }))
}

/// 启用/停用插件。
#[tauri::command]
pub(crate) fn plugin_set_enabled(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    manager.set_enabled(&plugin_id, enabled)
}

/// 写入用户授权集合；`granted` 为 `null` 表示撤销用户决策、回落到清单声明。
#[tauri::command]
pub(crate) fn plugin_set_grants(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    granted: Option<Vec<Permission>>,
) -> Result<(), String> {
    manager.set_grants(&plugin_id, granted)
}

/// 拉黑/解除拉黑插件。
#[tauri::command]
pub(crate) fn plugin_set_blocked(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    blocked: bool,
) -> Result<(), String> {
    manager.set_blocked(&plugin_id, blocked)
}

/// 重新扫描磁盘上的插件目录。
#[tauri::command]
pub(crate) fn plugin_reload(manager: tauri::State<'_, Arc<PluginManager>>) -> Vec<String> {
    manager.reload()
}

fn parse_kind(kind: &str) -> Result<PluginKind, String> {
    match kind.to_ascii_lowercase().as_str() {
        "adapter" => Ok(PluginKind::Adapter),
        "detector" => Ok(PluginKind::Detector),
        "coupler" => Ok(PluginKind::Coupler),
        other => Err(format!("未知插件种类: {}", other)),
    }
}
