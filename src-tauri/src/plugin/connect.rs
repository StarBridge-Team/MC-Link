//! 联机页编排命令：把检测器 / 适配器 / 耦合器串成一次完整联机。
//!
//! 这些是「前端联机页」的专用编排层，直接复用 [`PluginManager`] 的能力路由
//! （Detector / Adapter / Coupler 三种类按游戏路由并自动回退）。
//!
//! 关键约定：**加入房间需要哪些字段由适配器自己通过 `adapter.init` 声明**
//! （返回 `join_fields`），前端据此动态渲染表单——本文件绝不硬编码房间码 /
//! 密码之类的字段形态，因此换一个适配器无需改前端。

use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use crate::plugin::manager::PluginManager;
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::Permission;
use crate::plugin::protocol::{
    adapter_method, coupler_method, detector_method, GameInfo, PluginContext, RoomInfo,
};

/// 前端订阅的联机事件（`connect-event`）。
#[derive(Clone, Serialize)]
pub struct ConnectEvent {
    /// `progress` | `connected` | `error` | `stopped`
    pub stage: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nat_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_addr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

fn connect_event(stage: &str, message: &str, role: Option<&str>) -> ConnectEvent {
    ConnectEvent {
        stage: stage.to_string(),
        message: message.to_string(),
        role: role.map(|s| s.to_string()),
        room_code: None,
        nat_type: None,
        peer_addr: None,
        elapsed_ms: None,
    }
}

fn emit_event(app: &AppHandle, evt: ConnectEvent) {
    let _ = app.emit("connect-event", &evt);
}

/// 列出可用于联机的适配器，并带上各适配器自声明的 `join_fields`。
#[tauri::command]
pub async fn connect_adapters(
    manager: tauri::State<'_, Arc<PluginManager>>,
    game_id: Option<String>,
) -> Result<Value, String> {
    let plan = manager.plan(PluginKind::Adapter, game_id.as_deref());
    let mut adapters = Vec::new();
    for candidate in &plan.candidates {
        let Some(record) = manager.get(&candidate.plugin_id) else {
            continue;
        };
        // 让适配器自报 join 字段；拿不到就当没有（前端按"无需字段"处理）。
        let join_fields = manager
            .invoke(
                PluginKind::Adapter,
                game_id.as_deref(),
                adapter_method::INIT,
                json!({}),
            )
            .await
            .ok()
            .and_then(|(_, v)| v.get("join_fields").cloned())
            .unwrap_or(Value::Array(vec![]));
        adapters.push(json!({
            "pluginId": record.manifest.id,
            "name": record.manifest.name,
            "trust": record.trust,
            "ready": record.enabled && record.trust.is_runnable(),
            "joinFields": join_fields,
        }));
    }
    Ok(json!({ "adapters": adapters }))
}

/// 以房主身份创建房间：detector.scan → adapter.host.start → coupler.attach → 广播上下文。
///
/// 在后台任务里跑，立即返回适配器 ID；进度通过 `connect-event` 推给前端。
/// detector / coupler 任一步失败都不致命，仅记日志继续。
#[tauri::command]
pub async fn connect_start_host(
    manager: tauri::State<'_, Arc<PluginManager>>,
    app: AppHandle,
    adapter_id: String,
    game_id: Option<String>,
    fields: Value,
    player_name: String,
) -> Result<String, String> {
    let mgr = (*manager).clone();
    let app2 = app.clone();
    let aid = adapter_id.clone();
    let _ = tauri::async_runtime::spawn(async move {
        let provider = match mgr.provider(&aid).await {
            Ok(p) => p,
            Err(e) => {
                emit_event(&app2, connect_event("error", &e.message, Some("host")));
                return;
            }
        };

        // 1) 扫描本地游戏（best-effort），拿到端口用于游戏上下文。
        let port = match mgr
            .invoke(
                PluginKind::Detector,
                game_id.as_deref(),
                detector_method::SCAN,
                json!({}),
            )
            .await
        {
            Ok((_, v)) => v.get("port").and_then(|p| p.as_u64()).map(|p| p as u16),
            Err(e) => {
                eprintln!("[联机] 本地游戏扫描失败（不影响建房间）: {}", e.message);
                None
            }
        };

        emit_event(&app2, connect_event("progress", "正在创建房间…", Some("host")));

        // 2) 适配器建房间。
        let mut params = fields.clone();
        if let Value::Object(ref mut m) = params {
            m.insert("player_name".to_string(), json!(player_name));
            m.insert("role".to_string(), json!("host"));
            if let Some(p) = port {
                m.insert("game".to_string(), json!({ "port": p }));
            }
        }
        let value = match provider
            .invoke_checked(Permission::NetConnectAny, adapter_method::HOST_START, params)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                emit_event(&app2, connect_event("error", &e.message, Some("host")));
                return;
            }
        };
        let room_code = value
            .get("room")
            .and_then(|v| v.as_str())
            .or_else(|| fields.get("room_code").and_then(|v| v.as_str()))
            .map(|s| s.to_string());

        // 3) 耦合器广播（best-effort）。
        if let Err(e) = mgr
            .invoke(
                PluginKind::Coupler,
                game_id.as_deref(),
                coupler_method::ATTACH,
                json!({ "motd": "MC Link", "port": port.unwrap_or(25565) }),
            )
            .await
        {
            eprintln!("[联机] 耦合器广播失败（不影响建房间）: {}", e.message);
        }

        // 4) 广播上下文（角色 / 游戏 / 房间）。
        mgr.broadcast_context(&PluginContext {
            role: "host".to_string(),
            game: port.map(|p| GameInfo {
                port: Some(p),
                ..Default::default()
            }),
            room: room_code.as_ref().map(|c| RoomInfo {
                code: c.clone(),
                ..Default::default()
            }),
            peers: vec![],
        });

        let mut evt = connect_event("connected", "房间已创建", Some("host"));
        evt.room_code = room_code;
        emit_event(&app2, evt);
    });
    Ok(adapter_id)
}

