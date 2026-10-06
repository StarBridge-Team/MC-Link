//! WebSocket 升级、准入校验与双向挑战-应答。

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::WebSocketStream;

use crate::plugin::auth::{new_challenge, Challenge, METHOD_PSK_HMAC};
use crate::plugin::crypto::{self, SessionKeys};
use crate::plugin::manifest::LimitsSpec;
use crate::plugin::protocol::{self, error_code, Handshake};

/// 握手允许的最长时间。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// 升级成功后的连接。
pub struct Accepted {
    pub ws: WebSocketStream<TcpStream>,
    pub plugin_id: String,
}

/// 核心自述身份。只认清单，不认插件的自述。
pub(super) fn server_name() -> String {
    format!("mc-link/{}", env!("CARGO_PKG_VERSION"))
}

/// 完成握手：(流, 会话密钥, 挑战数据)。
pub(super) async fn handshake(
    mut ws: WebSocketStream<TcpStream>,
    plugin_id: &str,
    psk: &[u8],
    limits: &LimitsSpec,
) -> Result<(WebSocketStream<TcpStream>, SessionKeys, Challenge), String> {
    let mut challenge = new_challenge(plugin_id);

    send_handshake(
        &mut ws,
        &Handshake::Hello {
            v: protocol::PROTOCOL_VERSION,
            server: server_name(),
            session: challenge.session_id.clone(),
            nonce_s: challenge.nonce_s.clone(),
            methods: vec![METHOD_PSK_HMAC.to_string()],
            expect_plugin: plugin_id.to_string(),
        },
    )
    .await?;

    let (plugin_version, nonce_p, proof) = match recv_handshake(&mut ws).await? {
        Handshake::HelloAck {
            plugin_id: claimed,
            plugin_version,
            nonce_p,
            proof,
        } => {
            if claimed != plugin_id {
                reject_handshake(&mut ws, "插件 ID 与清单不符").await;
                return Err(format!(
                    "插件 ID 不符：期望 {}，声称 {}",
                    plugin_id, claimed
                ));
            }
            (plugin_version, nonce_p, proof)
        }
        other => return Err(format!("握手期望 hello_ack，收到 {:?}", other)),
    };

    let (_, core_proof) = challenge.finalize(psk, nonce_p);
    if !challenge.verify_plugin_proof(&proof) {
        reject_handshake(&mut ws, "预共享密钥校验失败").await;
        return Err(format!(
            "插件 {} 认证失败（版本 {}）",
            plugin_id, plugin_version
        ));
    }

    let keys = challenge.derive_keys(psk)?;

    send_handshake(
        &mut ws,
        &Handshake::AuthOk {
            proof: core_proof,
            heartbeat_ms: limits.heartbeat_ms,
        },
    )
    .await?;

    Ok((ws, keys, challenge))
}

async fn reject_handshake(ws: &mut WebSocketStream<TcpStream>, message: &str) {
    let _ = send_handshake(
        ws,
        &Handshake::Denied {
            code: error_code::UNAUTHORIZED.to_string(),
            message: message.to_string(),
        },
    )
    .await;
}

async fn send_handshake(
    ws: &mut WebSocketStream<TcpStream>,
    msg: &Handshake,
) -> Result<(), String> {
    let text = serde_json::to_string(msg).map_err(|e| format!("序列化握手报文失败: {}", e))?;
    ws.send(WsMessage::text(text))
        .await
        .map_err(|e| format!("发送握手报文失败: {}", e))
}

