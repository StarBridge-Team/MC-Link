//! 房主流程：起服务端 → 起引擎 → 维护成员。
//!
//! # 顺序为什么不能反
//!
//! 主机名里要写 Scaffolding 服务端的端口（`scaffolding-mc-server-<port>`），
//! 而主机名是**访客唯一能提前读到的元数据**（端口转发尚未建立时只能靠它）。
//! 所以必须先把服务端绑好、拿到端口，再拉起引擎。
//!
//! # 服务端的三个方法
//!
//! | 方法 | 作用 | 上游是否必需 |
//! |---|---|---|
//! | `c:ping` | 回显 | 是（访客用它确认"对面真的是陶瓦主机"） |
//! | `c:server_port` | 返回本机游戏端口 | 是 |
//! | `c:player_ping` / `c:player_profiles_list` | 成员心跳与名单 | 是 |
//! | `c:protocols` | 能力列表 | 否（但保留，便于对端探测） |

use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::engine::easytier::{self, LaunchSpec, Link};
use crate::plugin::builtin::support;

use super::members::{self, Profile, ProfileKind};
use super::room_code::RoomCode;
use super::scaffold::server::{Handler, ScaffoldServer};
use super::scaffold::{Frame, PacketResponse, STATUS_UNIMPLEMENTED};
use super::shared::Shared;
use super::state::{Phase, Role};

/// Scaffolding 服务端的首选端口（与上游默认一致）。
pub const SCAFFOLD_PORT: u16 = 13448;

/// 主机名里的前缀：端口写在它后面，访客据此发现服务端口。
pub const HOSTNAME_PREFIX: &str = "scaffolding-mc-server-";

/// 本端实现的方法列表。
const PROTOCOLS: [&str; 5] = [
    "c:ping",
    "c:protocols",
    "c:server_port",
    "c:player_ping",
    "c:player_profiles_list",
];

/// 成员心跳过期时间：超过它没 `c:player_ping` 就从名单里摘掉。
const MEMBER_TTL: Duration = Duration::from_secs(10);
/// 维护线程的检查间隔。
const TICK: Duration = Duration::from_secs(5);

/// 开一间房。
pub fn start(
    shared: Arc<Shared>,
    room: &RoomCode,
    player: String,
    game_port: u16,
) -> Result<(), String> {
    let data_dir = shared.data_dir.clone();
    let machine_id = shared.snapshot().machine_id;

    // 1) 先绑服务端：端口要写进主机名
    let handler: Handler = {
        let shared = shared.clone();
        Arc::new(move |frame: &Frame| dispatch(&shared, frame))
    };
    let server = ScaffoldServer::start(SCAFFOLD_PORT, handler.clone())
        // 13448 被占是很正常的事（用户可能自己跑着陶瓦）；此时让系统分配，
        // 端口照样写进主机名，互通性不受影响
        .or_else(|e| {
            eprintln!("[陶瓦] 端口 {} 不可用（{}），改用随机端口", SCAFFOLD_PORT, e);
            ScaffoldServer::start(0, handler.clone())
        })
        .map_err(|e| format!("无法启动陶瓦服务端: {}", e))?;
    let scaffold_port = server.port();
    shared.attach_server(server);

    // 2) 起引擎
    let bins = support::ensure_engine(&data_dir)?;
    let spec = LaunchSpec {
        network_name: room.network_name.clone(),
        network_secret: room.network_secret.clone(),
        hostname: format!("{}{}", HOSTNAME_PREFIX, scaffold_port),
        instance_name: "mclink-terracotta".to_string(),
        ipv4: support::HOST_IPV4,
        peers: easytier::PUBLIC_NODES.iter().map(|n| n.to_string()).collect(),
        // 访客要能通过隧道访问 Scaffolding 服务端与游戏端口
        tcp_whitelist: vec![scaffold_port, game_port],
        udp_whitelist: vec![game_port],
        forwards: Vec::new(),
        rpc_port: easytier::free_loopback_port(),
        // 与上游一致：只走 P2P，避免公共节点承担中继流量
        p2p_only: true,
    };

    let workdir = data_dir
        .join("Engine")
        .join("run")
        .join(format!("terracotta-host-{}", crate::plugin::crypto::random_hex(4)));
    let link = Link::spawn(&bins, &spec, &workdir).map_err(|e| {
        // 引擎起不来时服务端已经绑着了，必须回滚，否则下次建房会撞端口
        shared.stop_all();
        e
    })?;
    shared.attach_link(link, workdir);

    // 3) 状态就位
    shared.update(|session| {
        session.phase = Phase::HostOk;
        session.role = Role::Host;
        session.room = room.code.clone();
        session.port = Some(game_port);
        session.error = None;
        session.members = vec![Profile {
            machine_id: machine_id.clone(),
            name: player,
            vendor: members::vendor(),
            kind: ProfileKind::Host,
        }];
    });
    shared.touch(&machine_id);

    spawn_maintenance(shared);
    Ok(())
}

