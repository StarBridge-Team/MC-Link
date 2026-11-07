//! 会话主循环：收发、分发、心跳应答。
//!
//! 单独成文件的原因：它同时碰"网络帧"与"处理器注册表"，是 SDK 里最长的一段；
//! 与类型定义混在一起会让 `lib.rs` 既读不完也改不动。

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::wire::{self, Handshake, Message, SecureFrame};
use crate::{
    Context, Error, HandlerResult, PeerEvent, Plugin, PluginContext, Registry,
};

impl Plugin {    /// 处理入站消息，直到连接断开。
    pub async fn run(mut self) -> Result<(), Error> {
        let mut out_rx = self
            .out_rx
            .take()
            .ok_or_else(|| Error::internal("插件已经运行过"))?;

        let (sink, mut stream) = self.ws.split();
        let keys = self.keys.clone();
        let sid_writer = self.sid.clone();
        let sid_read = self.sid.clone();
        let writer = tokio::spawn(async move {
            let mut sink = sink;
            let mut seq = 0u64;
            while let Some(msg) = out_rx.recv().await {
                let frame = match wire::seal(&keys.c2s, &sid_writer, seq, &msg) {
                    Ok(frame) => frame,
                    Err(e) => {
                        eprintln!("[插件] 加密失败: {e}");
                        break;
                    }
                };
                let text = match serde_json::to_string(&frame) {
                    Ok(text) => text,
                    Err(e) => {
                        eprintln!("[插件] 序列化失败: {e}");
                        break;
                    }
                };
                if sink.send(WsMessage::text(text)).await.is_err() {
                    break;
                }
                seq += 1;
            }
            let _ = sink.close().await;
        });

        let registry = Arc::new(self.registry);
        let context = self.context.clone();
        let pending = self.pending.clone();
        let rpc = self.rpc.clone();
        let out = self.out.clone();
        let plugin_id: Arc<str> = Arc::from(self.plugin_id.as_str());
        let s2c = self.keys.s2c;
        let mut last_seq: i64 = -1;

        let result = loop {
            let incoming = match stream.next().await {
                Some(Ok(msg)) => msg,
                Some(Err(e)) => break Err(Error::unavailable(format!("连接错误: {e}"))),
                None => break Ok(()),
            };
            match incoming {
                WsMessage::Text(text) => {
                    let frame: SecureFrame = match serde_json::from_str(&text) {
                        Ok(frame) => frame,
                        Err(e) => break Err(Error::bad_request(format!("信封解析失败: {e}"))),
                    };
                    if (frame.seq as i64) <= last_seq {
                        break Err(Error::new("unauthorized", "检测到重放或乱序帧"));
                    }
                    last_seq = frame.seq as i64;
                    let msg = match wire::open(&s2c, &sid_read, &frame) {
                        Ok(msg) => msg,
                        Err(e) => break Err(e),
                    };
                    match msg {
                        Message::Request { id, method, params } => {
                            dispatch_request(
                                &registry,
                                &context,
                                &pending,
                                &rpc,
                                &out,
                                plugin_id.clone(),
                                id,
                                method,
                                params,
                            );
                        }
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
                                    let info = error.unwrap_or(wire::ErrorInfo {
                                        code: "internal".into(),
                                        message: "核心未提供错误详情".into(),
                                    });
                                    Err(Error::new(info.code, info.message))
                                };
                                let _ = tx.send(payload);
                            }
                        }
                        Message::Event { topic, data } => {
                            handle_event(&registry, &context, &topic, data);
                        }
                        Message::Ping { at } => {
                            let _ = out.send(Message::Pong { at });
                        }
                        Message::Pong { .. } => {}
                        Message::Bye { reason } => {
                            break Err(Error::unavailable(format!("核心断开: {reason}")));
                        }
                    }
                }
                WsMessage::Ping(_) => {
                    // 核心的心跳是**会话层**的 JSON `ping`（上面已处理）；
                    // WS 层的 ping 由 tungstenite 在读到下一帧时自动回 pong，
                    // 这里不需要（也不该）直接写 sink——它在发送任务手里。
                }
                WsMessage::Close(_) => break Ok(()),
                _ => {}
            }
        };

        writer.abort();
        result
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_request(
    registry: &Arc<Registry>,
    context: &Arc<RwLock<PluginContext>>,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<HandlerResult>>>>,
    rpc: &Arc<AtomicU64>,
    out: &mpsc::UnboundedSender<Message>,
    plugin_id: Arc<str>,
    id: String,
    method: String,
    params: Value,
) {
    let registry = registry.clone();
    let context = context.clone();
    let pending = pending.clone();
    let rpc = rpc.clone();
    let out = out.clone();

    tokio::spawn(async move {
        let is_peer = method == "peer.call";
        let name = if is_peer {
            params
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        } else {
            method.clone()
        };
        let handler = if is_peer {
            registry.peer_handlers.get(&name).cloned()
        } else {
            registry.handlers.get(&name).cloned()
        };

        let response = match handler {
            None => Message::Response {
                id,
                ok: false,
                result: Value::Null,
                error: Some(
                    Error::not_supported(if is_peer {
                        format!("目标插件未实现 peer 方法: {name}")
                    } else {
                        format!("插件未实现方法: {method}")
                    })
                    .into(),
                ),
            },
            Some(handler) => {
                let ctx = Context {
                    plugin_id,
                    depth: if is_peer {
                        params.get("depth").and_then(Value::as_u64).unwrap_or(0) as u32
                    } else {
                        0
                    },
                    from: params
                        .get("from")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    context,
                    out: out.clone(),
                    pending,
                    rpc,
                };
                // peer 方法的入参是载荷里的 `params`；能力方法就是整个 params
                let args = if is_peer {
                    params.get("params").cloned().unwrap_or(Value::Null)
                } else {
                    params
                };
                match handler(args, ctx).await {
                    Ok(value) => Message::Response {
                        id,
                        ok: true,
                        result: value,
                        error: None,
                    },
                    Err(e) => Message::Response {
                        id,
                        ok: false,
                        result: Value::Null,
                        error: Some(e.into()),
                    },
                }
            }
        };

        let _ = out.send(response);
    });
}

