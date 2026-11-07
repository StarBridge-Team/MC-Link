//! EasyTier 的运行时数据模型。
//!
//! 字段名与上游 `easytier-cli -o json peer list` 的输出**逐字对应**（v2.6.4 实测，
//! 见 `engine/easytier/mod.rs` 的模块注释）。上游改名会让 [`Peer::hostname`] 之类的
//! 读取静默变成空串，因此这里对每个字段都做了容错默认值：解析失败退化为"看不到这个
//! 字段"，而不是让整次查询失败。

use serde::Deserialize;
use std::net::Ipv4Addr;
use std::str::FromStr;

/// `easytier-cli -o json peer list` 数组里的一项。
///
/// 只声明我们真正会读的字段：其余（`lat_ms` / `rx_bytes` / `id` …）在本模块里没有
/// 消费者，多声明一份就等于多一份"上游改名后没人发现"的静默错误面。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Peer {
    /// 对端主机名。陶瓦用它携带"主机侧服务端口"（`mclink-server-<port>`）。
    #[serde(default)]
    pub hostname: String,
    /// 对端在虚拟网络里的 IPv4（未分配时为空串）。
    #[serde(default)]
    pub ipv4: String,
    /// 连接代价：`Local` 表示本机自己，其余（`p2p` / `relay` …）是对端。
    #[serde(default)]
    pub cost: String,
}

impl Peer {
    /// 本机自己那一项：`cost == "Local"`。
    pub fn is_local(&self) -> bool {
        self.cost.eq_ignore_ascii_case("local")
    }

    /// 对端的虚拟 IPv4；为空或非法时返回 `None`。
    pub fn ipv4_addr(&self) -> Option<Ipv4Addr> {
        Ipv4Addr::from_str(self.ipv4.trim()).ok()
    }

    /// 对端是否就是某个"某类主机"：主机名以 `prefix` 开头时返回其后的端口。
    ///
    /// 陶瓦的约定是 `scaffolding-mc-server-<port>`：端口写在主机名里，因为
    /// **在没有 RPC 的情况下**（端口转发还没建立时）主机名是唯一能跨机器读到的
    /// 元数据。
    pub fn service_port(&self, prefix: &str) -> Option<u16> {
        let rest = self.hostname.strip_prefix(prefix)?;
        rest.parse::<u16>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Vec<Peer> {
        serde_json::from_str::<Vec<Peer>>(json).expect("应能解析 peer 数组")
    }

    #[test]
    fn parses_real_upstream_payload() {
        // 取自 v2.6.4 实测输出（字段全部齐备）
        let peers = parse(
            r#"[{"cidr":"10.144.144.1/24","ipv4":"10.144.144.1","hostname":"host-node",
                 "cost":"Local","lat_ms":"-","loss_rate":"-","rx_bytes":"-","tx_bytes":"-",
                 "tunnel_proto":"-","nat_type":"PortRestricted","id":"664560844",
                 "version":"2.6.4-8428a89d"}]"#,
        );
        assert_eq!(peers.len(), 1);
        assert!(peers[0].is_local());
        assert_eq!(peers[0].ipv4_addr(), Some(Ipv4Addr::new(10, 144, 144, 1)));
    }

    #[test]
    fn tolerates_missing_and_empty_fields() {
        // 上游在"未分配 IP"时给的是空串；字段缺失也不该让整次查询失败
        let peers = parse(r#"[{"hostname":"guest","ipv4":"","cost":"p2p"}]"#);
        assert_eq!(peers[0].ipv4_addr(), None);
        assert!(!peers[0].is_local());
        assert_eq!(peers[0].hostname, "guest");
    }

    #[test]
    fn service_port_reads_trailing_number() {
        let peer = Peer {
            hostname: "scaffolding-mc-server-25565".to_string(),
            ..Default::default()
        };
        assert_eq!(peer.service_port("scaffolding-mc-server-"), Some(25565));
        assert_eq!(peer.service_port("mclink-server-"), None);
        assert_eq!(
            Peer {
                hostname: "scaffolding-mc-server-abc".to_string(),
                ..Default::default()
            }
            .service_port("scaffolding-mc-server-"),
            None
        );
    }
}
