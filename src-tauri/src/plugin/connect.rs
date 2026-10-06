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
    adapter_method, coupler_method, detector_method, GameInfo, PluginContext, RoomInfo, RoomMember,
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

/// 把适配器自报的字段列表规范化成前端契约（camelCase）。
///
/// # 为什么必须在这一层规范化
///
/// 适配器（尤其外部插件）可以按任意命名返回 `join_fields`：内置陶瓦用的是
/// snake_case 的 `autofill_from_invite`，而前端契约（`typesConnect.ts`）读的是
/// `autofillFromInvite`。不做转换时该字段恒为 `undefined`，前端只能退化为"取第一个
/// 字段当邀请码"，一旦适配器声明多个字段就会写错位置。
///
/// 因此把已知的 snake_case 元数据统一改名，其余未知字段原样保留（前向兼容）。
fn normalize_fields(value: Value) -> Value {
    let Value::Array(items) = value else {
        return value;
    };
    Value::Array(
        items
            .into_iter()
            .map(|item| {
                let Value::Object(mut map) = item else {
                    return item;
                };
                for (from, to) in [
                    ("autofill_from_invite", "autofillFromInvite"),
                    ("max_length", "maxLength"),
                ] {
                    if let Some(v) = map.remove(from) {
                        map.insert(to.to_string(), v);
                    }
                }
                Value::Object(map)
            })
            .collect(),
    )
}

/// 把前端传来的端口转成合法的 `u16`。
///
/// 客户端可以把 `port` 传成任意 u64（例如 70000）。直接 `as u16` 会静默截断成
/// 4464，房间广播/耦合器就会用错端口且无任何报错。越界一律判为无效并回退到扫描结果。
fn port_from_value(value: &Value) -> Option<u16> {
    let p = value.as_u64()?;
    (1..=65535).contains(&p).then_some(p as u16)
}

/// 从适配器返回值里提取房间成员。
///
/// 适配器实现各不相同，这里只认统一约定：`players` 数组（内置陶瓦适配器已把它的
/// `profiles` 规范化成这个形状）。拿不到就返回空数组——**不臆造成员**。
fn room_members(value: &Value) -> Vec<RoomMember> {
    value
        .get("players")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|p| {
                    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let machine_id = p.get("machineId").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() && machine_id.is_empty() {
                        return None;
                    }
                    Some(RoomMember {
                        name: name.to_string(),
                        kind: p.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        machine_id: machine_id.to_string(),
                        is_self: p.get("self").and_then(|v| v.as_bool()).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 列出可用于联机的适配器，并带上各适配器自声明的 `host_fields` / `join_fields`。
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
        // 让适配器自报房主/访客两侧字段；拿不到就当没有（前端按"无需字段"处理）。
        let init = manager
            .invoke(
                PluginKind::Adapter,
                game_id.as_deref(),
                adapter_method::INIT,
                json!({}),
            )
            .await
            .ok()
            .map(|(_, v)| v);
        let host_fields = normalize_fields(
            init.as_ref()
                .and_then(|v| v.get("host_fields").cloned())
                .unwrap_or(Value::Array(vec![])),
        );
        let join_fields = normalize_fields(
            init.as_ref()
                .and_then(|v| v.get("join_fields").cloned())
                .unwrap_or(Value::Array(vec![])),
        );
        adapters.push(json!({
            "pluginId": record.manifest.id,
            "name": record.manifest.name,
            "trust": record.trust,
            "ready": record.enabled && record.trust.is_runnable(),
            "hostFields": host_fields,
            "joinFields": join_fields,
        }));
    }
    Ok(json!({ "adapters": adapters }))
}

/// 扫描本机游戏实例（经检测器插件），返回发现到的游戏数组（可能为空）。
///
/// 房主模式据此展示「扫到的所有游戏」卡片；空数组即「没扫到游戏」，前端呈现空状态。
#[tauri::command]
pub async fn connect_scan(
    manager: tauri::State<'_, Arc<PluginManager>>,
    game_id: Option<String>,
) -> Result<Value, String> {
    let res = manager
        .invoke(
            PluginKind::Detector,
            game_id.as_deref(),
            detector_method::SCAN,
            json!({}),
        )
        .await
        .map_err(|e| e.message)?;
    // 统一成数组返回，便于前端 v-for。
    let arr = match res.1 {
        Value::Array(a) => a,
        other => vec![other],
    };
    Ok(json!(arr))
}

/// 取最近一次检测器扫描到的游戏，**不重新扫描**。
///
/// # 为什么需要它
///
/// 首页原本纯粹靠 `local-game-found` 事件驱动（扫描在应用打开时执行一次）。事件是
/// 一次性广播，**没有补发**：只要监听器比事件晚一步（页面重载、启动竞态、扫描失败一次），
/// 首页就会永远停在"正在寻找本地游戏…"，只能靠手动点重扫自救。
///
/// 这个命令让首页能在挂载时主动拉一次当前状态兜底。启用并缓存扫描结果的是
/// [`PluginManager::scan_local_games`]，事件与这里读的是同一份数据，不会出现两套口径。
#[tauri::command]
pub async fn connect_local_games(
    manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<Value, String> {
    Ok(json!(manager.local_games()))
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

        // 1) 端口：优先用房主所选游戏的端口（由前端经 fields.game.port 传入），
        //    缺失时再 best-effort 扫描一次。用于游戏上下文与耦合器广播。
        let port = if let Some(p) = fields
            .get("game")
            .and_then(|g| g.get("port"))
            .and_then(port_from_value)
        {
            Some(p)
        } else {
            match mgr
                .invoke(
                    PluginKind::Detector,
                    game_id.as_deref(),
                    detector_method::SCAN,
                    json!({}),
                )
                .await
            {
                Ok((_, v)) => v
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|g| g.get("port"))
                    .and_then(port_from_value),
                Err(e) => {
                    eprintln!("[联机] 本地游戏扫描失败（不影响建房间）: {}", e.message);
                    None
                }
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
        // 适配器自报的成员列表（陶瓦为 `profiles`）。不同适配器字段形态不一，
        // 用 `room_members` 统一提取，拿不到就是空数组。
        let members = room_members(&value);

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
            members,
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
        // 接住返回值：适配器会把规范化后的成员列表放在里面（见 `room_members`）。
        let value = match provider
            .invoke_checked(Permission::NetConnectAny, adapter_method::JOIN, params)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                emit_event(&app2, connect_event("error", &e.message, Some("guest")));
                return;
            }
        };
        let room_code = value
            .get("room")
            .and_then(|v| v.as_str())
            .or_else(|| fields.get("room_code").and_then(|v| v.as_str()))
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

        // 与房主侧对齐：把适配器自报的成员一并广播。
        let members = room_members(&value);
        mgr.broadcast_context(&PluginContext {
            role: "guest".to_string(),
            game: None,
            room: room_code.as_ref().map(|c| RoomInfo {
                code: c.clone(),
                ..Default::default()
            }),
            members,
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
