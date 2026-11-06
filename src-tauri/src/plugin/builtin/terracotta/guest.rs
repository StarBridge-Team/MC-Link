//! 访客流程：发现主机 → 建隧道 → 握手 → 把游戏端口接出来。
//!
//! # 为什么要"发现"
//!
//! 访客只知道房间码（网络名 + 密钥），不知道房主的虚拟地址，也不知道房主把
//! Scaffolding 服务端绑在哪个端口。这两样都写在**房主的对端信息**里：
//!
//! - 虚拟地址 → `peer list` 的 `ipv4`;
//! - 服务端口 → 主机名 `scaffolding-mc-server-<port>`。
//!
//! 因此访客必须借 `easytier-cli` 查一次对端（见 `engine/easytier/rpc.rs` 的说明）。
//!
//! # 两次端口转发
//!
//! 1. 隧道口（Scaffolding）：本机随机端口 → `房主IP:服务端口`，用来走陶瓦自己的握手；
//! 2. 游戏口：本机端口 → `房主IP:游戏端口`，端口号是握手拿到的。
//!
//! 第二条只能在握手之后加，这也是为什么"运行期追加端口转发"不能省。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::engine::easytier::{self, Forward, LaunchSpec, Link, Peer, Proto};
use crate::plugin::builtin::support;

use super::host::HOSTNAME_PREFIX;
use super::members::{self, Profile, ProfileKind};
use super::room_code::RoomCode;
use super::scaffold::client::ClientSession;
use super::scaffold::PacketResponse;
use super::shared::Shared;
use super::state::{Phase, Role};

/// 握手用的回显负载。
///
/// 它**不是**令牌或密钥：房主原样回显，访客只是借此确认"这条隧道对面真的在跑
/// 陶瓦服务端"，而不是被别的进程恰好占用了这个端口。
const FINGERPRINT: [u8; 16] = *b"mclink-scaffold!";

/// 等房主上线的最长时间。
///
/// 打洞与公共节点发现都可能慢，120 秒是"用户还愿意等"的上限；超时后给出
/// 明确原因，而不是一直转圈。
const DISCOVER_TIMEOUT: Duration = Duration::from_secs(120);
/// 发现阶段的轮询间隔。
const DISCOVER_TICK: Duration = Duration::from_secs(2);
/// 隧道建立后连接服务端的重试次数（每秒一次）。
const CONNECT_TRIES: u32 = 30;
/// 心跳间隔。
const KEEPALIVE_TICK: Duration = Duration::from_secs(5);

/// 加入房间。
pub fn join(shared: Arc<Shared>, room: &RoomCode, player: String) -> Result<(), String> {
    let data_dir = shared.data_dir.clone();
    let machine_id = shared.snapshot().machine_id;

    // 1) 先入网：此时还不知道主机在哪，因此不建任何转发
    let bins = support::ensure_engine(&data_dir)?;
    let spec = LaunchSpec {
        network_name: room.network_name.clone(),
        network_secret: room.network_secret.clone(),
        hostname: "mclink-guest".to_string(),
        instance_name: "mclink-terracotta".to_string(),
        ipv4: support::guest_ipv4(),
        peers: easytier::PUBLIC_NODES.iter().map(|n| n.to_string()).collect(),
        tcp_whitelist: Vec::new(),
        udp_whitelist: Vec::new(),
        forwards: Vec::new(),
        rpc_port: easytier::free_loopback_port(),
        p2p_only: true,
    };
    let workdir = data_dir
        .join("Engine")
        .join("run")
        .join(format!("terracotta-guest-{}", crate::plugin::crypto::random_hex(4)));
    let link = Link::spawn(&bins, &spec, &workdir)?;
    shared.attach_link(link, workdir);

    // 2) 找房主
    let (host_ip, scaffold_port) = discover(&shared)?;

    // 3) 隧道口：本机随机端口 → 房主的 Scaffolding 服务端
    let tunnel_port = easytier::free_loopback_port();
    add_forward(
        &shared,
        Forward {
            proto: Proto::Tcp,
            bind: SocketAddr::from((Ipv4Addr::LOCALHOST, tunnel_port)),
            dst: SocketAddr::from((host_ip, scaffold_port)),
        },
    )?;

    // 4) 握手：确认对面是陶瓦服务端，再问出游戏端口
    let mut session = connect(tunnel_port)?;
    let game_port = request_game_port(&mut session)?;

    // 5) 游戏口：本机端口 → 房主的游戏端口
    //
    // TCP 与 UDP 都转：原版只用 TCP，但依赖 UDP 的模组（如 SimpleVoiceChat）会按
    // 同一个端口号走 UDP，只转 TCP 时表现为"能进服务器但没有语音"。
    let local_port = support::pick_local_port(game_port);
    for proto in [Proto::Tcp, Proto::Udp] {
        add_forward(
            &shared,
            Forward {
                proto,
                bind: SocketAddr::from((Ipv4Addr::LOCALHOST, local_port)),
                dst: SocketAddr::from((host_ip, game_port)),
            },
        )?;
    }

    // 6) 状态就位（本机档案排在服务端名单之后）
    let local_profile = Profile {
        machine_id: machine_id.clone(),
        name: player.clone(),
        vendor: members::vendor(),
        kind: ProfileKind::Local,
    };
    shared.update(|state| {
        state.phase = Phase::GuestOk;
        state.role = Role::Guest;
        state.room = room.code.clone();
        state.port = Some(local_port);
        state.error = None;
        state.members = vec![local_profile.clone()];
    });
    refresh_members(&mut session, &shared);

    spawn_keepalive(shared, session, local_profile);
    Ok(())
}

