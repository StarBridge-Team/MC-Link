//! 房间编排门面：把「检测器 / 适配器 / 耦合器」串成一次完整联机。
//!
//! `domain/room` 只管状态（可纯单测）；这里负责调插件（必须起网关）：分开后，状态机的
//! "非法跃迁""rev 递增""列表过滤"都能在没有 WebView 的情况下验证。
//!
//! # 终态不可回退
//!
//! 轮询与建链任务随时可能比用户动作慢，**任何**后台写回都要先在会话锁内确认会话还活着
//! （见 `is_terminal`），否则会出现"用户已断开、房间又显示在线"这种没法收场的状态。

mod helpers;
mod prepare;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::domain::room::{Phase, Role, RoomRegistry, RoomSession, RoomSnapshot};
use crate::plugin::manager::PluginManager;
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::Permission;
use crate::plugin::protocol::{
    adapter_method, coupler_method, detector_method, GameInfo, PluginContext, RoomInfo,
    RoomMember as PluginMember,
};

use self::helpers::{port_from_value, room_members};

/// 终态：`Closed`（已结束）与 `Stopping`（正在结束，下一步就是 `Closed`）。
///
/// **不能拿 `Phase::is_active()` 代替**：它只排除 `Closed`，`Stopping` 仍算"进行中"，
/// 但 `Stopping` 的会话同样不该被后台写回——`leave()` 正在收尾，这时写回 `Online` 就"死而不僵"。
fn is_terminal(phase: Phase) -> bool {
    matches!(phase, Phase::Closed | Phase::Stopping)
}

/// 会话是否已经结束，或已被从注册表摘掉（读不到也算结束）。
fn is_gone(registry: &RoomRegistry, session_id: &str) -> bool {
    registry
        .snapshot(session_id)
        .map(|s| is_terminal(s.phase))
        .unwrap_or(true)
}

pub struct RoomMgr {
    registry: Arc<RoomRegistry>,
    plugins: Arc<PluginManager>,
    /// 建链任务句柄：`leave()` 靠它把还在跑的 `run_link` 掐掉。
    link_tasks: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl RoomMgr {
    pub fn new(registry: Arc<RoomRegistry>, plugins: Arc<PluginManager>) -> Arc<Self> {
        Arc::new(Self {
            registry,
            plugins,
            link_tasks: Mutex::new(HashMap::new()),
        })
    }

    pub fn registry(&self) -> &Arc<RoomRegistry> {
        &self.registry
    }

    /// 一次拿齐联机页需要的静态数据：可用适配器（含各自声明的字段）+ 已扫到的本地游戏。
    /// 旧前端要连发两个命令再自己拼，中间任何一步失败都得在前端处理半成品状态。
    pub async fn prepare(&self, game_id: Option<&str>) -> Value {
        prepare::prepare(&self.plugins, game_id).await
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
            let session = handle.lock().map_err(|_| AppError::internal("会话锁中毒"))?;
            (session.adapter_id.clone(), session.game_id.clone())
        };

        // 先推一次 `stopping`：房间窗口据此显示"正在断开"，而不是点了没反应。
        self.registry.update(session_id, |s| {
            s.phase = Phase::Stopping;
            s.message = "正在断开…".to_string();
        })?;

        // 掐掉还在跑的建链任务：否则它可能在下面"停适配器"之后又把适配器拉起来。
        self.abort_link_task(session_id);

        if let Ok(provider) = self.plugins.provider(&adapter_id).await {
            for method in [adapter_method::HOST_STOP, adapter_method::LEAVE] {
                let _ = provider
                    .invoke_checked(Permission::NetListenLocal, method, json!({}))
                    .await;
            }
        }

        let detach = self
            .plugins
            .invoke(PluginKind::Coupler, game_id.as_deref(), coupler_method::DETACH, json!({}))
            .await;
        if let Err(e) = detach {
            eprintln!("[联机] 解除耦合器失败（不影响断开）: {}", e.message);
        }

        let snapshot = self.registry.update(session_id, |s| {
            s.phase = Phase::Closed;
            s.message = "已断开".to_string();
            s.members.clear();
        })?;
        self.registry.progress(session_id, "stopped", "已断开");
        // 终态已经广播，会话不会再被查询：从注册表摘掉，别让历史房间永久驻留。
        self.registry.remove(session_id);
        Ok(snapshot)
    }

