//! 加密信封的编解码、出站发送与限流器。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::plugin::crypto::{Cipher, ReplayWindow};
use crate::plugin::protocol::{Message, SecureFrame};

use super::WsSink;

/// 当前时间（毫秒时间戳）。
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 令牌桶限流器（限制插件向核心的请求速率）。
pub(super) struct RateLimiter {
    capacity: f64,
    tokens: f64,
    refill_per_ms: f64,
    last: Instant,
}

impl RateLimiter {
    pub(super) fn new(per_sec: u32) -> Self {
        let cap = per_sec.max(1) as f64;
        Self {
            capacity: cap,
            tokens: cap,
            refill_per_ms: cap / 1000.0,
            last: Instant::now(),
        }
    }

    pub(super) fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_millis() as f64;
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_ms).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// 加密一条消息并发出。
pub(super) async fn send_message(
    sink: &mut WsSink,
    cipher: &Cipher,
    sid: &str,
    seq: &mut u64,
    msg: &Message,
) -> Result<(), String> {
    let frame = seal_message(cipher, sid, *seq, msg)?;
    *seq += 1;
    let text = serde_json::to_string(&frame).map_err(|e| format!("序列化出站帧失败: {}", e))?;
    sink.send(WsMessage::text(text))
        .await
        .map_err(|_| "插件连接已断开".to_string())
}

pub(super) fn seal_message(
    cipher: &Cipher,
    sid: &str,
    seq: u64,
    msg: &Message,
) -> Result<SecureFrame, String> {
    let plaintext = serde_json::to_vec(msg).map_err(|e| format!("序列化消息失败: {}", e))?;
    let mut frame = SecureFrame {
        v: 1,
        sid: sid.to_string(),
        seq,
        n: String::new(),
        ct: String::new(),
    };
    let aad = frame.aad();
    let (nonce, ct) = cipher.seal(&aad, &plaintext)?;
    frame.n = base64_encode(&nonce);
    frame.ct = base64_encode(&ct);
    Ok(frame)
}

pub(super) fn open_frame(
    cipher: &Cipher,
    sid: &str,
    replay: &mut ReplayWindow,
    text: &str,
) -> Result<Message, String> {
    let frame: SecureFrame =
        serde_json::from_str(text).map_err(|e| format!("信封解析失败: {}", e))?;
    if frame.v != 1 {
        return Err(format!("不支持的协议版本: {}", frame.v));
    }
    if frame.sid != sid {
        return Err("会话 ID 不匹配".to_string());
    }
    if !replay.accept(frame.seq) {
        return Err(format!("检测到重放或乱序帧: {}", frame.seq));
    }
    let nonce = base64_decode(&frame.n)?;
    let ct = base64_decode(&frame.ct)?;
    let plaintext = cipher.open(&frame.aad(), &nonce, &ct)?;
    serde_json::from_slice(&plaintext).map_err(|e| format!("消息解析失败: {}", e))
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|e| format!("base64 解码失败: {}", e))
}
