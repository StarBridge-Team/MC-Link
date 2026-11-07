//! 插件间通信中枢：把 [`PeerOp`] 转换成"查会话 → 校验目标方权限 → 投递"。
//!
//! # 与其它模块的边界
//!
//! - 会话侧（`session/io.rs`）只做**发起方**的权限判定与参数校验，然后经
//!   [`PeerHandle`] 把操作发到这里；
//! - 这里做**目标方**的权限判定（`peer_message` 是双向闸门：没声明的插件
//!   收不到任何 peer 消息）与实际投递；
//! - 能力方法（`adapter.*` 等）与 peer 信道**完全不交叉**：目标插件收到的
//!   固定是 `peer.call` / `peer.event`，方法名只是载荷里的字符串（见
//!   [`crate::plugin::peer`] 的命名空间隔离说明）。
//!
//! # 为什么没有消息队列/持久化
//!
//! peer 消息是**即时编排指令**，不是可靠消息总线：目标不在线就失败（除非
//! 显式 `launch: true` 请求按需拉起）。要"离线也能收"的插件应该自己落盘
//! （`fs_plugin_data`），而不是让核心替所有插件无限缓存——那会变成一个
//! 无界增长的攻击面。

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{mpsc, Semaphore};

use crate::plugin::capability::Provider;
use crate::plugin::peer::{next_trace, CallRequest, PeerOp, PEER_CALL, PEER_EVENT};
use crate::plugin::permission::Permission;
use crate::plugin::protocol::{error_code, ErrorInfo};

use super::PluginManager;

/// peer 信道全局并发上限。
///
/// 每次 `core.peer.call` 在等待目标回执期间都占一个许可。没有上限时，
/// 一枚插件可以对不存在的目标发起成千上万个调用（每个都等到超时），
/// 把运行时的任务数吃满。128 已远超"几枚插件互调"的正常用量。
const MAX_INFLIGHT_PEER_CALLS: usize = 128;

impl PluginManager {
    /// 启动 peer 中枢，返回会话侧使用的通道句柄。
    ///
    /// 中枢任务会运行到通道关闭（即管理器丢弃所有 `PeerHandle`）为止。
    pub(super) fn start_peer_hub(self: &Arc<Self>) -> crate::plugin::peer::PeerHandle {
        let (handle, rx) = crate::plugin::peer::PeerHandle::channel();
        let inner = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            consume_peer_ops(inner, rx).await;
        });
        handle
    }

    /// `core.peer.list` 的同步实现（供中枢与测试共用）。
    ///
    /// 列的是**可调用**的插件：已启用、未被拉黑、声明并生效了 `peer_message`，
    /// 且不是调用者自己。
    pub(super) fn peer_list(&self, from: &str) -> Vec<crate::plugin::protocol::PeerPluginInfo> {
        list_peers(&self.inner, from)
    }

    /// 按需拉起目标插件（`core.peer.call { launch: true }`）。
    async fn peer_launch(&self, plugin_id: &str) -> Result<(), ErrorInfo> {
        self.provider(plugin_id).await.map(|_| ()).map_err(|mut e| {
            // "未注册/未启用"对调用方来说就是"目标不可用"，统一语义
            e.code = error_code::UNAVAILABLE.to_string();
            e
        })
    }
}