async fn recv_handshake(ws: &mut WebSocketStream<TcpStream>) -> Result<Handshake, String> {
    let deadline = tokio::time::Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let next = tokio::time::timeout_at(deadline, ws.next())
            .await
            .map_err(|_| "握手超时".to_string())?;
        match next {
            Some(Ok(WsMessage::Text(text))) => {
                if text.len() > protocol::MAX_HANDSHAKE_BYTES {
                    return Err("握手报文过大".to_string());
                }
                return serde_json::from_str(text.as_str())
                    .map_err(|e| format!("握手报文解析失败: {}", e));
            }
            Some(Ok(WsMessage::Ping(_))) | Some(Ok(WsMessage::Pong(_))) => continue,
            Some(Ok(WsMessage::Close(_))) | None => {
                return Err("对端在握手期间关闭连接".to_string())
            }
            Some(Ok(_)) => return Err("握手期间收到非法帧类型".to_string()),
            Some(Err(e)) => return Err(format!("握手期间连接错误: {}", e)),
        }
    }
}

/// 升级 WebSocket 并校验启动令牌与子协议。
///
/// 令牌由核心在拉起插件进程时生成，通过**会话文件**而非命令行传递，
/// 避免被同机其它进程从进程列表中读取。
///
/// 这里的回调必须返回 `http::Response` 作为错误类型（tungstenite 的接口约定），
/// 它本身较大，但只出现在握手失败路径上，不影响热路径性能。
#[allow(clippy::result_large_err)]
pub async fn accept_ws(stream: TcpStream, expected_token: String) -> Result<Accepted, String> {
    let captured: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
    let sink = captured.clone();

    let callback = move |req: &Request, mut resp: Response| -> Result<Response, ErrorResponse> {
        let plugin_id = query_param(req, "plugin").unwrap_or_default();
        let token = query_param(req, "token").unwrap_or_default();

        if plugin_id.is_empty() {
            return Err(forbidden("缺少 plugin 参数"));
        }
        if !crypto::ct_eq(token.as_bytes(), expected_token.as_bytes()) {
            return Err(forbidden("启动令牌无效"));
        }
        if let Ok(mut slot) = sink.lock() {
            *slot = Some(plugin_id);
        }
        if let Ok(value) = protocol::WS_SUBPROTOCOL.parse() {
            resp.headers_mut().insert("Sec-WebSocket-Protocol", value);
        }
        Ok(resp)
    };

    // 握手阶段就把 WS 层帧上限收到 16 KB。
    //
    // tungstenite 默认 `max_message_size` 是 64 MB，而 `recv_handshake` 只在**拿到
    // 整帧之后**才检查 `MAX_HANDSHAKE_BYTES` —— 也就是说恶意对端可以用一个 64 MB 的
    // 文本帧把内存放大，或用无限 Ping 帧消耗 CPU。会话期有 `max_frame_bytes.min(4 MB)`
    // 兜底（见 `session/io.rs`），握手期必须在这里配。
    let config = WebSocketConfig {
        max_message_size: Some(protocol::MAX_HANDSHAKE_BYTES),
        max_frame_size: Some(protocol::MAX_HANDSHAKE_BYTES),
        ..WebSocketConfig::default()
    };

    let ws = tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config))
        .await
        .map_err(|e| format!("WebSocket 升级失败: {}", e))?;

    let plugin_id = captured
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .ok_or_else(|| "缺少插件标识".to_string())?;

    Ok(Accepted { ws, plugin_id })
}

fn query_param(request: &Request, key: &str) -> Option<String> {
    let query = request.uri().query()?;
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next()? == key {
            return it.next().map(percent_decode);
        }
    }
    None
}

/// 极简百分号解码（仅用于令牌与插件 ID）。
///
/// 全程按**字节**操作，不切片 `&str`：`s[i+1..i+3]` 在 `%` 后面跟多字节字符
/// （如 `%é`）时不是合法 char 边界，会直接 panic。这里改为在字节数组上取值，
/// 非 ASCII 字节原样透传（`from_utf8_lossy` 兜底）。
pub(super) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn forbidden(msg: &str) -> ErrorResponse {
    let mut resp = ErrorResponse::new(Some(msg.to_string()));
    *resp.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::FORBIDDEN;
    resp
}
