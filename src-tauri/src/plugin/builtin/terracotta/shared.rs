//! 一条陶瓦链路的共享句柄。
//!
//! 适配器方法、Scaffolding 服务端的处理线程、后台维护线程都要读写同一份状态，
//! 因此把它收成一个 `Arc<Shared>`：状态在锁里，进程与服务端在各自的槽里，
//! 停止信号是一个原子位。
//!
//! # 为什么锁不包住"整条链路"
//!
//! 查状态（前端每几秒轮询一次）不该被"停进程"或"建链"挡住。因此：
//! - `session` 锁只覆盖一次状态读写；
//! - `link` / `server` 的锁只在取用与替换时短暂持有；
//! - 建链（可能要下载引擎、等对端）在锁外进行。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::engine::easytier::Link;

use super::scaffold::server::ScaffoldServer;
use super::state::{Phase, SessionState};

/// 共享句柄。
pub struct Shared {
    /// 数据目录（引擎安装、机器标识都在它下面）。
    pub data_dir: PathBuf,
    /// 可观测状态。
    pub session: Mutex<SessionState>,
    /// 成员最后活跃时间（`machine_id` → 时刻），用于把掉线的访客摘掉。
    pub seen: Mutex<HashMap<String, Instant>>,
    link: Mutex<Option<Link>>,
    server: Mutex<Option<ScaffoldServer>>,
    workdir: Mutex<Option<PathBuf>>,
    stopped: AtomicBool,
}

impl Shared {
    /// 用初始状态构造。
    pub fn new(data_dir: &Path, session: SessionState) -> Arc<Self> {
        Arc::new(Self {
            data_dir: data_dir.to_path_buf(),
            session: Mutex::new(session),
            seen: Mutex::new(HashMap::new()),
            link: Mutex::new(None),
            server: Mutex::new(None),
            workdir: Mutex::new(None),
            stopped: AtomicBool::new(false),
        })
    }

    /// 开始一次新的联机：清掉上一次留下的停止信号与成员活跃记录。
    ///
    /// # 为什么必须有这一步
    ///
    /// `Shared` 的生命周期与适配器实例一致（不是一次联机一个），而 `stop_all` 会把
    /// `stopped` 永久置位。不重置的话，第二次建房拉起的维护线程会立刻看到"已停止"
    /// 然后退出——表现为"第二次联机永远没有心跳、成员列表空着、断线也检测不到"。
    pub fn begin(&self) {
        self.stopped.store(false, Ordering::SeqCst);
        if let Ok(mut seen) = self.seen.lock() {
            seen.clear();
        }
    }

    /// 状态快照。
    pub fn snapshot(&self) -> SessionState {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 在锁内改状态。
    pub fn update(&self, change: impl FnOnce(&mut SessionState)) {
        let mut guard = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        change(&mut guard);
    }

    /// 是否已经收到停止指令（后台线程据此退出）。
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// 把状态置为异常并记录原因。
    pub fn fail(&self, message: impl Into<String>) {
        let message = message.into();
        self.update(|session| {
            session.phase = Phase::Exception;
            session.error = Some(message);
        });
    }

    /// 登记成员活跃时间。
    pub fn touch(&self, machine_id: &str) {
        if let Ok(mut seen) = self.seen.lock() {
            seen.insert(machine_id.to_string(), Instant::now());
        }
    }

    /// 摘掉超过 `ttl` 没心跳的成员，返回是否有变化。
    pub fn expire_members(&self, ttl: std::time::Duration) -> bool {
        let stale: Vec<String> = {
            let Ok(seen) = self.seen.lock() else {
                return false;
            };
            seen.iter()
                .filter(|(_, at)| at.elapsed() >= ttl)
                .map(|(id, _)| id.clone())
                .collect()
        };
        if stale.is_empty() {
            return false;
        }

        let mut changed = false;
        self.update(|session| {
            let before = session.members.len();
            session.members.retain(|member| {
                member.kind == super::members::ProfileKind::Host
                    || !stale.contains(&member.machine_id)
            });
            changed = session.members.len() != before;
        });
        if let Ok(mut seen) = self.seen.lock() {
            for id in &stale {
                seen.remove(id);
            }
        }
        changed
    }

    /// 登记链路与其工作目录。
    pub fn attach_link(&self, link: Link, workdir: PathBuf) {
        if let Ok(mut slot) = self.link.lock() {
            *slot = Some(link);
        }
        if let Ok(mut slot) = self.workdir.lock() {
            *slot = Some(workdir);
        }
    }

    /// 在锁内使用链路。
    pub fn with_link<T>(&self, use_link: impl FnOnce(Option<&mut Link>) -> T) -> T {
        let mut guard = self.link.lock().unwrap_or_else(|p| p.into_inner());
        use_link(guard.as_mut())
    }

    /// 登记 Scaffolding 服务端（房主侧）。
    pub fn attach_server(&self, server: ScaffoldServer) {
        if let Ok(mut slot) = self.server.lock() {
            *slot = Some(server);
        }
    }

    /// 停止一切：服务端、进程、工作目录。
    ///
    /// 幂等；后台维护线程看到 `is_stopped` 后会自行退出。
    pub fn stop_all(&self) {
        self.stopped.store(true, Ordering::SeqCst);

        if let Ok(mut slot) = self.server.lock() {
            // Drop 里会停 accept 循环
            *slot = None;
        }
        if let Ok(mut slot) = self.link.lock() {
            if let Some(mut link) = slot.take() {
                link.stop();
            }
        }
        if let Ok(mut slot) = self.workdir.lock() {
            if let Some(dir) = slot.take() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        if let Ok(mut seen) = self.seen.lock() {
            seen.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::builtin::terracotta::members::{Profile, ProfileKind};
    use std::time::Duration;

    fn shared() -> Arc<Shared> {
        let session = SessionState {
            phase: Phase::HostOk,
            role: super::super::state::Role::Host,
            room: "U/7000-0000-0000-0000".to_string(),
            port: Some(25565),
            members: vec![Profile {
                machine_id: "host".to_string(),
                name: "Host".to_string(),
                vendor: "test".to_string(),
                kind: ProfileKind::Host,
            }],
            error: None,
            machine_id: "host".to_string(),
        };
        Shared::new(std::env::temp_dir().as_path(), session)
    }

    #[test]
    fn fail_sets_exception_with_reason() {
        let shared = shared();
        shared.fail("房主已离线");
        let snapshot = shared.snapshot();
        assert_eq!(snapshot.phase, Phase::Exception);
        assert_eq!(snapshot.error.as_deref(), Some("房主已离线"));
    }

    #[test]
    fn expire_members_keeps_host_and_drops_stale_guests() {
        let shared = shared();
        shared.update(|session| {
            session.members.push(Profile {
                machine_id: "guest".to_string(),
                name: "Guest".to_string(),
                vendor: "test".to_string(),
                kind: ProfileKind::Guest,
            });
        });
        shared.touch("guest");

        // TTL 已过：访客应被摘掉，房主保留
        assert!(shared.expire_members(Duration::from_millis(0)));
        let snapshot = shared.snapshot();
        assert_eq!(snapshot.members.len(), 1);
        assert_eq!(snapshot.members[0].machine_id, "host");

        // 再次调用没有变化
        assert!(!shared.expire_members(Duration::from_millis(0)));
    }

    #[test]
    fn stop_all_is_idempotent() {
        let shared = shared();
        shared.stop_all();
        shared.stop_all();
        assert!(shared.is_stopped());
    }
}
