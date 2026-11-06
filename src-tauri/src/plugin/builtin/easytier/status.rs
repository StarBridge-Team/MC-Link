//! 状态投影：把"当前链路 + 引擎状态"翻译成 `adapter.status` 的负载。
//!
//! 单独成文件的两个理由：
//!
//! 1. 它是**纯函数**（输入全是值），可以脱离进程与锁单测；
//! 2. `phase` 的取值是前端契约（`mgr/room.rs` 按 `ends_with("-ok")` 判断可用），
//!    把它和建链逻辑混在一起时，改一个文案就可能悄悄改坏判定。

use serde_json::{json, Value};

use crate::engine::easytier::Peer;

/// 一次状态投影所需的全部输入。
pub(super) struct View<'a> {
    /// 引擎状态：`ready` / `downloading` / `missing`。
    pub engine: &'a str,
    /// 内核进程是否还活着。
    pub alive: bool,
    /// `host` / `guest`。
    pub role: &'a str,
    /// 本机玩家名（用于把 `self` 对上人）。
    pub player_name: &'a str,
    /// 展示用房间码。
    pub room: String,
    /// 房主侧=游戏端口；访客侧=本机接出端口。
    pub local_port: Option<u16>,
    /// 当前对端列表。
    pub peers: &'a [Peer],
}

/// 没有链路时的状态。
pub(super) fn idle_value(engine: &str) -> Value {
    json!({
        "installed": engine == "ready",
        "running": false,
        "port": Value::Null,
        "phase": "waiting",
        "room": "",
        "players": [],
        "engine": engine,
    })
}

/// 有链路时的状态。
pub(super) fn status_value(view: View<'_>) -> Value {
    let players: Vec<Value> = view
        .peers
        .iter()
        .map(|peer| {
            let is_self = peer.is_local();
            json!({
                "name": if is_self { view.player_name.to_string() } else { peer.hostname.clone() },
                "kind": if is_self { "LOCAL" } else { peer_kind(view.role) },
                // 虚拟地址在房间内唯一，用它区分同名玩家
                "machineId": peer.ipv4,
                "self": is_self,
            })
        })
        .collect();

    json!({
        "installed": view.engine == "ready",
        "running": view.alive,
        "port": view.local_port,
        // 进程没了就是异常：让房间窗口把"已断开"显示出来，而不是继续转圈
        "phase": if view.alive { ok_phase(view.role) } else { "exception" },
        "room": view.room,
        "players": players,
        "engine": view.engine,
    })
}

/// 角色 → 状态机取值。
fn ok_phase(role: &str) -> &'static str {
    if role == "host" {
        "host-ok"
    } else {
        "guest-ok"
    }
}

/// 对端在成员列表里的 `kind`。
fn peer_kind(role: &str) -> &'static str {
    if role == "host" {
        "GUEST"
    } else {
        "HOST"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(hostname: &str, ipv4: &str, cost: &str) -> Peer {
        Peer {
            hostname: hostname.to_string(),
            ipv4: ipv4.to_string(),
            cost: cost.to_string(),
        }
    }

    fn view<'a>(role: &'a str, peers: &'a [Peer], alive: bool) -> View<'a> {
        View {
            engine: "ready",
            alive,
            role,
            player_name: "Steve",
            room: "ABCD-EFGH-JKLM-2556".to_string(),
            local_port: Some(25565),
            peers,
        }
    }

    #[test]
    fn idle_value_reports_waiting() {
        let value = idle_value("missing");
        assert_eq!(value["phase"], "waiting");
        assert_eq!(value["running"], false);
        assert_eq!(value["installed"], false);
        assert_eq!(value["engine"], "missing");
    }

    #[test]
    fn host_status_marks_self_and_guests() {
        let peers = [
            peer("mclink-host", "10.144.144.1", "Local"),
            peer("mclink-guest", "10.144.144.9", "p2p"),
        ];
        let value = status_value(view("host", &peers, true));
        assert_eq!(value["phase"], "host-ok");
        assert_eq!(value["players"][0]["name"], "Steve");
        assert_eq!(value["players"][0]["self"], true);
        assert_eq!(value["players"][1]["kind"], "GUEST");
        assert_eq!(value["players"][1]["machineId"], "10.144.144.9");
    }

    #[test]
    fn guest_status_labels_peers_as_host() {
        let peers = [peer("mclink-host", "10.144.144.1", "p2p")];
        let value = status_value(view("guest", &peers, true));
        assert_eq!(value["phase"], "guest-ok");
        assert_eq!(value["players"][0]["kind"], "HOST");
    }

    #[test]
    fn dead_process_reports_exception() {
        let value = status_value(view("host", &[], false));
        assert_eq!(value["phase"], "exception");
        assert_eq!(value["running"], false);
    }

    #[test]
    fn phase_suffix_is_the_contract() {
        // `mgr/room.rs` 按 `ends_with("-ok")` 判断可用，这里把它钉住
        assert!(ok_phase("host").ends_with("-ok"));
        assert!(ok_phase("guest").ends_with("-ok"));
        assert_ne!(ok_phase("host"), "connected");
    }
}
