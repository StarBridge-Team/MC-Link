//! 插件身份认证：挑战-应答 + 双向证明 + 失败熔断。
//!
//! # 威胁模型
//!
//! - **同机其它进程冒充插件**：不知道 PSK，无法产出正确的 `proof`。
//! - **同机其它进程冒充核心**：插件会校验核心返回的 `proof_core`，因此
//!   连到一个假核心不会泄露 PSK（证明是 HMAC，不可反推）。
//! - **重放历史握手**：`nonce_s` / `nonce_p` 每次会话重新生成，且派生密钥
//!   的盐值包含双方随机数与会话 ID，历史握手无法复用。
//! - **在线暴力破解**：连续失败达到阈值后该插件进入冷却，核心停止接受其握手。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::plugin::crypto;
use crate::plugin::protocol::Handshake;

/// 认证方式标识。
pub const METHOD_PSK_HMAC: &str = "psk_hmac";

/// 单次握手的挑战数据。
pub struct Challenge {
    pub session_id: String,
    pub plugin_id: String,
    pub nonce_s: String,
    pub nonce_p: String,
    /// 核心期望收到的插件证明（hex）。
    pub expected_plugin_proof: String,
}

/// 生成新挑战。
///
/// 注意：`expected_plugin_proof` 依赖 `nonce_p`，而 `nonce_p` 由插件提供，
/// 因此挑战分两步——先发 `nonce_s`，收到 `nonce_p` 后再计算期望值。
pub fn new_challenge(plugin_id: &str) -> Challenge {
    Challenge {
        session_id: crypto::random_hex(16),
        plugin_id: plugin_id.to_string(),
        nonce_s: crypto::random_hex(32),
        nonce_p: String::new(),
        expected_plugin_proof: String::new(),
    }
}

impl Challenge {
    /// 收到插件随机数后，计算双方应有的证明。
    pub fn finalize(&mut self, psk: &[u8], nonce_p: String) -> (String, String) {
        self.nonce_p = nonce_p;
        let plugin_proof = crypto::hmac_sha256_hex(
            psk,
            Handshake::transcript(
                &self.session_id,
                &self.plugin_id,
                &self.nonce_s,
                &self.nonce_p,
                "plugin",
            )
            .as_bytes(),
        );
        let core_proof = crypto::hmac_sha256_hex(
            psk,
            Handshake::transcript(
                &self.session_id,
                &self.plugin_id,
                &self.nonce_s,
                &self.nonce_p,
                "core",
            )
            .as_bytes(),
        );
        self.expected_plugin_proof = plugin_proof.clone();
        (plugin_proof, core_proof)
    }

    /// 派生本次会话的双向密钥。
    pub fn derive_keys(&self, psk: &[u8]) -> Result<crypto::SessionKeys, String> {
        // 盐值绑定双方随机数：同一 PSK 在不同会话派生不同密钥。
        let salt = format!("{}{}", self.nonce_s, self.nonce_p);
        let transcript = Handshake::transcript(
            &self.session_id,
            &self.plugin_id,
            &self.nonce_s,
            &self.nonce_p,
            "session",
        );
        crypto::SessionKeys::derive(psk, salt.as_bytes(), transcript.as_bytes())
    }

    /// 校验插件证明（常量时间）。
    pub fn verify_plugin_proof(&self, proof: &str) -> bool {
        !self.expected_plugin_proof.is_empty()
            && crypto::ct_eq_hex(&self.expected_plugin_proof, proof)
    }
}

/// 握手失败熔断器。
#[derive(Debug, Default)]
pub struct FailureTracker {
    failures: HashMap<String, (u32, Instant)>,
}

impl FailureTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次失败，返回该插件当前的连续失败次数。
    pub fn record_failure(&mut self, plugin_id: &str) -> u32 {
        let entry = self
            .failures
            .entry(plugin_id.to_string())
            .or_insert((0, Instant::now()));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = Instant::now();
        entry.0
    }

    /// 握手成功，清零计数。
    pub fn record_success(&mut self, plugin_id: &str) {
        self.failures.remove(plugin_id);
    }

    /// 是否处于冷却中。
    pub fn is_cooling_down(&self, plugin_id: &str, max_failures: u32, cooldown: Duration) -> bool {
        match self.failures.get(plugin_id) {
            Some((count, at)) => *count >= max_failures && at.elapsed() < cooldown,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_matches_between_two_parties() {
        let psk = b"pw";
        let mut c = new_challenge("dev.example.a");
        let (plugin_proof, core_proof) = c.finalize(psk, crypto::random_hex(32));
        assert!(c.verify_plugin_proof(&plugin_proof));
        assert_ne!(plugin_proof, core_proof);
        assert!(!c.verify_plugin_proof(&core_proof));
    }

    #[test]
    fn wrong_psk_fails() {
        let mut c = new_challenge("dev.example.a");
        c.finalize(b"right", crypto::random_hex(16));
        let (wrong, _) = {
            let mut other = new_challenge("dev.example.a");
            other.nonce_s = c.nonce_s.clone();
            other.finalize(b"wrong", c.nonce_p.clone())
        };
        assert!(!c.verify_plugin_proof(&wrong));
    }

    #[test]
    fn keys_differ_per_session() {
        let psk = b"pw";
        let mut a = new_challenge("dev.example.a");
        let mut b = new_challenge("dev.example.a");
        a.finalize(psk, "aa".into());
        b.finalize(psk, "bb".into());
        assert_ne!(
            a.derive_keys(psk).unwrap().c2s,
            b.derive_keys(psk).unwrap().c2s
        );
    }

    #[test]
    fn tracker_cools_down_after_threshold() {
        let mut t = FailureTracker::new();
        for _ in 0..5 {
            t.record_failure("dev.x");
        }
        assert!(t.is_cooling_down("dev.x", 5, Duration::from_secs(30)));
        t.record_success("dev.x");
        assert!(!t.is_cooling_down("dev.x", 5, Duration::from_secs(30)));
    }
}
