//! 协议层：握手报文、加密信封、消息编解码。
//!
//! 与核心 `src-tauri/src/plugin/protocol.rs` / `crypto.rs` 逐字节对齐。
//! 改这里的任何常量（transcript 拼法、HKDF info、AAD 格式）都必须同时改核心，
//! 否则表现为"握手成功但第一帧就解密失败"。

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;

use crate::Error;

/// 协议版本。不匹配时核心直接拒绝连接。
pub const PROTOCOL_VERSION: u16 = 1;
/// WebSocket 子协议名。
pub const WS_SUBPROTOCOL: &str = "mclink-plugin";
/// 握手转录串前缀。
pub const AUTH_PREFIX: &str = "mclink-plugin-auth-v1";
/// 派生密钥的 HKDF info 前缀。
pub const INFO_C2S: &[u8] = b"mclink-plugin-c2s-v1";
pub const INFO_S2C: &[u8] = b"mclink-plugin-s2c-v1";

/// 会话文件（核心在拉起插件时写入）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescriptor {
    pub protocol: u16,
    pub plugin_id: String,
    pub endpoint: String,
    pub token: String,
    /// 预共享密钥（hex）。
    pub psk: String,
    #[serde(default)]
    pub heartbeat_ms: u64,
}

impl SessionDescriptor {
    /// 从会话文件路径读取。
    pub fn load(path: &std::path::Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::new("bad_request", format!("读取会话文件失败 {}: {e}", path.display()))
        })?;
        let mut desc: Self = serde_json::from_str(&text)
            .map_err(|e| Error::new("bad_request", format!("会话文件解析失败: {e}")))?;
        desc.psk = desc.psk.trim().to_string();
        if desc.psk.is_empty() {
            return Err(Error::new("unauthorized", "会话文件缺少 psk"));
        }
        if desc.protocol != PROTOCOL_VERSION {
            return Err(Error::new(
                "unauthorized",
                format!("协议版本不匹配：核心 {}，SDK {PROTOCOL_VERSION}", desc.protocol),
            ));
        }
        Ok(desc)
    }

    /// 握手用的 PSK 原始字节。
    pub fn psk_bytes(&self) -> Result<[u8; 32], Error> {
        let bytes = hex::decode(&self.psk)
            .map_err(|e| Error::new("unauthorized", format!("psk 不是合法 hex: {e}")))?;
        if bytes.len() != 32 {
            return Err(Error::new("unauthorized", "psk 必须是 32 字节"));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    /// 建连地址（`endpoint` + 插件 ID 与令牌）。
    pub fn websocket_url(&self) -> String {
        format!(
            "{}?plugin={}&token={}",
            self.endpoint,
            urlencode(&self.plugin_id),
            urlencode(&self.token)
        )
    }
}

fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 握手报文（明文 JSON，一次性）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Handshake {
    Hello {
        v: u16,
        server: String,
        session: String,
        nonce_s: String,
        methods: Vec<String>,
        expect_plugin: String,
    },
    HelloAck {
        plugin_id: String,
        plugin_version: String,
        nonce_p: String,
        proof: String,
    },
    AuthOk {
        proof: String,
        heartbeat_ms: u64,
    },
    Denied {
        code: String,
        message: String,
    },
}

/// 会话报文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Message {
    Request {
        id: String,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Response {
        id: String,
        ok: bool,
        #[serde(default)]
        result: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ErrorInfo>,
    },
    Event {
        topic: String,
        #[serde(default)]
        data: Value,
    },
    Ping { at: u64 },
    Pong { at: u64 },
    Bye { reason: String },
}

/// 结构化错误（报文里的形状）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

/// 加密信封。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureFrame {
    pub v: u8,
    pub sid: String,
    pub seq: u64,
    pub n: String,
    pub ct: String,
}

impl SecureFrame {
    /// AAD：版本、会话与序号，任一被改都会解密失败。
    pub fn aad(&self) -> Vec<u8> {
        format!("v{}|{}|{}", self.v, self.sid, self.seq).into_bytes()
    }
}

/// 双向会话密钥。
#[derive(Clone)]
pub struct Keys {
    pub c2s: [u8; 32],
    pub s2c: [u8; 32],
}

impl Keys {
    /// HKDF-SHA256 派生。
    ///
    /// `salt` = `nonce_s || nonce_p`（两个 hex 字符串的 ASCII 拼接）；
    /// `info` = 方向前缀 **拼接** `role = "session"` 的转录串。
    pub fn derive(psk: &[u8; 32], nonce_s: &str, nonce_p: &str, session: &str, plugin_id: &str) -> Result<Self, Error> {
        let salt = format!("{nonce_s}{nonce_p}");
        let transcript = transcript(session, plugin_id, nonce_s, nonce_p, "session");
        let hk = Hkdf::<Sha256>::new(Some(salt.as_bytes()), psk);
        let mut c2s = [0u8; 32];
        let mut s2c = [0u8; 32];
        hk.expand_multi_info(&[INFO_C2S, transcript.as_bytes()], &mut c2s)
            .map_err(|_| Error::new("internal", "派生 c2s 密钥失败"))?;
        hk.expand_multi_info(&[INFO_S2C, transcript.as_bytes()], &mut s2c)
            .map_err(|_| Error::new("internal", "派生 s2c 密钥失败"))?;
        Ok(Self { c2s, s2c })
    }
}

