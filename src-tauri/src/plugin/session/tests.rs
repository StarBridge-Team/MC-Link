//! 会话层的单元测试。
//!
//! 这里的断言都围绕"密码学与限流这两条安全底线"展开，而不是业务行为——
//! 一旦它们失败，说明控制面已经不安全，应当视为阻断级问题。

use super::handshake::percent_decode;
use super::io::core_request;
use super::wire::{open_frame, seal_message, RateLimiter};
use crate::plugin::crypto::{self, Cipher, ReplayWindow};
use crate::plugin::peer::{PeerHandle, PeerOp};
use crate::plugin::permission::{Permission, PermissionSet};
use crate::plugin::protocol::{error_code, Message};
use serde_json::{json, Value};

/// 一个空的事件通道：core_request 需要它来转发 core.log。
fn events() -> crate::plugin::session::InboundTx {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    // 保留接收端直到进程结束没有意义；测试里它只用于断言日志是否被投递
    std::mem::forget(rx);
    tx
}

#[test]
fn rate_limiter_blocks_burst() {
    let mut rl = RateLimiter::new(2);
    assert!(rl.try_acquire());
    assert!(rl.try_acquire());
    assert!(!rl.try_acquire());
}

/// 限流口径：只有插件**主动发起**的请求与事件计入配额。
///
/// 回复与心跳回复的节奏由核心决定；把它们也算进桶里，会让"核心多轮询了几次
/// 状态"变成"插件被判定超速并断开"。
#[test]
fn only_plugin_initiated_traffic_counts_toward_budget() {
    use super::io::counts_toward_budget;

    assert!(counts_toward_budget(&Message::Request {
        id: "1".into(),
        method: "core.ping".into(),
        params: Value::Null,
    }));
    assert!(counts_toward_budget(&Message::Event {
        topic: "plugin.log".into(),
        data: Value::Null,
    }));

    assert!(!counts_toward_budget(&Message::Response {
        id: "1".into(),
        ok: true,
        result: Value::Null,
        error: None,
    }));
    assert!(!counts_toward_budget(&Message::Pong { at: 1 }));
    assert!(!counts_toward_budget(&Message::Ping { at: 1 }));
    assert!(!counts_toward_budget(&Message::Bye {
        reason: "x".into()
    }));
}

#[test]
fn seal_open_roundtrip_through_frames() {
    let key = [3u8; crypto::KEY_LEN];
    let cipher = Cipher::new(&key);
    let msg = Message::Pong { at: 7 };
    let frame = seal_message(&cipher, "s1", 0, &msg).unwrap();
    let text = serde_json::to_string(&frame).unwrap();
    let mut replay = ReplayWindow::new();
    let back = open_frame(&cipher, "s1", &mut replay, &text).unwrap();
    assert!(matches!(back, Message::Pong { at: 7 }));
    // 同一帧再次送达必须被拒（防重放）。
    assert!(open_frame(&cipher, "s1", &mut replay, &text).is_err());
}

#[test]
fn open_frame_rejects_foreign_session() {
    let key = [5u8; crypto::KEY_LEN];
    let cipher = Cipher::new(&key);
    let frame = seal_message(&cipher, "s1", 0, &Message::Pong { at: 1 }).unwrap();
    let text = serde_json::to_string(&frame).unwrap();
    let mut replay = ReplayWindow::new();
    assert!(open_frame(&cipher, "s2", &mut replay, &text).is_err());
}

#[test]
fn open_frame_rejects_wrong_key() {
    let frame = seal_message(
        &Cipher::new(&[1u8; crypto::KEY_LEN]),
        "s1",
        0,
        &Message::Pong { at: 1 },
    )
    .unwrap();
    let text = serde_json::to_string(&frame).unwrap();
    let mut replay = ReplayWindow::new();
    assert!(open_frame(
        &Cipher::new(&[2u8; crypto::KEY_LEN]),
        "s1",
        &mut replay,
        &text
    )
    .is_err());
}

#[test]
fn percent_decode_handles_escapes() {
    assert_eq!(percent_decode("a%2Fb"), "a/b");
    assert_eq!(percent_decode("plain"), "plain");
}

#[tokio::test]
async fn core_request_only_exposes_minimal_surface() {
    let (peer, _rx) = PeerHandle::channel();
    let perms = PermissionSet::default();

    let (ok, _, _) = core_request(&peer, &perms, "dev.a.a", "core.ping", Value::Null, &events()).await;
    assert!(ok);

    let (ok, _, err) = core_request(&peer, &perms, "dev.a.a", "core.exec", Value::Null, &events()).await;
    assert!(!ok);
    assert_eq!(
        err.map(|e| e.code),
        Some(error_code::NOT_SUPPORTED.to_string())
    );
}

/// 没有 `peer_message` 时，`core.peer.call` 必须被拒，且**不向中枢投递任何操作**。
#[tokio::test]
async fn peer_call_without_permission_never_reaches_the_hub() {
    let (peer, mut rx) = PeerHandle::channel();
    let perms = PermissionSet::default();
    let (ok, _, err) = core_request(
        &peer,
        &perms,
        "dev.a.a",
        "core.peer.call",
        json!({ "to": "dev.b.b", "method": "x.y" }),
        &events(),
    )
    .await;
    assert!(!ok);
    assert_eq!(err.map(|e| e.code), Some(error_code::FORBIDDEN.to_string()));
    assert!(
        rx.try_recv().is_err(),
        "被拒的调用不得进入 peer 中枢（否则能靠重试刷爆中转队列）"
    );
}

