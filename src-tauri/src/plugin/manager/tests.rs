//! 插件管理器的单元测试：注册、路由、启停与上下文广播。

use std::path::PathBuf;

use super::*;

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mclink-mgr-{}",
        crate::plugin::crypto::random_hex(6)
    ))
}

#[test]
fn manager_registers_builtin_adapters() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    let records = mgr.list();
    assert_eq!(records.len(), 4);
    let ids: Vec<&str> = records.iter().map(|r| r.manifest.id.as_str()).collect();
    assert!(ids.contains(&TERRACOTTA_PLUGIN_ID));
    assert!(ids.contains(&EASYTIER_PLUGIN_ID));
    assert!(ids.contains(&MINECRAFT_DETECTOR_PLUGIN_ID));
    assert!(ids.contains(&MINECRAFT_COUPLER_PLUGIN_ID));
    assert!(records.iter().all(|r| r.source == PluginSource::Builtin));
    let terra = records
        .iter()
        .find(|r| r.manifest.id == TERRACOTTA_PLUGIN_ID)
        .unwrap();
    assert!(terra.permissions.contains(Permission::NetListenLocal));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn routing_prefers_terracotta_over_generic_easytier() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    let plan = mgr.plan(PluginKind::Adapter, Some("minecraft-java"));
    // 两条内置适配器都覆盖全部游戏；能与上游互通的陶瓦应是首选
    assert_eq!(plan.primary(), Some(TERRACOTTA_PLUGIN_ID));
    assert!(plan.fallbacks().contains(&EASYTIER_PLUGIN_ID));
    let _ = std::fs::remove_dir_all(&dir);
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

    // 停用首选后，通用 EasyTier 适配器仍在候选里（回退链），但首选必须变
    let plan = mgr.plan(PluginKind::Adapter, None);
    assert_ne!(plan.primary(), Some(TERRACOTTA_PLUGIN_ID));
    assert_eq!(plan.primary(), Some(EASYTIER_PLUGIN_ID));

    // 两条都停用后才是空计划
    mgr.set_enabled(EASYTIER_PLUGIN_ID, false).unwrap();
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
fn builtin_records_are_all_registered_as_builtin_source() {
    let dir = temp_dir();
    let mgr = PluginManager::new(&dir).unwrap();
    assert!(mgr
        .list()
        .iter()
        .all(|record| record.source == PluginSource::Builtin));
    let _ = std::fs::remove_dir_all(&dir);
}

/// peer 的可调用列表是**三道闸门**的交集：已启用、未被拉黑、生效了 `peer_message`。
///
/// 这条用例把"目标方没声明就收不到消息"钉住——它是权限模型双向闸门的接收侧，
/// 一旦退化，任何插件都能往别的插件里塞消息。
#[test]
fn peer_list_only_exposes_declared_and_granted_plugins() {
    let dir = temp_dir();
    std::fs::create_dir_all(dir.join("Plugins")).unwrap();
    let plugin_dir = dir.join("Plugins").join("dev.example.peer");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::json!({
            "id": "dev.example.peer",
            "name": "示例对端",
            "version": "1.0.0",
            "kind": "utility",
            "permissions": ["peer_message", "fs_plugin_data"]
        })
        .to_string(),
    )
    .unwrap();

    let mgr = PluginManager::new(&dir).unwrap();

    // 未验签插件默认停用 → 不可见
    assert!(
        mgr.peer_list("dev.a.a").is_empty(),
        "未启用的插件不得出现在可调用列表里"
    );

    mgr.set_enabled("dev.example.peer", true).unwrap();
    // 已启用但用户没授权 peer_message（未验签插件只拿到 MINIMAL）→ 仍不可见
    assert!(
        mgr.peer_list("dev.a.a").is_empty(),
        "未授予 peer_message 的插件不得出现在可调用列表里"
    );

    mgr.set_grants(
        "dev.example.peer",
        Some(vec![Permission::PeerMessage, Permission::FsPluginData]),
    )
    .unwrap();
    let list = mgr.peer_list("dev.a.a");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].plugin_id, "dev.example.peer");
    assert_eq!(list[0].version, "1.0.0");
    assert!(!list[0].alive, "没有握过手，alive 必须是 false");

    // 自己不会出现在自己的列表里
    assert!(mgr.peer_list("dev.example.peer").is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// 目标方闸门：**没拿到 `peer_message` 的插件，核心直接拒绝中转**。
///
/// 这是"双向闸门"的接收侧。少了它，任何持有 `peer_message` 的插件都能往
/// 任意插件里塞消息（哪怕对方从没声明要参与互调）。
#[tokio::test]
async fn relay_rejects_target_without_peer_message() {
    let dir = temp_dir();
    std::fs::create_dir_all(dir.join("Plugins")).unwrap();
    let plugin_dir = dir.join("Plugins").join("dev.example.peer");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::json!({
            "id": "dev.example.peer",
            "name": "示例对端",
            "version": "1.0.0",
            "kind": "utility",
            "permissions": ["peer_message"]
        })
        .to_string(),
    )
    .unwrap();

    let mgr = PluginManager::new(&dir).unwrap();
    mgr.set_enabled("dev.example.peer", true).unwrap();

    // 已启用，但未授权 peer_message（未验签插件只拿到 MINIMAL）
    let request = crate::plugin::peer::CallRequest {
        to: "dev.example.peer".into(),
        method: "room.list".into(),
        params: serde_json::Value::Null,
        depth: 0,
        launch: false,
        timeout_ms: 1_000,
    };
    let err = super::peer_hub::relay_call(&mgr.inner, "dev.a.a", &request)
        .await
        .unwrap_err();
    assert_eq!(
        err.code,
        crate::plugin::protocol::error_code::FORBIDDEN.to_string()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