/// 维护线程：探活 + 清理掉线成员。
fn spawn_maintenance(shared: Arc<Shared>) {
    let spawned = std::thread::Builder::new()
        .name("terracotta-host-keepalive".to_string())
        .spawn(move || loop {
            std::thread::sleep(TICK);
            if shared.is_stopped() {
                return;
            }
            let alive = shared.with_link(|link| link.map(|link| link.is_alive()).unwrap_or(false));
            if !alive {
                shared.fail("主机侧 EasyTier 已退出，房间已关闭");
                shared.stop_all();
                return;
            }
            shared.expire_members(MEMBER_TTL);
        });
    if let Err(e) = spawned {
        eprintln!("[陶瓦] 无法启动维护线程: {}", e);
    }
}

/// 分派一帧请求。
fn dispatch(shared: &Arc<Shared>, frame: &Frame) -> io::Result<PacketResponse> {
    if frame.namespace != "c" {
        return PacketResponse::fail(STATUS_UNIMPLEMENTED, b"Unknown namespace".to_vec());
    }

    match frame.path.as_str() {
        // 回显：访客用它确认对端身份
        "ping" => PacketResponse::ok(frame.body.clone()),
        "protocols" => PacketResponse::ok(PROTOCOLS.join("\0").into_bytes()),
        "server_port" => {
            let session = shared.snapshot();
            match (session.phase, session.port) {
                (Phase::HostOk, Some(port)) => PacketResponse::ok(port.to_be_bytes().to_vec()),
                _ => PacketResponse::fail(32, b"Host is not ready".to_vec()),
            }
        }
        "player_ping" => player_ping(shared, &frame.body),
        "player_profiles_list" => {
            let session = shared.snapshot();
            let value = members::encode_profiles(&session.members);
            PacketResponse::ok(serde_json::to_vec(&value).unwrap_or_else(|_| b"[]".to_vec()))
        }
        _ => PacketResponse::fail(
            STATUS_UNIMPLEMENTED,
            b"Requested protocol hasn't been implemented.".to_vec(),
        ),
    }
}

/// 处理成员心跳：登记或更新一个访客档案。
fn player_ping(shared: &Arc<Shared>, body: &[u8]) -> io::Result<PacketResponse> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;

    let read = |key: &str| value.get(key).and_then(|v| v.as_str()).unwrap_or("");
    let name = read("name");
    let machine_id = read("machine_id");
    let vendor = read("vendor");
    if machine_id.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "缺少 machine_id",
        ));
    }
    if machine_id == shared.snapshot().machine_id {
        // 与本机重名意味着身份冲突，继续下去成员列表会分不清谁是谁
        return Err(io::Error::other(
            "machine_id 冲突：不能与本机相同",
        ));
    }

    // 需要先取 host 是否就绪，避免把访客塞进一个已经关闭的房间
    let session = shared.snapshot();
    if session.phase != Phase::HostOk {
        return PacketResponse::fail(32, b"Host is not ready".to_vec());
    }

    shared.touch(machine_id);
    shared.update(|session| {
        match session
            .members
            .iter_mut()
            .find(|member| member.machine_id == machine_id)
        {
            Some(existing) => existing.name = name.to_string(),
            None => session.members.push(Profile {
                machine_id: machine_id.to_string(),
                name: name.to_string(),
                vendor: vendor.to_string(),
                kind: ProfileKind::Guest,
            }),
        }
    });

    PacketResponse::ok(Vec::new())
}