/// 在 `peer list` 里找房主：返回（虚拟地址, 服务端口）。
///
/// 判定同时要求两件事，缺一不可：
/// - 主机名以 [`HOSTNAME_PREFIX`] 开头且后面是合法端口号；
/// - 有合法的虚拟地址（没有地址就无法建立端口转发）。
fn find_host(peers: &[Peer]) -> Option<(Ipv4Addr, u16)> {
    peers.iter().find_map(|peer| {
        if peer.is_local() {
            return None;
        }
        let port = peer.service_port(HOSTNAME_PREFIX)?;
        let address = peer.ipv4_addr()?;
        Some((address, port))
    })
}

/// 从 `c:server_port` 的响应里取端口。
fn parse_game_port(response: &PacketResponse) -> Option<u16> {
    if response.status() != 0 {
        return None;
    }
    let data = response.data();
    if data.len() != 2 {
        return None;
    }
    Some(u16::from_be_bytes([data[0], data[1]]))
}

/// 轮询对端列表直到发现房主。
fn discover(shared: &Arc<Shared>) -> Result<(Ipv4Addr, u16), String> {
    let started = Instant::now();
    loop {
        if shared.is_stopped() {
            return Err("已取消加入".to_string());
        }

        let peers = shared.with_link(|link| match link {
            Some(link) => link.peers(),
            None => Err("引擎已停止".to_string()),
        })?;
        if let Some(found) = find_host(&peers) {
            return Ok(found);
        }

        if started.elapsed() > DISCOVER_TIMEOUT {
            return Err(format!(
                "等待房主上线超时（{} 秒）。请确认房间码正确、房主仍在房间内，\
                 且双方网络能连上 EasyTier 公共节点。",
                DISCOVER_TIMEOUT.as_secs()
            ));
        }
        std::thread::sleep(DISCOVER_TICK);
    }
}