fn handle_event(
    registry: &Registry,
    context: &Arc<RwLock<PluginContext>>,
    topic: &str,
    data: Value,
) {
    if topic == "core.context.update" {
        if let Ok(parsed) = serde_json::from_value::<PluginContext>(data) {
            if let Ok(mut slot) = context.try_write() {
                *slot = parsed.clone();
            }
            for handler in &registry.context_handlers {
                handler(&parsed);
            }
        }
        return;
    }
    if topic == "peer.event" {
        if let Ok(event) = serde_json::from_value::<PeerEvent>(data.clone()) {
            for handler in &registry.peer_event_handlers {
                handler(event.clone());
            }
        }
    }
    if let Some(handlers) = registry.event_handlers.get(topic) {
        for handler in handlers {
            handler(data.clone());
        }
    }
}

/// 读一个握手报文（跳过心跳帧）。
pub(crate) async fn read_handshake(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> Result<Handshake, Error> {
    loop {
        let msg = ws
            .next()
            .await
            .ok_or_else(|| Error::unavailable("核心在握手期间关闭了连接"))?
            .map_err(|e| Error::unavailable(format!("握手期间连接错误: {e}")))?;
        match msg {
            WsMessage::Text(text) => {
                return serde_json::from_str(&text)
                    .map_err(|e| Error::new("unauthorized", format!("握手报文解析失败: {e}")));
            }
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => {
                return Err(Error::new(
                    "unauthorized",
                    format!("握手期间收到非法帧: {other:?}"),
                ))
            }
        }
    }
}

pub(crate) async fn send_handshake(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    msg: &Handshake,
) -> Result<(), Error> {
    let text = serde_json::to_string(msg)
        .map_err(|e| Error::internal(format!("序列化握手报文失败: {e}")))?;
    ws.send(WsMessage::text(text))
        .await
        .map_err(|e| Error::unavailable(format!("发送握手报文失败: {e}")))
}

/// 从工作目录的 `plugin.json` 读版本号（读不到就返回 `None`）。
pub(crate) fn manifest_version() -> Option<String> {
    let text = std::fs::read_to_string("plugin.json").ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
}
