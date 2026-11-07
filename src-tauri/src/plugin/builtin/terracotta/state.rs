//! 适配器状态机，以及它到前端契约的投影。
//!
//! # 为什么单独一层
//!
//! 建链过程有后台线程在写（成员变化、进程死亡），适配器方法也要读。把"状态是什么"
//! 与"怎么建链"分开之后，状态投影可以纯单测覆盖——不需要真的起一个 EasyTier 进程。
//!
//! # `phase` 的取值是前端契约
//!
//! [`crate::mgr::room::RoomMgr::refresh`] 用 `phase.ends_with("-ok")` 判断"这条链路
//! 可用"。写成别的前缀（例如 `connected`）不会报错，只会让房间永远停在"连接中"。

use serde_json::{json, Value};

use super::members::{self, Profile};

/// 适配器所处的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 空闲。
    Waiting,
    /// 房主侧已就绪。
    HostOk,
    /// 访客侧已就绪。
    GuestOk,
    /// 出错或链路已断。
    Exception,
}

impl Phase {
    /// 对前端暴露的取值。
    pub fn wire_name(self) -> &'static str {
        match self {
            Phase::Waiting => "waiting",
            Phase::HostOk => "host-ok",
            Phase::GuestOk => "guest-ok",
            Phase::Exception => "exception",
        }
    }

    /// 链路是否可用。
    pub fn is_ok(self) -> bool {
        matches!(self, Phase::HostOk | Phase::GuestOk)
    }
}

/// 房间角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Host,
    Guest,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Host => "host",
            Role::Guest => "guest",
        }
    }
}

/// 一次联机的全部可观测状态。
#[derive(Debug, Clone)]
pub struct SessionState {
    pub phase: Phase,
    pub role: Role,
    /// 展示用的房间码（`U/XXXX-…`）。
    pub room: String,
    /// 房主侧=本机游戏端口；访客侧=本机接出端口。
    pub port: Option<u16>,
    /// 成员档案。
    pub members: Vec<Profile>,
    /// 出错原因（`phase == Exception` 时给用户看）。
    pub error: Option<String>,
    /// 本机机器标识：用于把 `self` 标在成员列表里。
    pub machine_id: String,
}

impl SessionState {
    /// 空闲态。
    pub fn idle(machine_id: String) -> Self {
        Self {
            phase: Phase::Waiting,
            role: Role::Host,
            room: String::new(),
            port: None,
            members: Vec::new(),
            error: None,
            machine_id,
        }
    }

    /// 投影成 `adapter.status` 的返回值。
    pub fn status_value(&self, engine: &str) -> Value {
        json!({
            "installed": engine == "ready",
            "running": self.phase.is_ok(),
            "port": self.port,
            "phase": self.phase.wire_name(),
            "room": self.room,
            "players": members::to_players(&self.members, &self.machine_id),
            "engine": engine,
            "role": self.role.as_str(),
            // 只在出错时出现，避免前端拿到一个恒为 null 的字段
            "error": self.error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::builtin::terracotta::members::ProfileKind;

    fn state(phase: Phase) -> SessionState {
        SessionState {
            phase,
            role: Role::Host,
            room: "U/7000-0000-0000-0000".to_string(),
            port: Some(25565),
            members: vec![Profile {
                machine_id: "aa".to_string(),
                name: "Steve".to_string(),
                vendor: "test".to_string(),
                kind: ProfileKind::Host,
            }],
            error: None,
            machine_id: "aa".to_string(),
        }
    }

    #[test]
    fn wire_names_are_stable() {
        assert_eq!(Phase::Waiting.wire_name(), "waiting");
        assert_eq!(Phase::HostOk.wire_name(), "host-ok");
        assert_eq!(Phase::GuestOk.wire_name(), "guest-ok");
        assert_eq!(Phase::Exception.wire_name(), "exception");
        assert!(Phase::HostOk.is_ok() && Phase::GuestOk.is_ok());
        assert!(!Phase::Waiting.is_ok() && !Phase::Exception.is_ok());
    }

    #[test]
    fn status_value_matches_room_contract() {
        let value = state(Phase::HostOk).status_value("ready");
        assert_eq!(value["phase"], "host-ok");
        assert_eq!(value["running"], true);
        assert_eq!(value["installed"], true);
        assert_eq!(value["port"], 25565);
        assert_eq!(value["room"], "U/7000-0000-0000-0000");
        assert_eq!(value["players"][0]["self"], true);
        assert!(value["error"].is_null());
    }

    #[test]
    fn non_ready_engine_is_reported_as_not_installed() {
        let value = state(Phase::Waiting).status_value("downloading");
        assert_eq!(value["installed"], false);
        assert_eq!(value["engine"], "downloading");
        assert_eq!(value["running"], false);
    }

    #[test]
    fn error_surfaces_when_present() {
        let mut value_state = state(Phase::Exception);
        value_state.error = Some("房主已离线".to_string());
        let value = value_state.status_value("ready");
        assert_eq!(value["error"], "房主已离线");
        assert_eq!(value["running"], false);
    }
}