    /// 向适配器要一次实时状态，合并进会话。成员会陆续加入，而适配器只在被问到时才报，
    /// 所以房间窗口需要低频轮询。
    ///
    /// **只在真的变化时才写**：否则每 3 秒推一次快照会把 `rev` 刷成噪音，"状态是否更新"
    /// 会失去意义，所有窗口也要做无谓的重渲染。
    pub async fn refresh(&self, session_id: &str) -> AppResult<RoomSnapshot> {
        // 入口短路：会话已结束/正在结束就别再去问适配器了。
        let snapshot = self.registry.snapshot(session_id)?;
        if is_terminal(snapshot.phase) {
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
        let reported_port = value.get("port").and_then(port_from_value);
        let ready = value
            .get("phase")
            .and_then(|v| v.as_str())
            .map(|p| p.ends_with("-ok"))
            .unwrap_or(false);

        // 判定与写入都在会话锁内完成：拿锁外的旧快照写回，会把已断开的房间"复活"成在线。
        self.registry.update_if(session_id, |s| {
            // 终态守卫：已结束/正在结束的会话，轮询无权写回。
            if is_terminal(s.phase) {
                return false;
            }
            let port = reported_port.or(s.port);
            // 没 ready 时**保持当前 phase**：旧快照里的 phase 是离开前的值，写回去就是回退。
            let next_phase = if ready { Phase::Online } else { s.phase };
            // 比较也必须在锁内做，否则会拿旧快照误判"没变化"。
            if s.members == members && s.port == port && s.phase == next_phase {
                return false;
            }
            s.members = members;
            s.port = port;
            s.phase = next_phase;
            true
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

        // 防双击：同一适配器 + 游戏已有进行中的会话时直接返回它，否则两个会话会抢同一个
        // 本地端口，第二个必然失败，且用户看不出原因。
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
        // 跑在主线程（WebView2 的 IPC 回调栈上），那里没有 reactor。此前直接 `tokio::spawn`
        // 的结果是 panic → 该栈帧在 COM 回调里不可 unwind → 进程直接 abort，用户看到的是
        // "点开始联机，软件消失"。改用 `try_current`：从同步上下文调进来时返回明确错误，
        // 而不是把进程带走。
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| AppError::internal("房间启动缺少 Tokio 运行时上下文（命令必须是 async）"))?;
        let task_id = session_id.clone();
        let join = handle.spawn(async move {
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
        // 留住句柄：`leave()` 要靠它 abort 建链任务（句柄一丢就再也掐不掉）。
        self.track_link_task(&task_id, join);

        Ok(snapshot)
    }

    /// 掐掉某个会话的建链任务（若还在跑）。
    fn abort_link_task(&self, session_id: &str) {
        if let Ok(mut map) = self.link_tasks.lock() {
            if let Some(handle) = map.remove(session_id) {
                handle.abort();
            }
        }
    }

    /// 记录建链任务句柄，顺手丢掉已经跑完的旧句柄。
    fn track_link_task(&self, session_id: &str, handle: tokio::task::JoinHandle<()>) {
        if let Ok(mut map) = self.link_tasks.lock() {
            map.retain(|_, h| !h.is_finished());
            map.insert(session_id.to_string(), handle);
        }
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
    let start_message = match role {
        Role::Host => "正在创建房间…",
        Role::Guest => "正在加入房间…",
    };
    // 终态守卫：会话已经被断开，就别把它拽回"建链中"。
    let mut linking = false;
    let _ = registry.update_if(&session_id, |s| {
        if is_terminal(s.phase) {
            return false;
        }
        s.phase = Phase::Linking;
        s.message = start_message.to_string();
        linking = true;
        true
    });
    if !linking {
        return;
    }
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

    // 拉适配器之前再确认一次：用户可能在这几秒里就点了"断开"。
    if is_gone(&registry, &session_id) {
        return;
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

    // 适配器可能已经建好房间，但用户在这期间断开了：这时不该再广播、也不该写回在线。
    if is_gone(&registry, &session_id) {
        return;
    }

    let room_code = value
        .get("room")
        .and_then(|v| v.as_str())
        .or_else(|| fields.get("room_code").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_default();
    let members = room_members(&value);

    // 局域网广播的端口必须是「玩家该连的那个端口」：
    //
    // - 房主：自己的游戏端口（`port`）；
    // - 访客：引擎把房主端口接出来后**本机**的端口，由适配器回报（`value.port`）。
    //
    // 早先这里一律用 `port.unwrap_or(25565)`，于是访客接出端口不是 25565 时，
    // 局域网列表里的 "MC Link" 指向一个本机没人监听的端口——看得见、点不进去。
    let entry_port = value
        .get("port")
        .and_then(port_from_value)
        .or(port)
        .unwrap_or(25565);

    // 耦合器广播是 best-effort：它失败只影响"游戏能发现我们"，
    // 房间本身已经建好，不该因此把整次联机判失败。
    let attach = plugins
        .invoke(
            PluginKind::Coupler,
            game_id.as_deref(),
            coupler_method::ATTACH,
            json!({ "motd": crate::plugin::builtin::lan::MOTD, "port": entry_port }),
        )
        .await;
    if let Err(e) = attach {
        eprintln!("[联机] 耦合器广播失败（不影响房间）: {}", e.message);
    }

    plugins.broadcast_context(&PluginContext {
        role: role.as_str().to_string(),
        game: Some(GameInfo {
            port: Some(entry_port),
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

    let done_message = match role {
        Role::Host => "房间已创建",
        Role::Guest => "已加入房间",
    };
    // 收尾守卫：会话已经结束/正在结束，就别再写回"在线"。
    let mut linked = false;
    let _ = registry.update_if(&session_id, |s| {
        if is_terminal(s.phase) {
            return false;
        }
        s.phase = Phase::Online;
        s.room_code = room_code;
        s.members = members;
        // 会话里的 `port` 是"玩家该连的本机端口"，与会话角色无关：
        // 房主是自己的游戏端口，访客是引擎接出来的本机端口（上面算的 `entry_port`）。
        // 访客侧若这里留 `None`，房间窗口要等到第一次 `room_refresh` 才显示端口。
        s.port = Some(entry_port);
        s.message = done_message.to_string();
        linked = true;
        true
    });
    if linked {
        registry.progress(&session_id, "connected", done_message);
    }
}

fn fail(registry: &RoomRegistry, session_id: &str, message: &str) {
    // 已经在断开中就别覆盖"正在断开"的文案。
    let mut failed = false;
    let _ = registry.update_if(session_id, |s| {
        if is_terminal(s.phase) {
            return false;
        }
        s.phase = Phase::Closed;
        s.message = message.to_string();
        failed = true;
        true
    });
    if failed {
        registry.progress(session_id, "error", message);
    }
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
        .invoke(PluginKind::Detector, game_id, detector_method::SCAN, json!({}))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_covers_stopping_and_closed() {
        assert!(is_terminal(Phase::Closed) && is_terminal(Phase::Stopping));
        assert!(!is_terminal(Phase::Starting));
        assert!(!is_terminal(Phase::Linking));
        assert!(!is_terminal(Phase::Online));
    }
}
