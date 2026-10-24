//! 房间编排门面：把「检测器 / 适配器 / 耦合器」串成一次完整联机。
//!
//! # 为什么编排在这里而不是 `domain/room`
//!
//! `domain/room` 只管状态（可纯单测）；这里负责调插件（必须起网关）。
//! 两者分开后，状态机的"非法跃迁""rev 递增""列表过滤"都能在没有 WebView 的情况下验证。
//!
//! # 与旧 `plugin/connect.rs` 的差别
//!
//! 旧实现的命令直接 `spawn` 一段闭包，把进度用全局 `connect-event` 推给前端，
//! 并且**只返回 `adapterId`**——前端只能靠"当前页面上下文"猜是哪个房间。
//! 现在每次调用先建会话、立刻返回 `sessionId` + 快照，进度与状态都带 `sessionId`。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::domain::room::{Phase, Role, RoomMember, RoomRegistry, RoomSession, RoomSnapshot};
use crate::plugin::manager::PluginManager;
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::{required_for, Permission};
use crate::plugin::protocol::{
    adapter_method, coupler_method, detector_method, GameInfo, PluginContext, RoomInfo,
    RoomMember as PluginMember,
};

pub struct RoomMgr {
    registry: Arc<RoomRegistry>,
    plugins: Arc<PluginManager>,
}

impl RoomMgr {
    pub fn new(registry: Arc<RoomRegistry>, plugins: Arc<PluginManager>) -> Arc<Self> {
        Arc::new(Self { registry, plugins })
    }

    pub fn registry(&self) -> &Arc<RoomRegistry> {
        &self.registry
    }

    /// 一次拿齐联机页需要的静态数据：可用适配器（含各自声明的字段）+ 已扫到的本地游戏。
    ///
    /// 旧前端要连发 `connect_adapters` + `connect_local_games` 两个命令再自己拼，
    /// 中间任何一步失败都得在前端处理半成品状态。
    pub async fn prepare(&self, game_id: Option<&str>) -> Value {
        let plan = self.plugins.plan(PluginKind::Adapter, game_id);
        let mut adapters = Vec::new();
        for candidate in &plan.candidates {
            let Some(record) = self.plugins.get(&candidate.plugin_id) else {
                continue;
            };
            // 定向调用**这个**适配器：走 `invoke` 会重新路由，多适配器时全部拿到第一个的字段。
            let init = self
                .plugins
                .invoke_plugin(
                    &candidate.plugin_id,
                    required_for(PluginKind::Adapter, adapter_method::INIT),
                    adapter_method::INIT,
                    json!({}),
                )
                .await
                .ok();
            adapters.push(json!({
                "pluginId": record.manifest.id,
                "name": record.manifest.name,
                "trust": record.trust,
                "ready": record.enabled && record.trust.is_runnable(),
                "hostFields": normalize_fields(init.as_ref().and_then(|v| v.get("host_fields").cloned())),
                "joinFields": normalize_fields(init.as_ref().and_then(|v| v.get("join_fields").cloned())),
            }));
        }
        json!({
            "adapters": adapters,
            "games": self.plugins.local_games(),
        })
    }

    pub fn start_host(
        &self,
        adapter_id: String,
        game_id: Option<String>,
        fields: Value,
        player_name: String,
    ) -> AppResult<RoomSnapshot> {
        self.start(Role::Host, adapter_id, game_id, fields, player_name)
    }

    pub fn join(
        &self,
        adapter_id: String,
        game_id: Option<String>,
        fields: Value,
        player_name: String,
    ) -> AppResult<RoomSnapshot> {
        self.start(Role::Guest, adapter_id, game_id, fields, player_name)
    }

    /// 断开并结束会话。命令会等适配器收尾完成再返回，前端拿到的快照即终态。
    pub async fn leave(&self, session_id: &str) -> AppResult<RoomSnapshot> {
        let handle = self.registry.handle(session_id)?;
        let (adapter_id, game_id) = {
            let session = handle
                .lock()
                .map_err(|_| AppError::internal("会话锁中毒"))?;
            (session.adapter_id.clone(), session.game_id.clone())
        };

        // 先推一次 `stopping`：房间窗口据此显示"正在断开"，而不是点了没反应。
        self.registry.update(session_id, |s| {
            s.phase = Phase::Stopping;
            s.message = "正在断开…".to_string();
        })?;

        if let Ok(provider) = self.plugins.provider(&adapter_id).await {
            let _ = provider
                .invoke_checked(Permission::NetListenLocal, adapter_method::HOST_STOP, json!({}))
                .await;
            let _ = provider
                .invoke_checked(Permission::NetListenLocal, adapter_method::LEAVE, json!({}))
                .await;
        }

        if let Err(e) = self
            .plugins
            .invoke(
                PluginKind::Coupler,
                game_id.as_deref(),
                coupler_method::DETACH,
                json!({}),
            )
            .await
        {
            eprintln!("[联机] 解除耦合器失败（不影响断开）: {}", e.message);
        }

        let snapshot = self.registry.update(session_id, |s| {
            s.phase = Phase::Closed;
            s.message = "已断开".to_string();
            s.members.clear();
        })?;
        self.registry.progress(session_id, "stopped", "已断开");
        Ok(snapshot)
    }

