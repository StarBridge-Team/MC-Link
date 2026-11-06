//! 两个内置适配器共用的小工具。
//!
//! 只放"与具体协议无关"的碎片：引擎就位、虚拟地址、本机端口挑选、成员 JSON 形状。
//! 任何与陶瓦协议或 EasyTier 网络相关的东西都不该进来——它属于各自的适配器。

use std::net::{Ipv4Addr, TcpListener};

use crate::engine::easytier::{self, Binaries};

/// 陶瓦约定的主机虚拟地址（上游同样把主机固定在这里）。
pub const HOST_IPV4: Ipv4Addr = Ipv4Addr::new(10, 144, 144, 1);

/// 确保引擎可用，失败时给出**可操作**的文案。
pub fn ensure_engine(data_dir: &std::path::Path) -> Result<Binaries, String> {
    easytier::ensure_blocking(data_dir, |_| {})
        .map_err(|e| format!("EasyTier 引擎不可用：{}", e))
}

/// 给访客分配一个不冲突的虚拟地址。
///
/// 跳过 `.0`（网段地址）、`.1`（主机固定占用）与 `.255`（广播）。
/// 地址在**每次建链时随机**：`--no-tun` 下 DHCP 不生效（实测 `ipv4` 恒为空），
/// 只能自己挑；随机比"固定 .2"更不容易与同房间的其他访客撞车。
pub fn guest_ipv4() -> Ipv4Addr {
    let byte = u8::from_str_radix(&crate::plugin::crypto::random_hex(1), 16).unwrap_or(2);
    Ipv4Addr::new(10, 144, 144, 2 + (byte % 253))
}

/// 挑一个本机端口：优先用 `preferred`，被占则退回系统分配的空闲端口。
///
/// 为什么要"优先同号"：有些模组（如 SimpleVoiceChat）会按游戏端口 +1 使用 UDP，
/// 本机端口与主机端口一致时这类模组的兼容性最好。
pub fn pick_local_port(preferred: u16) -> u16 {
    if preferred >= 1024 && TcpListener::bind((Ipv4Addr::LOCALHOST, preferred)).is_ok() {
        return preferred;
    }
    easytier::free_loopback_port()
}

/// 把本机监听地址渲染成给用户看的入口串（与陶瓦 `/state` 的 `url` 字段同形）。
pub fn url_for(port: u16) -> String {
    if port == 25565 {
        "127.0.0.1".to_string()
    } else {
        format!("127.0.0.1:{}", port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_ip_is_inside_the_virtual_subnet() {
        assert_eq!(HOST_IPV4.octets()[0..3], [10, 144, 144]);
        assert_eq!(HOST_IPV4.octets()[3], 1);
    }

    #[test]
    fn guest_ip_never_collides_with_host_or_broadcast() {
        for _ in 0..200 {
            let octets = guest_ipv4().octets();
            assert_eq!(octets[0..3], [10, 144, 144]);
            assert!(octets[3] >= 2, "不能占用主机地址: {:?}", octets);
            assert!(octets[3] <= 254, "不能占用广播地址: {:?}", octets);
        }
    }

    #[test]
    fn pick_local_port_prefers_requested_when_free() {
        // 先占一个高位端口，确认它被占时会换一个
        let holder = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let taken = holder.local_addr().unwrap().port();
        let picked = pick_local_port(taken);
        assert_ne!(picked, taken, "端口被占时应换一个");
        assert!(picked > 0);
    }

    #[test]
    fn url_for_default_minecraft_port_is_bare_host() {
        assert_eq!(url_for(25565), "127.0.0.1");
        assert_eq!(url_for(25566), "127.0.0.1:25566");
    }
}
