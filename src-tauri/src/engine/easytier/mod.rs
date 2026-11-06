//! EasyTier 引擎：一个 [`Link`] 对应一个虚拟网络里的一次建链。
//!
//! # 分工
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`artifact`] | 版本 + 各平台产物 + SHA256（改动即发版） |
//! | [`binary`] | 在落地目录里认出 `easytier-core` / `easytier-cli` |
//! | [`provision`] | 下载 → 校验 → 解压 → 就位（单飞、可重试） |
//! | [`args`] | 启动参数（`easytier-core --help` 的 v2.6.4 语法） |
//! | [`rpc`] | 运行时管理（`easytier-cli`，仅查对端/加转发） |
//! | [`model`] | `peer list` 的负载类型 |
//! | 本文件 | 进程生命周期：起、探活、停 |
//!
//! # 一个进程 = 一条链路
//!
//! `easytier-core` 支持单进程跑多实例（`--config-dir` / `--instance-name`），但本应用
//! 的适配器是"一次联机一个房间"的语义，且**端口转发在启动参数里固定**（`--port-forward`
//! 只能启动时给，运行期靠 CLI 追加）。按进程隔离换来的是：停一个房间绝不会动到另一个，
//! 失败时整棵进程树都可以直接 kill 掉。
//!
//! # 每次建链都用新目录
//!
//! 核心会把 machine-id、缓存之类写进当前工作目录。共用目录会让两次联机的状态互相
//! 污染（上一轮的对端记录、被占用的监听端口），因此每建一条链路给一个全新的
//! `workdir`，退出时随目录一起丢弃。

pub mod args;
pub mod artifact;
pub mod binary;
pub mod model;
pub mod provision;
pub mod rpc;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub use args::{Forward, LaunchSpec, Proto};
pub use binary::Binaries;
pub use model::Peer;
pub use provision::{ensure_background, ensure_blocking, state, ProvisionState};

/// 启动后等待 RPC 门户可用的上限。
///
/// 实测冷启动在 300–800ms 之间就绪；给到 6 秒是为了覆盖"杀毒软件扫描新下载的 exe"
/// 这类极端情况，超时即判定启动失败并回收进程，而不是让它挂在那里。
const READY_TIMEOUT: Duration = Duration::from_secs(6);

/// EasyTier 官方公共节点（用于首次发现对端）。
///
/// 前两个是官方公共服务器，后两个是社区 HTTP 节点（陶瓦上游同样在用）。
/// 这些地址**只提供发现能力**，流量仍走 P2P；全部不可达时联机只是更难建立，
/// 不会静默失败（调用方会在等待对端超时后报错）。
pub const PUBLIC_NODES: [&str; 4] = [
    "tcp://public.easytier.top:11010",
    "tcp://public2.easytier.cn:54321",
    "https://etnode.zkitefly.eu.org/node1",
    "https://etnode.zkitefly.eu.org/node2",
];

/// 一次建链。
pub struct Link {
    child: Child,
    rpc_port: u16,
    /// `easytier-cli` 路径：运行时管理只走它。
    cli: PathBuf,
    workdir: PathBuf,
    log_path: PathBuf,
}

impl Link {
    /// 拉起一个核心进程，并等到它的 RPC 门户可用。
    ///
    /// 只有 RPC 可用之后，"查对端 / 加转发"才不是碰运气，因此就绪等待放在这里，
    /// 而不是让每个调用点各写一遍重试。
    pub fn spawn(bins: &Binaries, spec: &LaunchSpec, workdir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(workdir)
            .map_err(|e| format!("创建引擎工作目录失败（{}）: {}", workdir.display(), e))?;
        let log_path = workdir.join("easytier.log");
        let log = std::fs::File::create(&log_path)
            .map_err(|e| format!("创建引擎日志失败（{}）: {}", log_path.display(), e))?;
        let log_err = log
            .try_clone()
            .map_err(|e| format!("创建引擎日志失败: {}", e))?;

        let mut command = Command::new(&bins.core);
        command
            .args(spec.to_args())
            .current_dir(workdir)
            .stdin(Stdio::null())
            // 输出直接落文件：走管道而不读会在缓冲写满后把子进程卡死
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW：不给用户弹一个黑框
            command.creation_flags(0x0800_0000);
        }

        let child = command.spawn().map_err(|e| {
            format!(
                "启动 EasyTier 内核失败（{}）: {}",
                bins.core.display(),
                e
            )
        })?;

        let mut link = Self {
            child,
            rpc_port: spec.rpc_port,
            cli: bins.cli.clone(),
            workdir: workdir.to_path_buf(),
            log_path,
        };
        link.wait_ready()?;
        Ok(link)
    }

    /// 等 RPC 门户可用（同时也确认进程没在启动瞬间就退出）。
    fn wait_ready(&mut self) -> Result<(), String> {
        let started = Instant::now();
        loop {
            if !self.is_alive() {
                return Err(format!(
                    "EasyTier 内核启动后立即退出（日志见 {}）",
                    self.log_path.display()
                ));
            }
            if rpc::peers(&self.cli, self.rpc_port, &self.workdir).is_ok() {
                return Ok(());
            }
            if started.elapsed() > READY_TIMEOUT {
                let tail = self.log_tail();
                self.stop();
                return Err(format!(
                    "EasyTier 内核在 {} 秒内没有就绪。{}",
                    READY_TIMEOUT.as_secs(),
                    tail
                ));
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    /// 进程是否还活着。
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// 当前对端列表。
    pub fn peers(&self) -> Result<Vec<Peer>, String> {
        rpc::peers(&self.cli, self.rpc_port, &self.workdir)
    }

    /// 追加一条端口转发。
    pub fn add_forward(&self, forward: &Forward) -> Result<(), String> {
        rpc::add_forward(&self.cli, self.rpc_port, forward, &self.workdir)
    }

    /// 结束进程（幂等）。
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// 引擎日志尾部（最多 20 行），用于把失败原因带进错误消息。
    pub fn log_tail(&self) -> String {
        let Ok(text) = std::fs::read_to_string(&self.log_path) else {
            return String::new();
        };
        let lines: Vec<&str> = text.lines().rev().take(20).collect();
        if lines.is_empty() {
            return String::new();
        }
        let joined = lines.into_iter().rev().collect::<Vec<_>>().join("\n");
        format!("\n--- EasyTier 日志尾部 ---\n{}", joined)
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 取一个当前空闲的回环端口，供 RPC 门户使用。
///
/// 先绑再放，存在"放掉之后被别人抢走"的竞态；这里接受它：core 自己会在端口被占时
/// 报错退出，而我们的失败路径本来就会把日志尾巴带出来，不会变成"静默无响应"。
pub fn free_loopback_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_port_is_usable() {
        let port = free_loopback_port();
        assert!(port > 0, "应拿到一个真实端口");
    }

    #[test]
    fn public_nodes_look_like_peer_urls() {
        for node in PUBLIC_NODES {
            assert!(
                node.starts_with("tcp://") || node.starts_with("https://"),
                "公共节点应是可解析的 URL: {}",
                node
            );
            assert!(node.contains("://") && node.len() > 10);
        }
    }
}