/// 连上隧道口并完成回显校验。
fn connect(port: u16) -> Result<ClientSession, String> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    for attempt in 0..CONNECT_TRIES {
        match ClientSession::open(address) {
            Ok(mut session) => {
                let echoed = session.call("c", "ping", |body| body.extend_from_slice(&FINGERPRINT));
                match echoed {
                    Some(response) if response.status() == 0 && response.data() == FINGERPRINT => {
                        return Ok(session)
                    }
                    _ => {
                        // 端口通了但对面不是（或还不是）陶瓦服务端：稍后重试
                        if attempt + 1 == CONNECT_TRIES {
                            return Err(
                                "连上了房主端口，但对方没有按陶瓦协议应答（房间码可能对应到别的服务）"
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            Err(_) if attempt + 1 == CONNECT_TRIES => {
                return Err(format!(
                    "无法连上房主的陶瓦服务端（已重试 {} 次）。请确认房主侧没有被防火墙拦截。",
                    CONNECT_TRIES
                ))
            }
            Err(_) => {}
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err("无法连上房主的陶瓦服务端".to_string())
}

/// 问房主要游戏端口。
fn request_game_port(session: &mut ClientSession) -> Result<u16, String> {
    let response = session
        .call("c", "server_port", |_| {})
        .ok_or_else(|| "房主没有响应端口查询".to_string())?;
    parse_game_port(&response)
        .ok_or_else(|| "房主尚未开放游戏端口（本机游戏可能没有启动）".to_string())
}

/// 拉一次成员名单并写回状态。
fn refresh_members(session: &mut ClientSession, shared: &Arc<Shared>) {
    let Some(data) = session.call_body("c", "player_profiles_list", |_| {}) else {
        return;
    };
    let Some(server_profiles) = members::parse_profiles(&data) else {
        eprintln!("[陶瓦] 成员名单解析失败，保留上一次结果");
        return;
    };

    shared.update(|state| {
        let local: Vec<Profile> = state
            .members
            .iter()
            .filter(|member| member.kind == ProfileKind::Local)
            .cloned()
            .collect();
        let mut next = server_profiles;
        next.extend(local);
        state.members = next;
    });
}

/// 心跳线程：维持成员在线状态，并在断线时把房间标记为异常。
fn spawn_keepalive(shared: Arc<Shared>, mut session: ClientSession, local: Profile) {
    let spawned = std::thread::Builder::new()
        .name("terracotta-guest-keepalive".to_string())
        .spawn(move || loop {
            std::thread::sleep(KEEPALIVE_TICK);
            if shared.is_stopped() {
                return;
            }

            let alive = shared.with_link(|link| link.map(|link| link.is_alive()).unwrap_or(false));
            if !alive {
                shared.fail("EasyTier 已退出，房间连接已断开");
                shared.stop_all();
                return;
            }

            let payload = json!({
                "machine_id": local.machine_id,
                "name": local.name,
                "vendor": local.vendor,
            });
            let sent = session.call("c", "player_ping", |body| {
                if let Ok(bytes) = serde_json::to_vec(&payload) {
                    body.extend_from_slice(&bytes);
                }
            });
            if sent.is_none() {
                shared.fail("与房主的连接已断开");
                shared.stop_all();
                return;
            }

            refresh_members(&mut session, &shared);

            if !session.is_alive() {
                shared.fail("与房主的连接已断开");
                shared.stop_all();
                return;
            }
        });
    if let Err(e) = spawned {
        eprintln!("[陶瓦] 无法启动心跳线程: {}", e);
    }
}

/// 通过引擎追加一条端口转发。
fn add_forward(shared: &Arc<Shared>, forward: Forward) -> Result<(), String> {
    shared.with_link(|link| match link {
        Some(link) => link.add_forward(&forward),
        None => Err("引擎已停止".to_string()),
    })
}

/// 把主机端口写进状态的辅助函数（供诊断用）。
pub fn describe(host: Ipv4Addr, port: u16) -> Value {
    json!({ "host": host.to_string(), "port": port })
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

    #[test]
    fn find_host_needs_both_hostname_and_address() {
        let peers = vec![
            peer("mclink-guest", "10.144.144.2", "Local"),
            peer("scaffolding-mc-server-13448", "10.144.144.1", "p2p"),
        ];
        assert_eq!(
            find_host(&peers),
            Some((Ipv4Addr::new(10, 144, 144, 1), 13448))
        );
    }

    #[test]
    fn find_host_skips_local_and_addressless_peers() {
        let peers = vec![
            peer("scaffolding-mc-server-13448", "10.144.144.9", "Local"),
            peer("scaffolding-mc-server-13448", "", "p2p"),
            peer("mclink-host", "10.144.144.1", "p2p"),
        ];
        assert_eq!(find_host(&peers), None);
    }

    #[test]
    fn parse_game_port_requires_two_bytes_and_ok_status() {
        let ok = PacketResponse::Ok {
            data: 25565u16.to_be_bytes().to_vec(),
        };
        assert_eq!(parse_game_port(&ok), Some(25565));

        let short = PacketResponse::Ok { data: vec![1] };
        assert_eq!(parse_game_port(&short), None);

        let failed = PacketResponse::Fail {
            status: 32,
            data: 25565u16.to_be_bytes().to_vec(),
        };
        assert_eq!(parse_game_port(&failed), None);
    }

    #[test]
    fn describe_is_stable() {
        let value = describe(Ipv4Addr::new(10, 144, 144, 1), 25565);
        assert_eq!(value["host"], "10.144.144.1");
        assert_eq!(value["port"], 25565);
    }
}
