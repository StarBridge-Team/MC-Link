//! 启动参数构造：把"要一个什么样的房间"翻译成 `easytier-core` 的命令行。
//!
//! # 参数名以 v2.6.4 的 `easytier-core --help` 为准
//!
//! 参考实现（陶瓦 v2.5 那一代）用的是一份改过的内核，`-p` 当时表示"公共服务器"；
//! v2.6.4 里 `-p/--peers` 表示"最初要连接的对等节点"，语义相近但**参数名对不上**。
//! 这里逐项对照过 `easytier-core --help` 与 `easytier/src/core.rs` 的 `clap` 定义，
//! 不做"照着抄一遍"的移植。
//!
//! # 为什么固定 `--no-tun`
//!
//! 带 TUN 需要管理员/root 权限，而本应用刻意不申请提权；`--no-tun` 下用
//! **端口转发**访问对端服务，正好满足"只把一个游戏端口接出来"的需求。
//! 代价是必须自己指定 `--ipv4`（实测 `--no-tun` 下 DHCP 不会分配地址，
//! `peer list` 里的 `ipv4` 恒为空，端口转发因此打不通）。

use std::net::{Ipv4Addr, SocketAddr};

/// 传输层协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    /// 上游 URL 里使用的协议名。
    pub fn as_str(self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        }
    }
}

/// 一条端口转发：本机 `bind` ↔ 虚拟网络内 `dst`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Forward {
    pub proto: Proto,
    pub bind: SocketAddr,
    pub dst: SocketAddr,
}

impl Forward {
    /// 上游格式：`<proto>://<bind>/<dst>`（注意目的地址**不带** `//`）。
    ///
    /// 例：`tcp://127.0.0.1:25565/10.144.144.1:25565`
    pub fn to_url(&self) -> String {
        format!("{}://{}/{}", self.proto.as_str(), self.bind, self.dst)
    }
}

/// 一次建链的完整参数。
#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub network_name: String,
    pub network_secret: String,
    /// 本节点在房间内的标识；陶瓦约定主机侧写成 `scaffolding-mc-server-<port>`，
    /// 这样访客能在端口转发建立**之前**从 peer 列表里读到主机端口。
    pub hostname: String,
    /// 同机多实例时的区分名（本应用一次只跑一个实例，但日志里能看出是谁）。
    pub instance_name: String,
    /// 本节点虚拟 IPv4。**必须显式给**，见模块注释。
    pub ipv4: Ipv4Addr,
    /// 初始对端（公共节点或已知对端）。
    pub peers: Vec<String>,
    /// 允许对端通过端口转发访问的本机端口。
    pub tcp_whitelist: Vec<u16>,
    pub udp_whitelist: Vec<u16>,
    /// 启动时即建立的端口转发。
    pub forwards: Vec<Forward>,
    /// 本进程 RPC 门户端口（仅回环）：`easytier-cli` 靠它做管理。
    pub rpc_port: u16,
    /// 只与已建立 P2P 的对端通信。
    ///
    /// 陶瓦上游开启它（避免公共节点承担中继流量）；通用适配器不开启（中继兜底更稳）。
    pub p2p_only: bool,
}

