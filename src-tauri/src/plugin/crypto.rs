//! 插件控制面的密码学原语。
//!
//! 设计目标（对应安全策略中的"消息加密"与"防重放"）：
//! - **密钥派生**：以插件预共享密钥（PSK）为根，配合握手过程中双方随机数
//!   派生出互不相同的双向会话密钥，避免直接使用 PSK 加密数据。
//! - **认证加密**：所有控制面报文使用 AES-256-GCM，AAD 绑定协议版本、
//!   会话 ID 与序号，任何字段被篡改都会导致解密失败。
//! - **防重放**：单调递增序号 + 滑动窗口位图，重复或过旧的帧直接丢弃。
//! - **常量时间比较**：握手证明、MAC 校验全部使用常量时间比较，避免计时侧信道。

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// 对称密钥长度（AES-256）。
pub const KEY_LEN: usize = 32;
/// GCM 推荐随机数长度。
pub const NONCE_LEN: usize = 12;

/// 握手完成后派生出的双向会话密钥。
#[derive(Clone)]
pub struct SessionKeys {
    /// 插件 → 核心（client to server）
    pub c2s: [u8; KEY_LEN],
    /// 核心 → 插件（server to client）
    pub s2c: [u8; KEY_LEN],
}

impl SessionKeys {
    /// 由 PSK、盐值与握手转录（transcript）派生双向密钥。
    ///
    /// `transcript` 应包含双方随机数、插件 ID 与会话 ID，使同一 PSK 在不同
    /// 会话中派生出不同密钥（前向隔离）。
    pub fn derive(psk: &[u8], salt: &[u8], transcript: &[u8]) -> Result<Self, String> {
        let hk = Hkdf::<Sha256>::new(Some(salt), psk);
        let mut c2s = [0u8; KEY_LEN];
        let mut s2c = [0u8; KEY_LEN];
        hk.expand_multi_info(&[b"mclink-plugin-c2s-v1", transcript], &mut c2s)
            .map_err(|_| "派生 c2s 密钥失败".to_string())?;
        hk.expand_multi_info(&[b"mclink-plugin-s2c-v1", transcript], &mut s2c)
            .map_err(|_| "派生 s2c 密钥失败".to_string())?;
        Ok(Self { c2s, s2c })
    }
}

/// 生成 `n` 字节密码学安全随机数。
pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// 生成 `n` 字节随机数的十六进制表示。
pub fn random_hex(n: usize) -> String {
    hex::encode(random_bytes(n))
}

/// HMAC-SHA256，返回原始摘要。
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(data);
    let out = mac.finalize().into_bytes();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

/// HMAC-SHA256 的十六进制表示。
pub fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    hex::encode(hmac_sha256(key, data))
}

/// 常量时间校验 HMAC 十六进制值。
pub fn verify_hmac_hex(key: &[u8], data: &[u8], expected_hex: &str) -> bool {
    let expected = match hex::decode(expected_hex.trim()) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let actual = hmac_sha256(key, data);
    if expected.len() != actual.len() {
        return false;
    }
    actual.ct_eq(expected.as_slice()).into()
}

/// 常量时间比较两段字节。
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

/// 常量时间比较两个十六进制字符串。
pub fn ct_eq_hex(a: &str, b: &str) -> bool {
    let (Ok(ba), Ok(bb)) = (hex::decode(a.trim()), hex::decode(b.trim())) else {
        return false;
    };
    ct_eq(&ba, &bb)
}

/// 方向无关的单向 AEAD 封装（调用方自行选择 c2s / s2c 密钥）。
pub struct Cipher {
    inner: Aes256Gcm,
}

impl Cipher {
    pub fn new(key: &[u8; KEY_LEN]) -> Self {
        Self {
            inner: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key)),
        }
    }

    /// 加密明文，返回 (nonce, ciphertext)。
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let nonce_bytes = random_bytes(NONCE_LEN);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = self
            .inner
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| "控制面报文加密失败".to_string())?;
        Ok((nonce_bytes, ct))
    }

    /// 解密。AAD 不匹配、密文被篡改或密钥错误都会返回错误。
    pub fn open(&self, aad: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        if nonce.len() != NONCE_LEN {
            return Err("随机数长度非法".to_string());
        }
        let nonce = Nonce::from_slice(nonce);
        self.inner
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| "控制面报文解密失败（密钥不匹配或数据被篡改）".to_string())
    }
}

/// 防重放滑动窗口。
///
/// 记录已接收过的最大序号与最近 64 个序号的存在位图。序号重复、过旧或
/// 超出窗口范围的帧一律拒绝。
pub struct ReplayWindow {
    highest: u64,
    bitmap: u64,
    started: bool,
}

impl Default for ReplayWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplayWindow {
    pub fn new() -> Self {
        Self {
            highest: 0,
            bitmap: 0,
            started: false,
        }
    }

    /// 判断序号是否可接受；可接受时同时更新窗口状态。
    pub fn accept(&mut self, seq: u64) -> bool {
        if !self.started {
            self.started = true;
            self.highest = seq;
            self.bitmap = 1;
            return true;
        }

        if seq > self.highest {
            let shift = seq - self.highest;
            self.bitmap = if shift >= 64 {
                1
            } else {
                (self.bitmap << shift) | 1
            };
            self.highest = seq;
            return true;
        }

        let diff = self.highest - seq;
        if diff >= 64 {
            return false;
        }
        let mask = 1u64 << diff;
        if self.bitmap & mask != 0 {
            return false;
        }
        self.bitmap |= mask;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = [7u8; KEY_LEN];
        let cipher = Cipher::new(&key);
        let (nonce, ct) = cipher.seal(b"aad", b"hello plugin").unwrap();
        let pt = cipher.open(b"aad", &nonce, &ct).unwrap();
        assert_eq!(pt, b"hello plugin");
    }

    #[test]
    fn open_rejects_tampered_aad() {
        let key = [9u8; KEY_LEN];
        let cipher = Cipher::new(&key);
        let (nonce, ct) = cipher.seal(b"aad-1", b"payload").unwrap();
        assert!(cipher.open(b"aad-2", &nonce, &ct).is_err());
    }

    #[test]
    fn replay_window_rejects_duplicates() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(10));
        assert!(!w.accept(10));
        assert!(w.accept(11));
        assert!(w.accept(9));
        assert!(!w.accept(9));
    }

    #[test]
    fn replay_window_rejects_too_old() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(1000));
        assert!(!w.accept(1));
    }

    #[test]
    fn derived_keys_differ_per_session() {
        let psk = b"shared-secret";
        let a = SessionKeys::derive(psk, b"salt", b"session-a").unwrap();
        let b = SessionKeys::derive(psk, b"salt", b"session-b").unwrap();
        assert_ne!(a.c2s, b.c2s);
        assert_ne!(a.c2s, a.s2c);
    }
}
