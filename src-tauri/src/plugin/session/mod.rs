//! 单个插件连接的会话：握手、加密收发、RPC 调度、心跳与限流。
//!
//! 一个会话 = 一条 WebSocket 连接 + 一对 AEAD 密钥 + 一份生效权限。
//! 所有状态都集中在会话句柄里，插件断开后其权限与句柄立即失效，
//! 不留"半连接"状态给后续调度误判。
//!
//! # 文件划分
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `mod.rs` | 会话句柄（对外唯一入口）与会话装配 |
//! | `handshake.rs` | WebSocket 升级、准入校验、双向挑战-应答 |
//! | `wire.rs` | 加密信封编解码、限流器、时间工具 |
//! | `io.rs` | 会话主循环、报文分派、心跳与失联判定 |

mod handshake;
mod io;
mod wire;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::WebSocketStream;

use crate::plugin::manifest::LimitsSpec;
use crate::plugin::permission::{Permission, PermissionSet};
use crate::plugin::protocol::{error_code, ErrorInfo, Message};

use serde_json::Value;

pub use handshake::accept_ws;
pub use wire::now_millis;

/// 等待插件回应的 RPC 请求表。
type Pending = AsyncMutex<HashMap<String, oneshot::Sender<Result<Value, ErrorInfo>>>>;

/// WebSocket 出站半边。
type WsSink = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, WsMessage>;

/// 会话向核心上报的事件。
pub enum Inbound {
    /// 插件已完成握手并可用。
    Connected(Arc<SessionHandle>),
    /// 插件断开。
    Disconnected { plugin_id: String, reason: String },
    /// 插件推送的事件。
    Event {
        plugin_id: String,
        topic: String,
        data: Value,
    },
}

/// 向核心发送事件的通道。
pub type InboundTx = mpsc::UnboundedSender<Inbound>;

/// 会话句柄：核心侧调用插件、向插件推送事件的唯一入口。
pub struct SessionHandle {
    pub plugin_id: String,
    pub session_id: String,
    pub permissions: PermissionSet,
    pub limits: LimitsSpec,
    /// 连接建立时间（毫秒时间戳）。
    pub connected_at: i64,
    outbound: mpsc::UnboundedSender<Message>,
    pending: Arc<Pending>,
    closed: Arc<AtomicBool>,
    last_seen: Arc<AtomicI64>,
    rpc_counter: AtomicU64,
}

impl SessionHandle {
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// 最近一次收到入站报文的毫秒时间戳。
    pub fn last_seen(&self) -> i64 {
        self.last_seen.load(Ordering::SeqCst)
    }

    /// 调用插件方法并等待响应。
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, ErrorInfo> {
        if self.is_closed() {
            return Err(ErrorInfo::new(error_code::UNAVAILABLE, "插件连接已断开"));
        }
        let id = format!(
            "{}-{}",
            self.session_id,
            self.rpc_counter.fetch_add(1, Ordering::SeqCst)
        );
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        let msg = Message::Request {
            id: id.clone(),
            method: method.to_string(),
            params,
        };
        if self.outbound.send(msg).is_err() {
            self.pending.lock().await.remove(&id);
            return Err(ErrorInfo::new(error_code::UNAVAILABLE, "插件连接已关闭"));
        }

        let timeout = Duration::from_millis(self.limits.rpc_timeout_ms.max(1_000));
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ErrorInfo::new(error_code::INTERNAL, "插件响应通道异常")),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(ErrorInfo::new(
                    error_code::TIMEOUT,
                    format!("插件调用超时: {}", method),
                ))
            }
        }
    }

    /// 带权限校验的调用。权限判定发生在核心进程内，插件无法绕过。
    pub async fn call_checked(
        &self,
        required: Permission,
        method: &str,
        params: Value,
    ) -> Result<Value, ErrorInfo> {
        self.permissions.require(required)?;
        self.call(method, params).await
    }

    /// 单向事件推送（不需要响应）。
    pub fn notify(&self, topic: &str, data: Value) -> Result<(), ErrorInfo> {
        if self.is_closed() {
            return Err(ErrorInfo::new(error_code::UNAVAILABLE, "插件连接已断开"));
        }
        self.outbound
            .send(Message::Event {
                topic: topic.to_string(),
                data,
            })
            .map_err(|_| ErrorInfo::new(error_code::UNAVAILABLE, "插件连接已关闭"))
    }

    /// 关闭会话：通知插件并终止 IO 循环。
    pub fn close(&self, reason: &str) {
        self.closed.store(true, Ordering::SeqCst);
        let _ = self.outbound.send(Message::Bye {
            reason: reason.to_string(),
        });
    }
}

/// 完成握手并启动会话 IO 循环。
pub async fn serve(
    ws: WebSocketStream<TcpStream>,
    plugin_id: String,
    psk: [u8; 32],
    permissions: PermissionSet,
    limits: LimitsSpec,
    inbound: InboundTx,
) -> Result<Arc<SessionHandle>, String> {
    let (ws, keys, challenge) = handshake::handshake(ws, &plugin_id, &psk, &limits).await?;

    let (out_tx, out_rx) = mpsc::unbounded_channel::<Message>();
    let pending = Arc::new(Pending::new(HashMap::new()));
    let closed = Arc::new(AtomicBool::new(false));
    let last_seen = Arc::new(AtomicI64::new(now_millis()));

    let handle = Arc::new(SessionHandle {
        plugin_id: plugin_id.clone(),
        session_id: challenge.session_id.clone(),
        permissions,
        limits: limits.clone(),
        connected_at: now_millis(),
        outbound: out_tx,
        pending: pending.clone(),
        closed: closed.clone(),
        last_seen: last_seen.clone(),
        rpc_counter: AtomicU64::new(0),
    });

    if inbound.send(Inbound::Connected(handle.clone())).is_err() {
        return Err("核心事件通道已关闭".to_string());
    }

    let sid = challenge.session_id.clone();
    let pid = plugin_id.clone();
    tokio::spawn(async move {
        let ctx = io::IoContext {
            ws,
            sid,
            plugin_id: pid.clone(),
            keys,
            limits,
            inbound: inbound.clone(),
            out_rx,
            pending: pending.clone(),
            closed: closed.clone(),
            last_seen,
        };
        let reason = io::io_loop(ctx).await;
        closed.store(true, Ordering::SeqCst);
        pending.lock().await.clear();
        let _ = inbound.send(Inbound::Disconnected {
            plugin_id: pid,
            reason,
        });
    });

    Ok(handle)
}