/// 以访客身份加入房间：adapter.join → coupler.attach → 广播上下文。
#[tauri::command]
pub async fn connect_join(
    manager: tauri::State<'_, Arc<PluginManager>>,
    app: AppHandle,
    adapter_id: String,
    game_id: Option<String>,
    fields: Value,
    player_name: String,
) -> Result<String, String> {
    let mgr = (*manager).clone();
    let app2 = app.clone();
    let aid = adapter_id.clone();
    let _ = tauri::async_runtime::spawn(async move {
        let provider = match mgr.provider(&aid).await {
            Ok(p) => p,
            Err(e) => {
                emit_event(&app2, connect_event("error", &e.message, Some("guest")));
                return;
            }
        };

        emit_event(&app2, connect_event("progress", "正在加入房间…", Some("guest")));

        let mut params = fields.clone();
        if let Value::Object(ref mut m) = params {
            m.insert("player_name".to_string(), json!(player_name));
            m.insert("role".to_string(), json!("guest"));
        }
        if let Err(e) = provider
            .invoke_checked(Permission::NetConnectAny, adapter_method::JOIN, params)
            .await
        {
            emit_event(&app2, connect_event("error", &e.message, Some("guest")));
            return;
        }
        let room_code = fields
            .get("room_code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 耦合器广播（best-effort）。
        if let Err(e) = mgr
            .invoke(
                PluginKind::Coupler,
                game_id.as_deref(),
                coupler_method::ATTACH,
                json!({ "motd": "MC Link", "port": 25565 }),
            )
            .await
        {
            eprintln!("[联机] 耦合器广播失败（不影响加入）: {}", e.message);
        }

        mgr.broadcast_context(&PluginContext {
            role: "guest".to_string(),
            game: None,
            room: room_code.as_ref().map(|c| RoomInfo {
                code: c.clone(),
                ..Default::default()
            }),
            peers: vec![],
        });

        let mut evt = connect_event("connected", "已加入房间", Some("guest"));
        evt.room_code = room_code;
        emit_event(&app2, evt);
    });
    Ok(adapter_id)
}

/// 查询当前连接状态（适配器自报）。
#[tauri::command]
pub async fn connect_status(
    manager: tauri::State<'_, Arc<PluginManager>>,
    adapter_id: String,
    game_id: Option<String>,
) -> Result<Value, String> {
    let _ = game_id;
    let provider = manager.provider(&adapter_id).await.map_err(|e| e.message)?;
    provider
        .invoke_checked(Permission::NetListenLocal, adapter_method::STATUS, json!({}))
        .await
        .map_err(|e| e.message)
}

/// 断开当前房间 / 主机，并解除耦合器广播。
#[tauri::command]
pub async fn connect_stop(
    manager: tauri::State<'_, Arc<PluginManager>>,
    app: AppHandle,
    adapter_id: String,
    game_id: Option<String>,
) -> Result<(), String> {
    if let Ok(provider) = manager.provider(&adapter_id).await {
        let _ = provider
            .invoke_checked(Permission::NetListenLocal, adapter_method::HOST_STOP, json!({}))
            .await;
        let _ = provider
            .invoke_checked(Permission::NetListenLocal, adapter_method::LEAVE, json!({}))
            .await;
    }
    let _ = manager
        .invoke(
            PluginKind::Coupler,
            game_id.as_deref(),
            coupler_method::DETACH,
            json!({}),
        )
        .await;
    emit_event(&app, connect_event("stopped", "已断开", None));
    Ok(())
}
