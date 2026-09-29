//! 身份验证存储 — 用于注册流程中暂存已验证身份

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mc_link_common::log::{log, LogLevel};

const IDENTITY_TTL: Duration = Duration::from_secs(600); // 10 分钟

pub struct IdentityEntry {
    pub email: Option<String>,
    pub provider: Option<String>,
    pub provider_id: Option<String>,
    pub name: Option<String>,
    pub created_at: Instant,
}

pub struct IdentityStore {
    tokens: Mutex<HashMap<String, IdentityEntry>>,
}

impl IdentityStore {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// 创建一个身份验证令牌
    pub fn create(&self, entry: IdentityEntry) -> String {
        use rand::Rng;
        let token: String = (0..32)
            .map(|_| rand::thread_rng().sample(rand::distributions::Alphanumeric) as char)
            .collect();

        let mut map = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(token.clone(), entry);
        token
    }

    /// 验证并取出身份信息（一次性消费）
    pub fn consume(&self, token: &str) -> Option<IdentityEntry> {
        let mut map = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        let entry = map.remove(token)?;
        if entry.created_at.elapsed() > IDENTITY_TTL {
            log(LogLevel::Info, "身份令牌已过期");
            None
        } else {
            Some(entry)
        }
    }

    /// 清理过期令牌
    pub fn cleanup(&self) {
        if let Ok(mut map) = self.tokens.lock() {
            let before = map.len();
            map.retain(|_, entry| entry.created_at.elapsed() < IDENTITY_TTL);
            let after = map.len();
            if before != after {
                log(LogLevel::Info, &format!("清理了 {} 个过期身份令牌", before - after));
            }
        }
    }
}