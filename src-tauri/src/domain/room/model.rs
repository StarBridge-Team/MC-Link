//! 会话模型。
//!
//! 快照字段即前端契约（camelCase）。改字段名等于改前端类型，必须同步改
//! `src/ipc/contracts/room.ts`。

use serde::Serialize;

use crate::core::ids::SessionId;

/// 会话阶段。
///
/// 刻意**不**存"当前第几步"之类的细粒度进度：阶段是状态，进度是事件。
/// 把两者混在一起会让 `rev` 被高频进度刷爆（一次连接能推上千次）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// 已创建，尚未开始调适配器。
    Starting,
    /// 正在建链 / 打洞 / 等对端。
    Linking,
    /// 链路就绪，成员可见。
    Online,
    /// 正在断开（已发命令，等适配器收尾）。
    Stopping,
    /// 已结束（成功断开或失败）。失败原因在 `message`。
    Closed,
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Starting => "starting",
            Phase::Linking => "linking",
            Phase::Online => "online",
            Phase::Stopping => "stopping",
            Phase::Closed => "closed",
        }
    }

    /// 是否仍算"进行中的房间"。列表只展示这一类。
    pub fn is_active(&self) -> bool {
        !matches!(self, Phase::Closed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Host,
    Guest,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Host => "host",
            Role::Guest => "guest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMember {
    pub name: String,
    pub kind: String,
    pub machine_id: String,
    pub is_self: bool,
}

/// 前端看到的会话全量快照。
///
/// 全量而非增量：一个会话撑死几百字节，diff 带来的合并逻辑与漏字段风险不值得。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSnapshot {
    pub session_id: String,
    /// 单调递增版本号。前端用它去重与判序。
    pub rev: u64,
    pub role: Role,
    pub adapter_id: String,
    pub game_id: Option<String>,
    pub phase: Phase,
    pub room_code: String,
    pub members: Vec<RoomMember>,
    pub port: Option<u16>,
    pub message: String,
    pub created_at: u64,
    pub updated_at: u64,
}

/// 会话内部状态。**不实现 Serialize**：对外只能给快照。
#[derive(Debug, Clone)]
pub struct RoomSession {
    pub id: SessionId,
    pub rev: u64,
    pub role: Role,
    pub adapter_id: String,
    pub game_id: Option<String>,
    pub phase: Phase,
    pub room_code: String,
    pub members: Vec<RoomMember>,
    pub port: Option<u16>,
    pub message: String,
    pub created_at: u64,
    pub updated_at: u64,
    /// 进度/日志序号，与 `rev` 独立。
    pub seq: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl RoomSession {
    pub fn new(role: Role, adapter_id: String, game_id: Option<String>) -> Self {
        let now = now_ms();
        Self {
            id: SessionId::generate(),
            rev: 1,
            role,
            adapter_id,
            game_id,
            phase: Phase::Starting,
            room_code: String::new(),
            members: Vec::new(),
            port: None,
            message: String::new(),
            created_at: now,
            updated_at: now,
            seq: 0,
        }
    }

    pub fn snapshot(&self) -> RoomSnapshot {
        RoomSnapshot {
            session_id: self.id.to_string(),
            rev: self.rev,
            role: self.role,
            adapter_id: self.adapter_id.clone(),
            game_id: self.game_id.clone(),
            phase: self.phase,
            room_code: self.room_code.clone(),
            members: self.members.clone(),
            port: self.port,
            message: self.message.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }

    /// 状态推进：`rev` +1 并刷新时间戳。只由 `RoomRegistry::update` 调用。
    pub fn touch(&mut self) {
        self.rev += 1;
        self.updated_at = now_ms();
    }

    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_isolated_from_session() {
        let mut session = RoomSession::new(Role::Host, "adapter.a".into(), None);
        session.room_code = "ABC".into();
        let snapshot = session.snapshot();
        session.room_code = "XYZ".into();
        assert_eq!(snapshot.room_code, "ABC", "快照必须是拷贝");
        assert_eq!(snapshot.rev, 1);
    }

    #[test]
    fn touch_bumps_rev_and_seq_independently() {
        let mut session = RoomSession::new(Role::Guest, "a".into(), None);
        session.next_seq();
        session.next_seq();
        session.touch();
        assert_eq!(session.rev, 2);
        assert_eq!(session.seq, 2);
    }

    #[test]
    fn closed_is_not_active() {
        assert!(Phase::Starting.is_active());
        assert!(!Phase::Closed.is_active());
    }
}