    /// 向适配器要一次实时状态，合并进会话。
    ///
    /// 成员会陆续加入，而适配器只在被问到时才报——房间窗口因此需要低频轮询。
    /// **只在真的变化时才写**：每 3 秒推一次快照会把 `rev` 刷成噪音，
    /// 让"状态是否更新"失去意义，也会让所有窗口做无谓的重渲染。
    pub async fn refresh(&self, session_id: &str) -> AppResult<RoomSnapshot> {
        let snapshot = self.registry.snapshot(session_id)?;
        if !snapshot.phase.is_active() {
            return Ok(snapshot);
        }
        let provider = self
            .plugins
            .provider(&snapshot.adapter_id)
            .await
            .map_err(|e| AppError::new("adapter-failed", e.message))?;
        let value = provider
            .invoke_checked(Permission::NetListenLocal, adapter_method::STATUS, json!({}))
            .await
            .map_err(|e| AppError::new("adapter-failed", e.message))?;

        let members = room_members(&value);
        let port = value
            .get("port")
            .and_then(port_from_value)
            .or(snapshot.port);
        let ready = value
            .get("phase")
            .and_then(|v| v.as_str())
            .map(|p| p.ends_with("-ok"))
            .unwrap_or(false);
        let phase = if ready { Phase::Online } else { snapshot.phase };

        if snapshot.members == members && snapshot.port == port && snapshot.phase == phase {
            return Ok(snapshot);
        }
        self.registry.update(session_id, |s| {
            s.members = members;
            s.port = port;
            s.phase = phase;
        })
    }

    pub fn snapshot(&self, session_id: &str) -> AppResult<RoomSnapshot> {
        self.registry.snapshot(session_id)
    }

    pub fn list(&self) -> Vec<RoomSnapshot> {
        self.registry.list_active()
    }

    fn start(
        &self,
        role: Role,
        adapter_id: String,
        game_id: Option<String>,
        fields: Value,
        player_name: String,
    ) -> AppResult<RoomSnapshot> {
        let adapter_id = adapter_id.trim().to_string();
        if adapter_id.is_empty() {
            return Err(AppError::invalid("未选择联机适配器"));
        }

        // 防双击：同一适配器 + 游戏已有进行中的会话时直接返回它。
        // 否则两个会话会抢同一个本地端口，第二个必然失败，且用户看不出原因。
        if let Some(existing) = self.registry.find_active(&adapter_id, game_id.as_deref()) {
            return Ok(existing);
        }

        let session = RoomSession::new(role, adapter_id.clone(), game_id.clone());
        let snapshot = self.registry.insert(session)?;

        let registry = self.registry.clone();
        let plugins = self.plugins.clone();
        let session_id = snapshot.session_id.clone();

        // 必须显式取运行时句柄再 spawn。
        //
        // `tokio::spawn` 要求**当前线程**处于 Tokio 运行时上下文里；而 Tauri 的**同步**命令
        // 跑在主线程（WebView2 的 IPC 回调栈上），那里没有 reactor。之前在这里直接
        // `tokio::spawn` 的结果是：panic → 该栈帧在 COM 回调里不可 unwind → 进程直接 abort
        // （STATUS_STACK_BUFFER_OVERRUN），用户看到的是"点开始联机，软件消失"。
        //
        // 用 `try_current` 而不是 `spawn`：将来若有人从同步上下文调进来，
        // 这里返回一个明确的错误，而不是把进程带走。
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| AppError::internal("房间启动缺少 Tokio 运行时上下文（命令必须是 async）"))?;
        handle.spawn(async move {
            run_link(
                plugins,
                registry,
                session_id,
                role,
                adapter_id,
                game_id,
                fields,
                player_name,
            )
            .await;
        });

        Ok(snapshot)
    }
}

