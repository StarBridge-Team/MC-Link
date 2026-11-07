//! 通过 `easytier-cli` 做**运行时管理**：查对端、加端口转发。
//!
//! # 为什么必须借 CLI
//!
//! `easytier-core` 只有启动参数，**没有子命令**（v2.6.4 实测：`--help` 全是 flag）。
//! 进程起来之后，"现在有哪些对端""再加一条端口转发"这两件事只能通过它的 RPC 门户
//! 完成，而 `easytier-cli` 就是那个门户的官方客户端。它与 `easytier-core` 同一个
//! 压缩包，不额外下载，也不参与建链——建链永远由 core 的启动参数完成。
//!
//! # 语法以 v2.6.4 为准
//!
//! - 全局参数（`-p` 指 RPC 门户、`-o json`）必须写在**子命令之前**；
//! - 子命令是 `peer list`（不是旧版的 `peer`）；
//! - 端口转发是 `port-forward add <tcp|udp> <bind> <dst>`。
//!
//! # 超时
//!
//! `Command::output()` 在子进程卡住时会永久阻塞，而调用方是 `spawn_blocking`
//! 上的适配器线程——卡死的后果是"点开始联机没有任何反应，也没有错误"。
//! 因此这里自己轮询 `try_wait`，超时即 kill 并明确报错。

use super::args::Forward;
use super::model::Peer;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 单次 CLI 调用上限。这些命令只查内存状态，正常在 100ms 内返回。
const CLI_TIMEOUT: Duration = Duration::from_secs(10);
/// 轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(40);

/// Windows 上的 `CREATE_NO_WINDOW`：不弹黑色控制台窗口。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 查询当前对端列表。
pub fn peers(cli: &Path, rpc_port: u16, workdir: &Path) -> Result<Vec<Peer>, String> {
    let args = [
        "-p".to_string(),
        format!("127.0.0.1:{}", rpc_port),
        "-o".to_string(),
        "json".to_string(),
        "peer".to_string(),
        "list".to_string(),
    ];
    let text = run(cli, &args, workdir)?;
    serde_json::from_str::<Vec<Peer>>(text.trim()).map_err(|e| {
        format!(
            "解析对端列表失败: {}（原始输出: {}）",
            e,
            abbreviate(text.trim())
        )
    })
}

/// 追加一条端口转发（立即生效，不需要重启 core）。
pub fn add_forward(
    cli: &Path,
    rpc_port: u16,
    forward: &Forward,
    workdir: &Path,
) -> Result<(), String> {
    let args = [
        "-p".to_string(),
        format!("127.0.0.1:{}", rpc_port),
        "port-forward".to_string(),
        "add".to_string(),
        forward.proto.as_str().to_string(),
        forward.bind.to_string(),
        forward.dst.to_string(),
    ];
    run(cli, &args, workdir).map(|_| ())
}

/// 执行一次 CLI，返回 stdout；非零退出码或超时都算失败。
fn run(cli: &Path, args: &[String], workdir: &Path) -> Result<String, String> {
    let mut command = Command::new(cli);
    command
        .args(args)
        .current_dir(workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("启动 easytier-cli 失败（{}）: {}", cli.display(), e))?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = drain(child.stdout.take());
                let err = drain(child.stderr.take());
                if status.success() {
                    return Ok(out);
                }
                let detail = if err.trim().is_empty() { out } else { err };
                return Err(format!(
                    "easytier-cli {} 失败（{}）: {}",
                    args.join(" "),
                    status,
                    abbreviate(detail.trim())
                ));
            }
            Ok(None) => {
                if started.elapsed() > CLI_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "easytier-cli {} 超过 {} 秒未返回，已终止",
                        args.join(" "),
                        CLI_TIMEOUT.as_secs()
                    ));
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => return Err(format!("等待 easytier-cli 失败: {}", e)),
        }
    }
}

/// 读空一个管道（进程退出后调用，不会阻塞）。
fn drain(pipe: Option<impl Read>) -> String {
    let mut text = String::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_string(&mut text);
    }
    text
}

/// 截断过长的输出，避免把整段日志塞进错误消息弹给用户。
fn abbreviate(text: &str) -> String {
    const MAX: usize = 300;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    let head: String = text.chars().take(MAX).collect();
    format!("{}…", head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_names_match_upstream_cli() {
        use crate::engine::easytier::args::Proto;
        assert_eq!(Proto::Tcp.as_str(), "tcp");
        assert_eq!(Proto::Udp.as_str(), "udp");
    }

    #[test]
    fn missing_binary_reports_clear_error() {
        let dir = std::env::temp_dir();
        let err = run(Path::new("definitely-not-a-real-cli-xyz"), &[], &dir).unwrap_err();
        assert!(err.contains("启动 easytier-cli 失败"), "{}", err);
    }

    #[test]
    fn abbreviate_keeps_short_text() {
        assert_eq!(abbreviate("ok"), "ok");
        let long = "x".repeat(400);
        let out = abbreviate(&long);
        assert!(out.ends_with('…'));
        assert_eq!(out.chars().count(), 301);
    }
}