/// 有权时，请求按"从谁、发给谁、什么方法、第几层"原样进中枢，回执原路返回。
#[tokio::test]
async fn peer_call_is_relayed_with_origin_and_depth() {
    let (peer, mut rx) = PeerHandle::channel();
    let perms = PermissionSet::from_declared(&[Permission::PeerMessage]);

    let responder = tokio::spawn(async move {
        match rx.recv().await.expect("中枢应收到一次调用") {
            PeerOp::Call {
                from,
                request,
                reply,
            } => {
                assert_eq!(from, "dev.a.a");
                assert_eq!(request.to, "dev.b.b");
                assert_eq!(request.method, "room.directory");
                assert_eq!(request.depth, 0);
                assert_eq!(request.params["query"], "mc");
                let _ = reply.send(Ok(json!({ "rooms": ["A", "B"] })));
            }
            other => panic!("期望一次 Call，收到 {:?}", other),
        }
    });

    let (ok, result, err) = core_request(
        &peer,
        &perms,
        "dev.a.a",
        "core.peer.call",
        json!({
            "to": "dev.b.b",
            "method": "room.directory",
            "params": { "query": "mc" }
        }),
        &events(),
    )
    .await;
    assert!(ok, "调用应当成功: {:?}", err);
    assert_eq!(result["rooms"][0], "A");
    responder.await.unwrap();
}

#[tokio::test]
async fn peer_call_rejects_reserved_method_and_self_call() {
    let (peer, _rx) = PeerHandle::channel();
    let perms = PermissionSet::from_declared(&[Permission::PeerMessage]);

    let (ok, _, err) = core_request(
        &peer,
        &perms,
        "dev.a.a",
        "core.peer.call",
        json!({ "to": "dev.b.b", "method": "adapter.host.start" }),
        &events(),
    )
    .await;
    assert!(!ok, "借用能力方法前缀必须被拒");
    assert_eq!(err.map(|e| e.code), Some(error_code::BAD_REQUEST.to_string()));

    let (ok, _, _) = core_request(
        &peer,
        &perms,
        "dev.a.a",
        "core.peer.call",
        json!({ "to": "dev.a.a", "method": "x.y" }),
        &events(),
    )
    .await;
    assert!(!ok, "不允许调用自己");
}

/// 嵌套深度上限：超过 [`crate::plugin::peer::MAX_DEPTH`] 一律拒绝，
/// 否则两枚互相调用的插件可以无限乒乓。
#[tokio::test]
async fn peer_call_rejects_excessive_depth() {
    let (peer, _rx) = PeerHandle::channel();
    let perms = PermissionSet::from_declared(&[Permission::PeerMessage]);
    let (ok, _, err) = core_request(
        &peer,
        &perms,
        "dev.a.a",
        "core.peer.call",
        json!({
            "to": "dev.b.b",
            "method": "x.y",
            "depth": crate::plugin::peer::MAX_DEPTH
        }),
        &events(),
    )
    .await;
    assert!(!ok);
    assert_eq!(err.map(|e| e.code), Some(error_code::BAD_REQUEST.to_string()));
}

/// `core.log` 不需要任何权限，且必须真的投递到事件流（界面靠它排障）。
#[tokio::test]
async fn core_log_is_forwarded_without_permissions() {
    let (peer, _rx) = PeerHandle::channel();
    let perms = PermissionSet::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let (ok, _, err) = core_request(
        &peer,
        &perms,
        "dev.a.a",
        "core.log",
        json!({ "level": "warn", "message": "端口被占用", "port": 25565 }),
        &tx,
    )
    .await;
    assert!(ok, "日志不要求 ui_notify: {:?}", err);

    match rx.recv().await.expect("应投递一条日志事件") {
        crate::plugin::session::Inbound::Event {
            plugin_id,
            topic,
            data,
        } => {
            assert_eq!(plugin_id, "dev.a.a");
            assert_eq!(topic, "plugin.log");
            assert_eq!(data["level"], "warn");
            assert_eq!(data["message"], "端口被占用");
            // 附加字段原样带上，便于把日志和具体资源对上
            assert_eq!(data["port"], 25565);
        }
        other => panic!("期望 Event，收到 {:?}", std::mem::discriminant(&other)),
    }
}

/// 缺 `message` 或超长时拒绝：日志会进界面，不能变成无界内存。
#[tokio::test]
async fn core_log_validates_payload() {
    let (peer, _rx) = PeerHandle::channel();
    let perms = PermissionSet::default();

    let (ok, _, _) = core_request(&peer, &perms, "dev.a.a", "core.log", json!({}), &events()).await;
    assert!(!ok, "缺 message 应被拒");

    let long = "x".repeat(8 * 1024);
    let (ok, _, _) = core_request(
        &peer,
        &perms,
        "dev.a.a",
        "core.log",
        json!({ "message": long }),
        &events(),
    )
    .await;
    assert!(!ok, "超长日志应被拒");
}