async fn run_link(
    plugins: Arc<PluginManager>,
    registry: Arc<RoomRegistry>,
    session_id: String,
    role: Role,
    adapter_id: String,
    game_id: Option<String>,
    fields: Value,
    player_name: String,
) {
    let start_message = if role == Role::Host {
        "正在创建房间…"
    } else {
        "正在加入房间…"
    };
    let _ = registry.update(&session_id, |s| {
        s.phase = Phase::Linking;
        s.message = start_message.to_string();
    });
    registry.progress(&session_id, "progress", start_message);

    let provider = match plugins.provider(&adapter_id).await {
        Ok(p) => p,
        Err(e) => {
            fail(&registry, &session_id, &e.message);
            return;
        }
    };

    // 端口只对房主有意义（耦合器要广播本机游戏端口）。成员侧由适配器自己处理。
    let port = if role == Role::Host {
        resolve_port(&plugins, game_id.as_deref(), &fields).await
    } else {
        None
    };

    let mut params = fields.clone();
    if let Value::Object(map) = &mut params {
        map.insert("player_name".to_string(), json!(player_name));
        map.insert("role".to_string(), json!(role.as_str()));
        if let Some(p) = port {
            map.insert("game".to_string(), json!({ "port": p }));
        }
    }

    let method = if role == Role::Host {
        adapter_method::HOST_START
    } else {
        adapter_method::JOIN
    };
    let value = match provider
        .invoke_checked(Permission::NetConnectAny, method, params)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            fail(&registry, &session_id, &e.message);
            return;
        }
    };

    let room_code = value
        .get("room")
        .and_then(|v| v.as_str())
        .or_else(|| fields.get("room_code").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_default();
    let members = room_members(&value);

    // 耦合器广播是 best-effort：它失败只影响"游戏能发现我们"，
    // 房间本身已经建好，不该因此把整次联机判失败。
    if let Err(e) = plugins
        .invoke(
            PluginKind::Coupler,
            game_id.as_deref(),
            coupler_method::ATTACH,
            json!({ "motd": "MC Link", "port": port.unwrap_or(25565) }),
        )
        .await
    {
        eprintln!("[联机] 耦合器广播失败（不影响房间）: {}", e.message);
    }

    plugins.broadcast_context(&PluginContext {
        role: role.as_str().to_string(),
        game: port.map(|p| GameInfo {
            port: Some(p),
            ..Default::default()
        }),
        room: (!room_code.is_empty()).then(|| RoomInfo {
            code: room_code.clone(),
            ..Default::default()
        }),
        members: members
            .iter()
            .map(|m| PluginMember {
                name: m.name.clone(),
                kind: m.kind.clone(),
                machine_id: m.machine_id.clone(),
                is_self: m.is_self,
            })
            .collect(),
        peers: vec![],
    });

    let done_message = if role == Role::Host {
        "房间已创建"
    } else {
        "已加入房间"
    };
    let _ = registry.update(&session_id, |s| {
        s.phase = Phase::Online;
        s.room_code = room_code;
        s.members = members;
        s.port = port;
        s.message = done_message.to_string();
    });
    registry.progress(&session_id, "connected", done_message);
}

fn fail(registry: &RoomRegistry, session_id: &str, message: &str) {
    let _ = registry.update(session_id, |s| {
        s.phase = Phase::Closed;
        s.message = message.to_string();
    });
    registry.progress(session_id, "error", message);
}

async fn resolve_port(
    plugins: &PluginManager,
    game_id: Option<&str>,
    fields: &Value,
) -> Option<u16> {
    if let Some(p) = fields
        .get("game")
        .and_then(|g| g.get("port"))
        .and_then(port_from_value)
    {
        return Some(p);
    }
    match plugins
        .invoke(
            PluginKind::Detector,
            game_id,
            detector_method::SCAN,
            json!({}),
        )
        .await
    {
        Ok((_, value)) => value
            .as_array()
            .and_then(|a| a.first())
            .and_then(|g| g.get("port"))
            .and_then(port_from_value),
        Err(e) => {
            eprintln!("[联机] 本地游戏扫描失败（不影响建房）: {}", e.message);
            None
        }
    }
}

/// 把适配器自报的字段列表规范化成前端契约（camelCase）。
///
/// 外部插件可以按任意命名返回：内置陶瓦用 `autofill_from_invite`，前端契约读的是
/// `autofillFromInvite`。不转换时该字段恒为 `undefined`，前端只能退化成
/// "取第一个字段当邀请码"——适配器一旦声明多个字段就会写错位置。
fn normalize_fields(value: Option<Value>) -> Value {
    let Some(Value::Array(items)) = value else {
        return Value::Array(vec![]);
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

/// 端口越界一律判无效：`as u16` 会把 70000 静默截断成 4464，
/// 于是耦合器往一个没人监听的端口广播，且没有任何报错。
fn port_from_value(value: &Value) -> Option<u16> {
    let p = value.as_u64()?;
    (1..=65535).contains(&p).then_some(p as u16)
}

/// 从适配器返回值里提取成员列表。只认统一约定 `players` 数组，拿不到就是空数组。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_fields_renames_known_snake_case_keys() {
        let raw = json!([{ "key": "code", "autofill_from_invite": true, "max_length": 32 }]);
        let out = normalize_fields(Some(raw));
        assert_eq!(out[0]["autofillFromInvite"], true);
        assert_eq!(out[0]["maxLength"], 32);
        assert!(out[0].get("autofill_from_invite").is_none());
    }

    #[test]
    fn normalize_fields_defaults_to_empty_array() {
        assert_eq!(normalize_fields(None), json!([]));
    }

    #[test]
    fn port_rejects_out_of_range() {
        assert_eq!(port_from_value(&json!(25565)), Some(25565));
        assert_eq!(port_from_value(&json!(70000)), None);
        assert_eq!(port_from_value(&json!(0)), None);
        assert_eq!(port_from_value(&json!("25565")), None);
    }

    #[test]
    fn members_skip_anonymous_entries() {
        let raw = json!({ "players": [
            { "name": "Steve", "machineId": "m1", "self": true },
            { "name": "", "machineId": "" }
        ]});
        let members = room_members(&raw);
        assert_eq!(members.len(), 1);
        assert!(members[0].is_self);
    }
}
