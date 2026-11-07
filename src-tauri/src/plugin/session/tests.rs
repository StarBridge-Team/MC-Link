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

#[test]
fn rate_limiter_blocks_burst() {
    let mut rl = RateLimiter::new(2);
    assert!(rl.try_acquire());
    assert!(rl.try_acquire());
    assert!(!rl.try_acquire());
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

    let (ok, _, _) = core_request(&peer, &perms, "dev.a.a", "core.ping", Value::Null).await;
    assert!(ok);

    let (ok, _, err) = core_request(&peer, &perms, "dev.a.a", "core.exec", Value::Null).await;
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
    )
    .await;
    assert!(!ok);
    assert_eq!(err.map(|e| e.code), Some(error_code::BAD_REQUEST.to_string()));
}
