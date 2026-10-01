//! 会话主循环：双向收发、报文分派、心跳与失联判定。

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::WebSocketStream;
use tokio::net::TcpStream;

use crate::plugin::crypto::{Cipher, ReplayWindow, SessionKeys};
use crate::plugin::manifest::LimitsSpec;
use crate::plugin::protocol::{self, core_method, error_code, ErrorInfo, Message};

use super::wire::{now_millis, open_frame, send_message, RateLimiter};
use super::{InboundTx, Pending};

/// 连续密文/序号异常多少次后判定为攻击并断开。
const MAX_FRAME_VIOLATIONS: u32 = 8;

/// 会话 IO 所需的全部状态。
pub(super) struct IoContext {
    pub ws: WebSocketStream<TcpStream>,
    pub sid: String,
    pub plugin_id: String,
    pub keys: SessionKeys,
    pub limits: LimitsSpec,
    pub inbound: InboundTx,
    pub out_rx: mpsc::UnboundedReceiver<Message>,
    pub pending: Arc<Pending>,
    pub closed: Arc<AtomicBool>,
    pub last_seen: Arc<AtomicI64>,
}

/// 会话主循环。返回断开原因。
pub(super) async fn io_loop(ctx: IoContext) -> String {
    let IoContext {
        ws,
        sid,
        plugin_id,
        keys,
        limits,
        inbound,
        mut out_rx,
        pending,
        closed,
        last_seen,
    } = ctx;

    let c2s = Cipher::new(&keys.c2s);
    let s2c = Cipher::new(&keys.s2c);
    let (mut sink, mut stream) = ws.split();

    let mut seq_out: u64 = 0;
    let mut replay = ReplayWindow::new();
    let mut limiter = RateLimiter::new(limits.max_rpc_per_sec);
    let mut violations: u32 = 0;
    // 由入站分支产出的即时响应，统一在本轮 select 结束后按序发出。
    let mut replies: Vec<Message> = Vec::new();

    let mut heartbeat = tokio::time::interval(Duration::from_millis(limits.heartbeat_ms.max(500)));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await; // 跳过立即触发的首次 tick

    loop {
        if closed.load(Ordering::SeqCst) {
            let _ = sink.send(WsMessage::Close(None)).await;
            return "会话被核心关闭".to_string();
        }

        tokio::select! {
            outbound = out_rx.recv() => {
                let Some(msg) = outbound else {
                    return "核心侧关闭会话".to_string();
                };
                if let Err(e) = send_message(&mut sink, &s2c, &sid, &mut seq_out, &msg).await {
                    return e;
                }
            }

            incoming = stream.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        // 双重上限：插件的清单上限不得突破核心的硬上限。
                        let limit = limits.max_frame_bytes.min(protocol::MAX_FRAME_BYTES);
                        if text.len() > limit {
                            return format!(
                                "帧超出大小限制（{} > {}），疑似恶意插件",
                                text.len(),
                                limit
                            );
                        }
                        let msg = match open_frame(&c2s, &sid, &mut replay, text.as_str()) {
                            Ok(m) => m,
                            Err(e) => {
                                violations += 1;
                                if violations >= MAX_FRAME_VIOLATIONS {
                                    return format!("连续非法帧，断开连接: {}", e);
                                }
                                continue;
                            }
                        };
                        // 这个计数是**连续**非法帧（错误文案也这么写），成功解析一次就清零。
                        // 不清零的话，长会话里偶发几次解包失败累计到阈值就会断掉正常连接，
                        // 表现为"莫名掉线"。
                        violations = 0;
                        last_seen.store(now_millis(), Ordering::SeqCst);
                        match dispatch_inbound(
                            msg, &plugin_id, &mut limiter, &inbound, &pending, &mut replies,
                        ).await {
                            Ok(()) => {}
                            Err(reason) => return reason,
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        let _ = sink.send(WsMessage::Pong(payload)).await;
                        last_seen.store(now_millis(), Ordering::SeqCst);
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        last_seen.store(now_millis(), Ordering::SeqCst);
                    }
                    Some(Ok(WsMessage::Close(_))) | None => {
                        return "插件主动断开".to_string();
                    }
                    Some(Ok(_)) => {
                        violations += 1;
                        if violations >= MAX_FRAME_VIOLATIONS {
                            return "收到非法帧类型，断开连接".to_string();
                        }
                    }
                    Some(Err(e)) => return format!("连接错误: {}", e),
                }
            }

            _ = heartbeat.tick() => {
                let idle = now_millis() - last_seen.load(Ordering::SeqCst);
                if idle > limits.idle_timeout_ms as i64 {
                    return format!("心跳超时（{} ms 无响应）", idle);
                }
                let ping = Message::Ping { at: now_millis().max(0) as u64 };
                if let Err(e) = send_message(&mut sink, &s2c, &sid, &mut seq_out, &ping).await {
                    return e;
                }
            }
        }

        // 本轮的即时响应按序发出，保证 s2c 序号单调，插件端不会误判重放。
        for msg in replies.drain(..) {
            if let Err(e) = send_message(&mut sink, &s2c, &sid, &mut seq_out, &msg).await {
                return e;
            }
        }
    }
}

/// 处理一条已解密的入站消息。
async fn dispatch_inbound(
    msg: Message,
    plugin_id: &str,
    limiter: &mut RateLimiter,
    inbound: &InboundTx,
    pending: &Arc<Pending>,
    replies: &mut Vec<Message>,
) -> Result<(), String> {
    if !limiter.try_acquire() {
        return Err("超出速率限制，断开连接".to_string());
    }

    match msg {
        Message::Response {
            id,
            ok,
            result,
            error,
        } => {
            if let Some(tx) = pending.lock().await.remove(&id) {
                let payload = if ok {
                    Ok(result)
                } else {
                    Err(error.unwrap_or_else(|| {
                        ErrorInfo::new(error_code::INTERNAL, "插件未提供错误详情")
                    }))
                };
                let _ = tx.send(payload);
            }
        }
        Message::Event { topic, data } => {
            if topic.len() > 128 || topic.is_empty() {
                return Err("事件主题非法".to_string());
            }
            let _ = inbound.send(super::Inbound::Event {
                plugin_id: plugin_id.to_string(),
                topic,
                data,
            });
        }
        Message::Request { id, method, params } => {
            let (ok, result, error) = handle_core_request(&method, params);
            replies.push(Message::Response {
                id,
                ok,
                result,
                error,
            });
        }
        Message::Ping { at } => {
            replies.push(Message::Pong { at });
        }
        Message::Pong { .. } => {}
        Message::Bye { reason } => {
            return Err(format!("插件请求断开: {}", reason));
        }
    }
    Ok(())
}

/// 核心侧对插件发起的请求只开放最小信息面。
///
/// 这里刻意不提供任何"让核心替插件做事"的方法——核心不是插件的工具箱，
/// 插件想访问资源必须靠自己去申请到的权限，而不是通过 RPC 借核心之手。
pub(super) fn handle_core_request(
    method: &str,
    _params: Value,
) -> (bool, Value, Option<ErrorInfo>) {
    match method {
        core_method::PING => (true, json!({ "at": now_millis() }), None),
        core_method::INFO => (
            true,
            json!({
                "server": super::handshake::server_name(),
                "protocol": protocol::PROTOCOL_VERSION,
            }),
            None,
        ),
        _ => (
            false,
            Value::Null,
            Some(ErrorInfo::not_supported(format!(
                "核心不向插件开放该方法: {}",
                method
            ))),
        ),
    }
}
