//! 会话管理 — 用户登录后的会话存储

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::oauth::OAuthUser;

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub user: OAuthUser,
    pub created_at: u64,
}

/// 会话管理器
pub struct SessionStore {
    sessions: Mutex<HashMap<String, SessionInfo>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// 创建新会话，返回 session_id
    pub fn create(&self, user: OAuthUser) -> String {
        let session_id = uuid::Uuid::new_v4().to_string().replace('-', "");
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let info = SessionInfo { session_id: session_id.clone(), user, created_at };
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        // 清理过期会话（24小时）
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(86400);
        sessions.retain(|_, s| s.created_at >= cutoff);
        sessions.insert(session_id.clone(), info);
        session_id
    }

    /// 获取会话信息
    pub fn get(&self, session_id: &str) -> Option<SessionInfo> {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.get(session_id).cloned()
    }

    /// 删除会话
    pub fn remove(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.remove(session_id);
    }

    /// 清理过期会话（超过24小时）
    pub fn cleanup(&self) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(86400);
        sessions.retain(|_, s| s.created_at >= cutoff);
    }
}