/// 消费 `PeerOp` 直到通道关闭。
async fn consume_peer_ops(inner: Arc<super::Inner>, mut rx: mpsc::UnboundedReceiver<PeerOp>) {
    let inflight = Arc::new(Semaphore::new(MAX_INFLIGHT_PEER_CALLS));
    while let Some(op) = rx.recv().await {
        match op {
            PeerOp::List { from, reply } => {
                let _ = reply.send(list_peers(&inner, &from));
            }
            PeerOp::Publish {
                from,
                to,
                topic,
                data,
            } => {
                publish(&inner, &from, to.as_deref(), &topic, data);
            }
            PeerOp::Call {
                from,
                request,
                reply,
            } => {
                // 每次调用一个任务 + 一个许可：等待目标回执不阻塞中枢，
                // 但总量被封顶。拿不到许可说明全局在途调用已满，立即失败
                // 而不是排队（排队会让"等超时"变成"排队 + 等超时"）。
                let Ok(permit) = inflight.clone().try_acquire_owned() else {
                    let _ = reply.send(Err(ErrorInfo::new(
                        error_code::RATE_LIMITED,
                        "peer 调用并发已达上限，请稍后重试",
                    )));
                    continue;
                };
                // `inner` 在外层循环里还要继续用，这里必须克隆进任务
                let inner = inner.clone();
                tauri::async_runtime::spawn(async move {
                    let _permit = permit;
                    let result = relay_call(&inner, &from, &request).await;
                    let _ = reply.send(result);
                });
            }
        }
    }
}