/// 握手转录串。双方必须逐字符一致。
pub fn transcript(session: &str, plugin_id: &str, nonce_s: &str, nonce_p: &str, role: &str) -> String {
    format!("{AUTH_PREFIX}|{session}|{plugin_id}|{nonce_s}|{nonce_p}|{role}")
}

/// HMAC-SHA256 → 小写 hex。
pub fn hmac_hex(key: &[u8], data: &[u8]) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

/// 常量时间比较两个 hex 字符串（长度不同直接否）。
pub fn ct_eq_hex(a: &str, b: &str) -> bool {
    let (Ok(a), Ok(b)) = (hex::decode(a.trim()), hex::decode(b.trim())) else {
        return false;
    };
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 加密一条消息。
pub fn seal(key: &[u8; 32], sid: &str, seq: u64, msg: &Message) -> Result<SecureFrame, Error> {
    let plaintext = serde_json::to_vec(msg)
        .map_err(|e| Error::new("internal", format!("序列化报文失败: {e}")))?;
    let mut frame = SecureFrame {
        v: 1,
        sid: sid.to_string(),
        seq,
        n: String::new(),
        ct: String::new(),
    };
    let aad = frame.aad();
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| Error::new("internal", "AES-256-GCM 密钥长度非法"))?;
    let sealed = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload { msg: &plaintext, aad: &aad },
        )
        .map_err(|_| Error::new("internal", "加密失败"))?;
    frame.n = B64.encode(nonce_bytes);
    frame.ct = B64.encode(sealed);
    Ok(frame)
}

/// 解密一条消息（`sid` 必须与本地一致）。
pub fn open(key: &[u8; 32], sid: &str, frame: &SecureFrame) -> Result<Message, Error> {
    if frame.v != 1 {
        return Err(Error::new("unauthorized", format!("不支持的协议版本: {}", frame.v)));
    }
    if frame.sid != sid {
        return Err(Error::new("unauthorized", "会话 ID 不匹配"));
    }
    let nonce = B64
        .decode(&frame.n)
        .map_err(|e| Error::new("unauthorized", format!("nonce 解码失败: {e}")))?;
    let ct = B64
        .decode(&frame.ct)
        .map_err(|e| Error::new("unauthorized", format!("密文解码失败: {e}")))?;
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| Error::new("internal", "AES-256-GCM 密钥长度非法"))?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload { msg: &ct, aad: &frame.aad() },
        )
        .map_err(|_| Error::new("unauthorized", "解密密文失败（密钥或 AAD 不匹配）"))?;
    serde_json::from_slice(&plain)
        .map_err(|e| Error::new("bad_request", format!("报文解析失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 固定向量：由 `node:crypto`（独立实现）生成，两边必须逐字节一致。
    ///
    /// 只要这些值对不上，插件与核心就不可能握手成功——比"连不上"更早暴露问题。
    #[test]
    fn hmac_proof_matches_cross_language_vector() {
        let psk: [u8; 32] = std::array::from_fn(|i| i as u8);
        let proof = hmac_hex(
            &psk,
            transcript("S", "dev.a.a", "NS", "NP", "plugin").as_bytes(),
        );
        assert_eq!(
            proof,
            "4bc3e85948b068a6b5e43b59c24fe686f906d70033995aa3ad5330292a9a2355"
        );
    }

    /// HKDF 与 `node:crypto.hkdfSync` 一致（salt = nonce_s || nonce_p）。
    #[test]
    fn hkdf_matches_cross_language_vector() {
        let psk: [u8; 32] = std::array::from_fn(|i| i as u8);
        let keys = Keys::derive(&psk, "NS", "NP", "S", "dev.a.a").unwrap();
        assert_eq!(
            hex::encode(keys.c2s),
            "cc59ba453bb26f764dd45bc841168b20629010ab42a6faf161196f779ee19ee5"
        );
        assert_eq!(
            hex::encode(keys.s2c),
            "56beefc7ead711475b33054e3db108cd144c18ffa13b00a83f559cab2120c525"
        );
    }

    #[test]
    fn envelope_roundtrip_and_aad_binding() {
        let key = [3u8; 32];
        let msg = Message::Ping { at: 42 };
        let frame = seal(&key, "sid", 7, &msg).unwrap();
        assert_eq!(frame.v, 1);
        assert_eq!(frame.seq, 7);

        let back = open(&key, "sid", &frame).unwrap();
        assert!(matches!(back, Message::Ping { at: 42 }));

        assert!(open(&key, "other", &frame).is_err(), "sid 不同必须失败");
        let mut tampered = frame.clone();
        tampered.seq = 8;
        assert!(open(&key, "sid", &tampered).is_err(), "seq 被改必须失败");
    }

    #[test]
    fn psk_must_be_32_bytes() {
        let desc = SessionDescriptor {
            protocol: 1,
            plugin_id: "a.b".into(),
            endpoint: "ws://127.0.0.1:1/plugin".into(),
            token: "t".into(),
            psk: "00".repeat(31),
            heartbeat_ms: 5000,
        };
        assert!(desc.psk_bytes().is_err());
    }

    #[test]
    fn websocket_url_encodes_query_values() {
        let desc = SessionDescriptor {
            protocol: 1,
            plugin_id: "dev.a.b".into(),
            endpoint: "ws://127.0.0.1:9/plugin".into(),
            token: "a b+c".into(),
            psk: "00".repeat(32),
            heartbeat_ms: 5000,
        };
        let url = desc.websocket_url();
        assert!(url.contains("plugin=dev.a.b"));
        assert!(url.contains("token=a%20b%2Bc"), "{url}");
    }
}
