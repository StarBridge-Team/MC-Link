//! 陶瓦端口探测：后台线程轮询 + 结果共享。
//!
//! # 为什么轮询要放到后台线程
//!
//! 端口探测最多要等 12 秒（陶瓦启动并写出 `terracotta-port.json`）。
//! 旧实现在**调用线程**里 `thread::sleep` 轮询：谁调用谁被卡住 12 秒。
//! 而调用方既有 tokio worker（异步命令），也有主线程（退出路径）——
//! 前者会卡住整个异步运行时，后者会让所有窗口无响应。
//!
//! 现在由后台线程做轮询，调用方只等结果；等待期间**不持有任何锁**，
//! 因此同一时刻的其它适配器调用照常进行。
//!
//! # 为什么还需要一个"任务槽"
//!
//! 去掉全局锁之后，"并发调用各自拉起一个陶瓦进程"的风险回来了：
//! 两个进程会互相覆盖端口文件，最终谁都拿不到端口。
//! [`LaunchSlot`] 负责去重：同一时刻只允许一个"拉起 + 探测"任务，
//! 后来的调用复用同一份结果。

use std::path::Path;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::utils::lock_or_recover;

/// 端口探测的总时限（与旧实现一致：12 秒）。
pub(super) const PORT_PROBE_TIMEOUT: Duration = Duration::from_secs(12);

/// 轮询间隔。
const PORT_PROBE_INTERVAL: Duration = Duration::from_millis(200);

/// 单次 TCP 连通性探测的超时。
const PORT_PROBE_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// 端口是否可连通。
///
/// 最多阻塞 2 秒，因此调用方**不要**在持有其它锁时调用它。
pub(super) fn is_port_alive(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        PORT_PROBE_CONNECT_TIMEOUT,
    )
    .is_ok()
}

/// 轮询端口文件，直到拿到可用端口或超时；超时返回 `None`。
///
/// 阻塞调用线程（内部 `thread::sleep`），因此只在后台线程里调用。
pub(super) fn poll_port(dir: &Path, timeout: Duration) -> Option<u16> {
    let hmcl_file = dir.join("Terracotta.temp").join("terracotta-port.json");
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return None;
        }
        if let Some(port) = read_port(&hmcl_file) {
            if is_port_alive(port) {
                println!("[适配器] 陶瓦联机端口: {}", port);
                let _ = std::fs::remove_file(&hmcl_file);
                return Some(port);
            }
        }
        std::thread::sleep(PORT_PROBE_INTERVAL);
    }
}

/// 从端口文件里读出端口号。
fn read_port(hmcl_file: &Path) -> Option<u16> {
    let content = std::fs::read_to_string(hmcl_file).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let port = json.get("port").and_then(|v| v.as_u64())? as u16;
    Some(port)
}

/// 一次"拉起 + 探测"任务的共享槽位。
///
/// 两个作用：
/// 1. **去重**：并发调用只会有一次真正的"拉起进程 + 探测端口"；
/// 2. **共享结果**：后来的调用直接复用先到者的结果，不必再等一轮 12 秒。
pub(super) struct LaunchSlot {
    state: Mutex<LaunchState>,
    done: Condvar,
}

#[derive(Default)]
struct LaunchState {
    /// 是否正有任务在跑。
    running: bool,
    /// 最近一次任务的结果（`None` = 还没有结果）。
    outcome: Option<Result<u16, String>>,
}

impl LaunchSlot {
    pub(super) fn new() -> Self {
        LaunchSlot {
            state: Mutex::new(LaunchState::default()),
            done: Condvar::new(),
        }
    }

    /// 抢占一次任务：没有任务在跑时返回 `true`（由调用方负责执行）。
    pub(super) fn try_begin(&self) -> bool {
        let mut g = lock_or_recover(&self.state, "launch_slot");
        if g.running {
            return false;
        }
        g.running = true;
        g.outcome = None;
        true
    }

    /// 任务结束：发布结果并唤醒等待者。
    pub(super) fn finish(&self, outcome: Result<u16, String>) {
        {
            let mut g = lock_or_recover(&self.state, "launch_slot");
            g.outcome = Some(outcome);
            g.running = false;
        }
        self.done.notify_all();
    }

    /// 等待任务出结果；超时返回 `None`。
    pub(super) fn wait(&self, timeout: Duration) -> Option<Result<u16, String>> {
        let deadline = Instant::now() + timeout;
        let mut g = lock_or_recover(&self.state, "launch_slot");
        while g.running {
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                break;
            }
            g = match self.done.wait_timeout(g, remain) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
        g.outcome.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_port_gives_up_when_the_port_file_never_appears() {
        let dir = std::env::temp_dir().join(format!(
            "mc-link-probe-{}",
            crate::plugin::crypto::random_hex(4)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // 没有端口文件：只能超时返回 None（失败路径）。
        assert!(poll_port(&dir, Duration::from_millis(300)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_slot_allows_only_one_task_at_a_time() {
        let slot = LaunchSlot::new();
        assert!(slot.try_begin(), "第一个调用应当抢到任务");
        assert!(!slot.try_begin(), "同一时刻只允许一个拉起任务");
        slot.finish(Ok(12345));
        assert_eq!(slot.wait(Duration::from_millis(100)), Some(Ok(12345)));
    }

    #[test]
    fn launch_slot_reports_failure_to_waiters() {
        let slot = LaunchSlot::new();
        assert!(slot.try_begin());
        slot.finish(Err("陶瓦联机自动启动超时".to_string()));
        let outcome = slot.wait(Duration::from_millis(100)).expect("应拿到结果");
        assert_eq!(outcome.unwrap_err(), "陶瓦联机自动启动超时");
    }
}
