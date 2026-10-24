//! 领域 ID。
//!
//! 用 newtype 而不是裸 `String`：会话 ID、插件 ID、适配器 ID 在结构上完全同形，
//! 传错一个编译器不会拦。多房间场景下这类错误的表现是"关掉 A 房间把 B 房间断了"，
//! 现场极难定位。包一层之后至少调用点看得见自己在传什么。

use serde::{Deserialize, Serialize};

/// 一次联机会话的 ID（UUID v4）。
///
/// **生命周期**：`room_start_host` / `room_join` 时生成，会话被移除后作废。
/// 同时用于窗口 label（`room-<id>`）与事件路由，因此**必须全局唯一**。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    /// 新会话：UUID v4。
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 从既有字符串还原（读快照、前端回传）。
    ///
    /// 空串直接拒绝：空 ID 会让 `room-` 这种窗口 label 与所有会话撞车。
    pub fn parse(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 对应的窗口 label。房间窗口一会话一个，label 是窗口身份的唯一权威。
    pub fn window_label(&self) -> String {
        format!("room-{}", self.0)
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_is_unique_and_labeled() {
        let a = SessionId::generate();
        let b = SessionId::generate();
        assert_ne!(a, b);
        assert_eq!(a.window_label(), format!("room-{}", a.as_str()));
    }

    #[test]
    fn parse_rejects_blank() {
        assert!(SessionId::parse("").is_none());
        assert!(SessionId::parse("   ").is_none());
        assert_eq!(SessionId::parse(" abc ").unwrap().as_str(), "abc");
    }
}
