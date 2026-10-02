//! 会话层的单元测试。
//!
//! 这里的断言都围绕"密码学与限流这两条安全底线"展开，而不是业务行为——
//! 一旦它们失败，说明控制面已经不安全，应当视为阻断级问题。

use super::handshake::percent_decode;
use super::io::handle_core_request;
use super::wire::{open_frame, seal_message, RateLimiter};
use crate::plugin::crypto::{self, Cipher, ReplayWindow};
use crate::plugin::protocol::{error_code, Message};
use serde_json::Value;

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

#[test]
fn core_request_only_exposes_minimal_surface() {
    let (ok, _, _) = handle_core_request("core.ping", Value::Null);
    assert!(ok);
    let (ok, _, err) = handle_core_request("core.exec", Value::Null);
    assert!(!ok);
    assert_eq!(
        err.map(|e| e.code),
        Some(error_code::NOT_SUPPORTED.to_string())
    );
}
