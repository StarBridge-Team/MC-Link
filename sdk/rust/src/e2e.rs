//! 用**假核心**验证 Rust SDK 的会话流程（握手顺序、信封、分发、心跳）。
//!
//! 这不是跨实现验证——两侧都用本 crate 的 `wire`。跨实现一致性由
//! `wire.rs` 的 `node:crypto` 固定向量，以及核心侧
//! `src-tauri/src/plugin/node_e2e.rs`（真实 Bun + 真实网关）负责。
//!
//! 这里专门盯"顺序与状态机"：先发 hello、收到 hello_ack 再发 auth_ok、
//! 之后才允许加密帧。这类错误在真实环境里只表现为"插件连不上"，很难定位。

use std::path::PathBuf;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::wire::{self, Handshake, Message, PROTOCOL_VERSION};

fn temp_session(port: u16, psk_hex: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mclink-sdk-e2e-{}",
        hex::encode(crate::wire::hmac_hex(b"seed", &rand::random::<u64>().to_le_bytes()))
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dev.example.sdk.session.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&json!({
            "protocol": PROTOCOL_VERSION,
            "plugin_id": "dev.example.sdk",
            "endpoint": format!("ws://127.0.0.1:{port}/plugin"),
            "token": "token-1",
            "psk": psk_hex,
            "heartbeat_ms": 5000
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

/// 假核心：完成握手，然后发一个 `adapter.init` 请求并校验响应。
#[tokio::test]
async fn sdk_completes_handshake_and_answers_requests() {
    let psk = [7u8; 32];
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let session_path = temp_session(port, &hex::encode(psk));

    // 插件侧：建连 → 注册 → 跑起来
    let plugin_task = tokio::spawn(async move {
        let mut plugin = crate::Plugin::connect(&session_path).await?;
        plugin.handle("adapter.init", |_params, _ctx| async {
            Ok(json!({ "ready": true }))
        });
        plugin.handle("adapter.status", |_params, _ctx| async {
            Ok(json!({ "phase": "waiting" }))
        });
        plugin.on_peer("room.get", |_params, _ctx| async {
            Ok(json!({ "room": "ABCD" }))
        });
        plugin.run().await
    });

    // 核心侧：必须回 `Sec-WebSocket-Protocol`，否则客户端会判定子协议协商失败
    // （真实网关在 `gateway.rs::accept_ws` 的回调里做了同一件事）
    let (stream, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_hdr_async(
        stream,
        |_req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         mut resp: tokio_tungstenite::tungstenite::handshake::server::Response| {
            resp.headers_mut().insert(
                "Sec-WebSocket-Protocol",
                tokio_tungstenite::tungstenite::http::HeaderValue::from_static(
                    crate::wire::WS_SUBPROTOCOL,
                ),
            );
            Ok(resp)
        },
    )
    .await
    .unwrap();

    let session_id = "sid-1";
    let nonce_s = "aa".repeat(32);
    send(
        &mut ws,
        &Handshake::Hello {
            v: PROTOCOL_VERSION,
            server: "fake-core".into(),
            session: session_id.into(),
            nonce_s: nonce_s.clone(),
            methods: vec!["psk_hmac".into()],
            expect_plugin: "dev.example.sdk".into(),
        },
    )
    .await;

    let ack = read_handshake(&mut ws).await;
    let nonce_p = match ack {
        Handshake::HelloAck {
            plugin_id,
            nonce_p,
            proof,
            ..
        } => {
            assert_eq!(plugin_id, "dev.example.sdk");
            let expected = wire::hmac_hex(
                &psk,
                wire::transcript(session_id, "dev.example.sdk", &nonce_s, &nonce_p, "plugin")
                    .as_bytes(),
            );
            assert_eq!(proof, expected, "插件证明必须与核心期望一致");
            nonce_p
        }
        other => panic!("期望 hello_ack，收到 {other:?}"),
    };

    let core_proof = wire::hmac_hex(
        &psk,
        wire::transcript(session_id, "dev.example.sdk", &nonce_s, &nonce_p, "core").as_bytes(),
    );
    send(
        &mut ws,
        &Handshake::AuthOk {
            proof: core_proof,
            heartbeat_ms: 5000,
        },
    )
    .await;

    let keys = wire::Keys::derive(&psk, &nonce_s, &nonce_p, session_id, "dev.example.sdk").unwrap();

    // 能力方法
    let init = call(
        &mut ws,
        &keys,
        session_id,
        0,
        "dev.example.sdk-1",
        "adapter.init",
        json!({}),
    )
    .await;
    assert_eq!(init["ready"], true);

    // 未注册的方法必须回 not_supported，而不是挂起
    let missing = call_raw(
        &mut ws,
        &keys,
        session_id,
        1,
        "dev.example.sdk-2",
        "adapter.nope",
        json!({}),
    )
    .await;
    match missing {
        Message::Response { ok, error, .. } => {
            assert!(!ok, "未注册的方法不得返回成功");
            assert_eq!(error.map(|e| e.code).as_deref(), Some("not_supported"));
        }
        other => panic!("期望响应，收到 {other:?}"),
    }

    // peer 方法：目标侧收到的固定方法名是 `peer.call`
    let peer = call(
        &mut ws,
        &keys,
        session_id,
        2,
        "dev.example.sdk-3",
        "peer.call",
        json!({ "from": "dev.a.a", "method": "room.get", "params": null, "depth": 1 }),
    )
    .await;
    assert_eq!(peer["room"], "ABCD");

    // 会话层 ping 必须回 pong
    send_frame(&mut ws, &keys, session_id, 3, Message::Ping { at: 42 }).await;
    let pong = read_message(&mut ws, &keys, session_id).await;
    assert!(matches!(pong, Message::Pong { at: 42 }));

    // 收尾：关掉连接，插件应正常结束
    let _ = ws.close(None).await;
    let outcome = tokio::time::timeout(Duration::from_secs(5), plugin_task)
        .await
        .expect("插件未在关闭后退出");
    assert!(outcome.is_ok(), "插件任务 panic: {outcome:?}");
}

async fn send(ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, msg: &Handshake) {
    ws.send(WsMessage::text(serde_json::to_string(msg).unwrap()))
        .await
        .unwrap();
}

async fn read_handshake(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> Handshake {
    loop {
        match ws.next().await.unwrap().unwrap() {
            WsMessage::Text(text) => return serde_json::from_str(&text).unwrap(),
            _ => continue,
        }
    }
}

async fn send_frame(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    keys: &wire::Keys,
    sid: &str,
    seq: u64,
    msg: Message,
) {
    let frame = wire::seal(&keys.s2c, sid, seq, &msg).unwrap();
    ws.send(WsMessage::text(serde_json::to_string(&frame).unwrap()))
        .await
        .unwrap();
}

async fn read_message(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    keys: &wire::Keys,
    sid: &str,
) -> Message {
    loop {
        match ws.next().await.unwrap().unwrap() {
            WsMessage::Text(text) => {
                let frame: wire::SecureFrame = serde_json::from_str(&text).unwrap();
                return wire::open(&keys.c2s, sid, &frame).unwrap();
            }
            _ => continue,
        }
    }
}

async fn call_raw(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    keys: &wire::Keys,
    sid: &str,
    seq: u64,
    id: &str,
    method: &str,
    params: serde_json::Value,
) -> Message {
    send_frame(
        ws,
        keys,
        sid,
        seq,
        Message::Request {
            id: id.into(),
            method: method.into(),
            params,
        },
    )
    .await;
    loop {
        let msg = read_message(ws, keys, sid).await;
        match msg {
            Message::Response { .. } => return msg,
            // 心跳之类的帧可能在响应前后穿插
            _ => continue,
        }
    }
}

async fn call(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    keys: &wire::Keys,
    sid: &str,
    seq: u64,
    id: &str,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    match call_raw(ws, keys, sid, seq, id, method, params).await {
        Message::Response {
            ok: true, result, ..
        } => result,
        Message::Response { error, .. } => panic!("调用 {method} 失败: {error:?}"),
        other => panic!("期望响应，收到 {other:?}"),
    }
}