impl LaunchSpec {
    /// 构造一组命令行参数。
    pub fn to_args(&self) -> Vec<String> {
        /// 追加 `flag value`。
        fn pair(args: &mut Vec<String>, flag: &str, value: &str) {
            args.push(flag.to_string());
            args.push(value.to_string());
        }
        /// 追加无值开关。
        fn switch(args: &mut Vec<String>, flag: &str) {
            args.push(flag.to_string());
        }

        let mut args: Vec<String> = Vec::with_capacity(40);

        pair(&mut args, "--network-name", &self.network_name);
        pair(&mut args, "--network-secret", &self.network_secret);
        pair(&mut args, "--hostname", &self.hostname);
        pair(&mut args, "--instance-name", &self.instance_name);
        pair(&mut args, "--ipv4", &self.ipv4.to_string());

        // 只监听随机端口：不占固定端口，且让打洞用到的映射随每次建链刷新
        pair(&mut args, "--listeners", "tcp://0.0.0.0:0");
        pair(&mut args, "--listeners", "udp://0.0.0.0:0");

        // 无 TUN：靠端口转发接出游戏端口（见模块注释）
        switch(&mut args, "--no-tun");

        // 与陶瓦同款的质量取向：压缩、多线程、低延迟优先、KCP 代理（丢包链路收益明显）
        pair(&mut args, "--compression", "zstd");
        switch(&mut args, "--multi-thread");
        switch(&mut args, "--latency-first");
        switch(&mut args, "--enable-kcp-proxy");

        if self.p2p_only {
            switch(&mut args, "--p2p-only");
        }

        pair(&mut args, "--rpc-portal", &format!("127.0.0.1:{}", self.rpc_port));

        // 日志级别压到 warn：核心自身会把每次连接细节打满，塞进文件也没人看
        pair(&mut args, "--console-log-level", "warn");

        for peer in &self.peers {
            pair(&mut args, "-p", peer);
        }
        for port in &self.tcp_whitelist {
            pair(&mut args, "--tcp-whitelist", &port.to_string());
        }
        for port in &self.udp_whitelist {
            pair(&mut args, "--udp-whitelist", &port.to_string());
        }
        for forward in &self.forwards {
            pair(&mut args, "--port-forward", &forward.to_url());
        }

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> LaunchSpec {
        LaunchSpec {
            network_name: "scaffolding-mc-ABCD1234".to_string(),
            network_secret: "XXXX-YYYY".to_string(),
            hostname: "scaffolding-mc-server-25565".to_string(),
            instance_name: "mclink".to_string(),
            ipv4: Ipv4Addr::new(10, 144, 144, 1),
            peers: vec!["tcp://public.easytier.top:11010".to_string()],
            tcp_whitelist: vec![25565, 13448],
            udp_whitelist: vec![25565],
            forwards: vec![Forward {
                proto: Proto::Tcp,
                bind: "127.0.0.1:25565".parse().unwrap(),
                dst: "10.144.144.1:25565".parse().unwrap(),
            }],
            rpc_port: 15888,
            p2p_only: true,
        }
    }

    /// 取 `flag` 后面紧跟的值（重复出现时返回全部）。
    fn values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
        args.iter()
            .enumerate()
            .filter(|(_, item)| item.as_str() == flag)
            .filter_map(|(i, _)| args.get(i + 1).map(|s| s.as_str()))
            .collect()
    }

    #[test]
    fn emits_core_flags_with_values() {
        let args = spec().to_args();
        assert_eq!(values(&args, "--network-name"), ["scaffolding-mc-ABCD1234"]);
        assert_eq!(values(&args, "--ipv4"), ["10.144.144.1"]);
        assert_eq!(values(&args, "--rpc-portal"), ["127.0.0.1:15888"]);
        assert_eq!(values(&args, "-p"), ["tcp://public.easytier.top:11010"]);
        // 白名单可以有多个，且两个协议各自独立
        assert_eq!(values(&args, "--tcp-whitelist"), ["25565", "13448"]);
        assert_eq!(values(&args, "--udp-whitelist"), ["25565"]);
    }

    #[test]
    fn no_tun_and_p2p_only_are_bare_flags() {
        let args = spec().to_args();
        assert!(args.iter().any(|a| a == "--no-tun"));
        assert!(args.iter().any(|a| a == "--p2p-only"));

        let mut relaxed = spec();
        relaxed.p2p_only = false;
        assert!(!relaxed.to_args().iter().any(|a| a == "--p2p-only"));
    }

    #[test]
    fn port_forward_uses_upstream_url_shape() {
        let args = spec().to_args();
        assert_eq!(
            values(&args, "--port-forward"),
            ["tcp://127.0.0.1:25565/10.144.144.1:25565"]
        );
    }

    #[test]
    fn forward_url_switches_protocol() {
        let forward = Forward {
            proto: Proto::Udp,
            bind: "0.0.0.0:5000".parse().unwrap(),
            dst: "10.144.144.2:5000".parse().unwrap(),
        };
        assert_eq!(forward.to_url(), "udp://0.0.0.0:5000/10.144.144.2:5000");
    }

    #[test]
    fn empty_lists_do_not_emit_flags() {
        let mut spec = spec();
        spec.tcp_whitelist.clear();
        spec.udp_whitelist.clear();
        spec.forwards.clear();
        spec.peers.clear();
        let args = spec.to_args();
        for flag in ["--tcp-whitelist", "--udp-whitelist", "--port-forward", "-p"] {
            assert!(values(&args, flag).is_empty(), "不应出现 {}", flag);
        }
    }
}