/// 访问本机游戏端口是否可用（供调用方在报"已连接"前做一次确认）。
pub fn probe_game_port(game_port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from((Ipv4Addr::LOCALHOST, game_port)),
        Duration::from_secs(2),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::builtin::terracotta::state::SessionState;

    fn shared_host() -> Arc<Shared> {
        let dir = std::env::temp_dir();
        let session = SessionState {
            phase: Phase::HostOk,
            role: Role::Host,
            room: "U/7000-0000-0000-0000".to_string(),
            port: Some(25565),
            members: vec![Profile {
                machine_id: "host-id".to_string(),
                name: "Host".to_string(),
                vendor: "test".to_string(),
                kind: ProfileKind::Host,
            }],
            error: None,
            machine_id: "host-id".to_string(),
        };
        Shared::new(&dir, session)
    }

    fn frame(path: &str, body: &[u8]) -> Frame {
        Frame {
            namespace: "c".to_string(),
            path: path.to_string(),
            body: body.to_vec(),
        }
    }

    #[test]
    fn ping_echoes_body() {
        let shared = shared_host();
        let response = dispatch(&shared, &frame("ping", b"fingerprint")).unwrap();
        assert_eq!(response.status(), 0);
        assert_eq!(response.data(), b"fingerprint");
    }

    #[test]
    fn server_port_returns_big_endian_u16() {
        let shared = shared_host();
        let response = dispatch(&shared, &frame("server_port", b"")).unwrap();
        assert_eq!(response.data(), 25565u16.to_be_bytes());

        shared.update(|session| session.phase = Phase::Exception);
        let response = dispatch(&shared, &frame("server_port", b"")).unwrap();
        assert_eq!(response.status(), 32);
    }

    #[test]
    fn protocols_are_nul_separated() {
        let shared = shared_host();
        let response = dispatch(&shared, &frame("protocols", b"")).unwrap();
        let text = String::from_utf8(response.data().to_vec()).unwrap();
        assert_eq!(text, "c:ping\0c:protocols\0c:server_port\0c:player_ping\0c:player_profiles_list");
    }

    #[test]
    fn player_ping_registers_guest_once() {
        let shared = shared_host();
        let body = br#"{"name":"Steve","machine_id":"guest-1","vendor":"Terracotta"}"#;

        let first = dispatch(&shared, &frame("player_ping", body)).unwrap();
        assert_eq!(first.status(), 0);
        let second = dispatch(&shared, &frame("player_ping", body)).unwrap();
        assert_eq!(second.status(), 0, "重复心跳不应报错");

        let session = shared.snapshot();
        assert_eq!(session.members.len(), 2, "同一 machine_id 不应重复登记");
        assert_eq!(session.members[1].kind, ProfileKind::Guest);
    }

    #[test]
    fn player_ping_renames_existing_member() {
        let shared = shared_host();
        dispatch(
            &shared,
            &frame(
                "player_ping",
                br#"{"name":"Steve","machine_id":"guest-1","vendor":"v"}"#,
            ),
        )
        .unwrap();
        dispatch(
            &shared,
            &frame(
                "player_ping",
                br#"{"name":"Alex","machine_id":"guest-1","vendor":"v"}"#,
            ),
        )
        .unwrap();
        let session = shared.snapshot();
        assert_eq!(session.members[1].name, "Alex");
    }

    #[test]
    fn player_ping_rejects_host_machine_id() {
        let shared = shared_host();
        let body = br#"{"name":"Fake","machine_id":"host-id","vendor":"v"}"#;
        assert!(dispatch(&shared, &frame("player_ping", body)).is_err());
    }

    #[test]
    fn unknown_method_is_unimplemented() {
        let shared = shared_host();
        let response = dispatch(&shared, &frame("nope", b"")).unwrap();
        assert_eq!(response.status(), STATUS_UNIMPLEMENTED);
    }

    #[test]
    fn profiles_list_matches_wire_shape() {
        let shared = shared_host();
        let response = dispatch(&shared, &frame("player_profiles_list", b"")).unwrap();
        let parsed: Value = serde_json::from_slice(response.data()).unwrap();
        assert_eq!(parsed[0]["kind"], "HOST");
        assert_eq!(parsed[0]["machine_id"], "host-id");
    }
}