/// `core.peer.list`：同机**可调用**的其它插件。
///
/// 过滤条件与投递路径共用 [`allowed_targets`]——列出来的就能调、调不到的不列，
/// 避免"列表里有、一调就 forbidden"这种自相矛盾的接口。
fn list_peers(inner: &Arc<super::Inner>, from: &str) -> Vec<crate::plugin::protocol::PeerPluginInfo> {
    let allowed = allowed_targets(inner, from, None);
    let records = inner
        .registry
        .read()
        .ok()
        .map(|r| r.list().into_iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let mut out = Vec::new();
    for record in records {
        if !allowed.contains(record.id()) {
            continue;
        }
        out.push(crate::plugin::protocol::PeerPluginInfo {
            plugin_id: record.manifest.id.clone(),
            kind: record.manifest.kind.as_str().to_string(),
            games: record.manifest.games.clone(),
            endpoints: Vec::new(),
            version: record.manifest.version.clone(),
            alive: inner
                .providers
                .read()
                .ok()
                .and_then(|m| m.get(record.manifest.id.as_str()).map(|p| p.is_available()))
                .unwrap_or(false),
        });
    }
    out
}

/// `core.peer.publish`：定向投递或广播。没有回执，失败只打日志。
fn publish(
    inner: &Arc<super::Inner>,
    from: &str,
    to: Option<&str>,
    topic: &str,
    data: Value,
) {
    let sessions = collect_publish_targets(inner, from, to);
    if sessions.is_empty() {
        // 常见原因不是"崩了"，而是目标没声明 `peer_message` 或没连上——
        // 两种都只影响这一次投递，不该升级成错误。
        eprintln!(
            "[插件] peer.publish 没有投递对象（from={from} to={to:?} topic={topic}）"
        );
        return;
    }
    let payload = json!({ "from": from, "topic": topic, "data": data });
    for (id, session) in sessions {
        // 目标侧收到的固定是 `peer.event` 事件，主题与方法名都在载荷里
        if let Err(e) = session.notify(PEER_EVENT, payload.clone()) {
            eprintln!("[插件] peer.publish 投递失败（to={id}）: {}", e.message);
        }
    }
}

/// 广播/定向的目标解析。
///
/// 与 `relay_call` 同口径：**权限以注册表为准**（它是 `set_grants` 之后最新的一份），
/// 只有同时满足"已启用 + 未被拉黑 + 生效了 `peer_message` + 已连接"的插件才会收到。
fn collect_publish_targets(
    inner: &Arc<super::Inner>,
    from: &str,
    to: Option<&str>,
) -> Vec<(String, Arc<crate::plugin::session::SessionHandle>)> {
    let allowed = allowed_targets(inner, from, to);
    let mut out = Vec::new();
    if let Ok(providers) = inner.providers.read() {
        for (id, provider) in providers.iter() {
            let Provider::Remote(session) = provider else { continue };
            if session.is_closed() || !allowed.contains(id) {
                continue;
            }
            out.push((id.clone(), session.clone()));
        }
    }
    out
}

/// 注册表口径的"可投递目标集合"（含 self 排除与定向过滤）。
fn allowed_targets(
    inner: &Arc<super::Inner>,
    from: &str,
    to: Option<&str>,
) -> std::collections::HashSet<String> {
    let mut allowed = std::collections::HashSet::new();
    let Ok(registry) = inner.registry.read() else {
        return allowed;
    };
    for record in registry.list() {
        let id = record.id();
        if id == from || !record.enabled || !record.trust.is_runnable() {
            continue;
        }
        if let Some(target) = to {
            if id != target {
                continue;
            }
        }
        if !record.permissions.contains(Permission::PeerMessage) {
            continue;
        }
        allowed.insert(id.to_string());
    }
    allowed
}

/// `core.peer.call` 的中转主体。
///
/// `pub(super)` 是为了让 `manager::tests` 能直接验证目标方闸门——这条闸门是
/// 权限模型的双向接收侧，退化了就等于任何插件都能往别的插件里塞消息。
pub(super) async fn relay_call(
    inner: &Arc<super::Inner>,
    from: &str,
    request: &CallRequest,
) -> Result<Value, ErrorInfo> {
    // 目标方权限闸门（发起方的判定在 session/io.rs 已做）
    {
        let record = inner
            .registry
            .read()
            .ok()
            .and_then(|r| r.get(&request.to).cloned());
        let Some(record) = record else {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("目标插件未注册: {}", request.to),
            ));
        };
        if !record.enabled || !record.trust.is_runnable() {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("目标插件未启用或已被拉黑: {}", request.to),
            ));
        }
        if !record.permissions.contains(Permission::PeerMessage) {
            return Err(ErrorInfo::new(
                error_code::FORBIDDEN,
                format!("目标插件 {} 未声明 peer_message，不接收插件间消息", request.to),
            ));
        }
    }

    // 目标在线直接投递；不在线且显式请求拉起时先拉起（拉起即等待握手完成）
    if !provider_available(inner, &request.to) {
        if !request.launch {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("目标插件 {} 当前未连接（可传 launch: true 请求按需拉起）", request.to),
            ));
        }
        // 复用管理器的拉起逻辑：含串行锁、握手等待与失败回收
        let manager = PluginManager { inner: inner.clone() };
        manager.peer_launch(&request.to).await?;
    }

    // 重新取会话（拉起完成后才存在）
    let session = remote_session(inner, &request.to).ok_or_else(|| {
        ErrorInfo::new(
            error_code::UNAVAILABLE,
            format!("目标插件 {} 会话不可用", request.to),
        )
    })?;

    let trace = next_trace();
    let payload = request.wire_payload(from, &trace);
    eprintln!(
        "[插件] peer.call 中转：{} → {} method={} depth={} trace={}",
        from, request.to, request.method, request.depth, trace
    );
    // 目标侧收到的固定是 `peer.call`，发起方要调的方法名在载荷里。
    //
    // 超时取**发起方**声明的值：目标自己的 `rpc_timeout_ms` 只是它的上限，
    // 不该让 A 的 3 秒调用被 B 的 30 秒配置拖住。
    let timeout = request.timeout();
    match tokio::time::timeout(timeout, session.call(PEER_CALL, payload)).await {
        Ok(result) => result,
        Err(_) => Err(ErrorInfo::new(
            error_code::TIMEOUT,
            format!("peer 调用超时（{} ms）", timeout.as_millis()),
        )),
    }
}

fn provider_available(inner: &Arc<super::Inner>, plugin_id: &str) -> bool {
    inner
        .providers
        .read()
        .ok()
        .map(|m| m.get(plugin_id).map(|p| p.is_available()).unwrap_or(false))
        .unwrap_or(false)
}

fn remote_session(
    inner: &Arc<super::Inner>,
    plugin_id: &str,
) -> Option<Arc<crate::plugin::session::SessionHandle>> {
    inner
        .providers
        .read()
        .ok()
        .and_then(|m| m.get(plugin_id).cloned())
        .and_then(|p| match p {
            Provider::Remote(s) if !s.is_closed() => Some(s),
            _ => None,
        })
}
