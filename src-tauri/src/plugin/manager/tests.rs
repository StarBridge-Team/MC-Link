//! 插件管理器的单元测试：注册、路由、启停与上下文广播。

use std::path::PathBuf;

use super::*;

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("mclink-mgr-{}", crate::plugin::crypto::random_hex(6)))
}

#[test]
fn manager_registers_builtin_adapter() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    let records = mgr.list();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].manifest.id, TERRACOTTA_PLUGIN_ID);
    assert_eq!(records[0].source, PluginSource::Builtin);
    assert!(records[0].permissions.contains(Permission::NetListenLocal));
}

#[test]
fn routing_prefers_builtin_for_any_game() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    let plan = mgr.plan(PluginKind::Adapter, Some("minecraft-java"));
    assert_eq!(plan.primary(), Some(TERRACOTTA_PLUGIN_ID));
}

#[test]
fn game_registry_is_loaded() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    assert!(mgr.games().iter().any(|g| g.id == "minecraft-java"));
}

#[test]
fn disabled_plugin_is_not_routed() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    // 写启用状态需要 Plugins 目录存在（注册表文件落在那里）。
    std::fs::create_dir_all(dir.join("Plugins")).unwrap();
    mgr.set_enabled(TERRACOTTA_PLUGIN_ID, false).unwrap();
    assert!(mgr.plan(PluginKind::Adapter, None).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn broadcast_context_is_safe_without_plugins() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    mgr.broadcast_context(&crate::plugin::protocol::PluginContext {
        role: "host".to_string(),
        ..Default::default()
    });
    assert!(!mgr.peer_summaries().is_empty());
}

#[test]
fn gateway_is_not_started_until_async_start() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    let info = mgr.gateway_info();
    assert_eq!(info["running"], serde_json::json!(false));
    // 网关未启动时不应泄露任何地址信息。
    assert!(info.get("endpoint").is_none());
}

#[test]
fn adapter_dir_points_into_data_dir() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    assert_eq!(mgr.adapter_dir().unwrap(), dir.join("Adapter"));
}
