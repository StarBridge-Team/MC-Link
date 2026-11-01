//! 外部插件的拉起票据：让"拉起进程"与"等待握手"两段解耦。
//!
//! # 为什么不再用一把全局锁串起两段
//!
//! 旧实现用 `launch_lock`（tokio Mutex）把"拉起进程 + 等握手（最多 20 秒）"
//! 整段串行化：某个外部插件握手不成功时，**所有**插件的 RPC（检测器扫描、
//! 适配器状态、耦合器广播）都要排队等这把锁——表现为"点开始联机，界面转 20 秒圈"。
//!
//! 现在拆成两段：
//!
//! - **拉起段**（`launch_lock`）：只保护 `launch_external` 这一小段，毫秒级；
//! - **握手等待段**：不持锁，靠本模块的"票据"（[`LaunchTicket`]）等待。
//!
//! 票据按插件 ID 记录"正在拉起"，因此同一个插件的并发请求会复用同一次拉起
//! （不会各拉各的、互相 kill），而**不同**插件之间互不影响。
//!
//! # 关于就绪通知
//!
//! 插件握手完成的唯一入口是 `inbound.rs` 收到 `Connected` 后往 `providers`
//! 里插入 provider。这里不改动那条通道，而是**轮询 `providers`**（200ms 一次，
//! 与旧实现一致）并配合票据上的 [`tokio::sync::Notify`]：票据结束时立刻唤醒
//! 等待者，不用干等下一个轮询周期。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

use crate::plugin::capability::Provider;
use crate::plugin::protocol::{error_code, ErrorInfo};
use crate::utils::lock_or_recover;

use super::PluginManager;

/// 等待握手的上限。
///
/// 与 `provision.rs` 里的 `PLUGIN_READY_TIMEOUT` 取值一致（20 秒）。
/// 那里的常量是模块私有的，父模块取不到，这里复制一份同样的数值：
/// 改一处就要改两处，但比让 20 秒的等待重新变成全局串行要好。
pub(super) const LAUNCH_WAIT_TIMEOUT: Duration = Duration::from_secs(20);

/// 等待期间的轮询间隔。
const LAUNCH_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// 一次拉起的结果。
pub(super) enum LaunchAwait {
    /// 插件已就绪。
    Ready(Provider),
    /// 拉起失败。
    Failed(ErrorInfo),
    /// 等待超时。
    Timeout,
}

/// 某个插件的拉起票据。
pub(super) struct LaunchTicket {
    /// 票据状态：拉起是否结束、失败原因。
    state: Mutex<TicketState>,
    /// 拉起结束时唤醒等待者。
    ready: Notify,
}

#[derive(Default)]
struct TicketState {
    /// 拉起是否已经结束。
    done: bool,
    /// 失败原因（成功时为 `None`）。
    error: Option<ErrorInfo>,
}

impl LaunchTicket {
    fn new() -> Self {
        LaunchTicket {
            state: Mutex::new(TicketState::default()),
            ready: Notify::new(),
        }
    }

    /// 拉起结束：记录结果并唤醒等待者。
    fn finish(&self, error: Option<ErrorInfo>) {
        {
            let mut g = lock_or_recover(&self.state, "launch_ticket");
            g.done = true;
            g.error = error;
        }
        self.ready.notify_waiters();
    }

    /// 拉起是否已经结束。
    fn is_done(&self) -> bool {
        lock_or_recover(&self.state, "launch_ticket").done
    }

    /// 失败原因（若有）。
    fn error(&self) -> Option<ErrorInfo> {
        lock_or_recover(&self.state, "launch_ticket").error.clone()
    }
}

impl PluginManager {
    /// 登记一次拉起。
    ///
    /// 返回 `(票据, 是否由本次调用负责拉起)`：同一个插件若已有拉起在进行中，
    /// 就复用它的票据（不重复拉起进程），否则新建一张票据并由本次调用负责。
    pub(super) fn acquire_launch(&self, plugin_id: &str) -> (Arc<LaunchTicket>, bool) {
        if let Ok(map) = self.inner.launching.read() {
            if let Some(ticket) = map.get(plugin_id) {
                return (ticket.clone(), false);
            }
        }

        let ticket = Arc::new(LaunchTicket::new());
        let mut map = match self.inner.launching.write() {
            Ok(m) => m,
            Err(poisoned) => poisoned.into_inner(),
        };
        // 双检：拿到写锁之前可能已经有人登记了同一个插件
        if let Some(existing) = map.get(plugin_id) {
            return (existing.clone(), false);
        }
        map.insert(plugin_id.to_string(), ticket.clone());
        (ticket, true)
    }

    /// 结束一次拉起：发布结果、唤醒等待者并回收票据。
    pub(super) fn complete_launch(&self, plugin_id: &str, error: Option<ErrorInfo>) {
        let ticket = {
            let mut map = match self.inner.launching.write() {
                Ok(m) => m,
                Err(poisoned) => poisoned.into_inner(),
            };
            map.remove(plugin_id)
        };
        if let Some(ticket) = ticket {
            ticket.finish(error);
        }
    }

    /// 等待别人拉起的同一个插件就绪（**不持任何全局锁**）。
    ///
    /// 结束条件有三个：provider 出现（就绪）、票据带着错误结束（拉起失败）、
    /// 到达 20 秒上限（超时）。
    pub(super) async fn await_launch(
        &self,
        plugin_id: &str,
        ticket: &Arc<LaunchTicket>,
    ) -> LaunchAwait {
        let deadline = tokio::time::Instant::now() + LAUNCH_WAIT_TIMEOUT;
        loop {
            if let Some(provider) = self.cached_provider(plugin_id) {
                return LaunchAwait::Ready(provider);
            }
            if let Some(error) = ticket.error() {
                return LaunchAwait::Failed(error);
            }
            if ticket.is_done() {
                // 拉起结束却没有可用提供者：按握手未完成处理。
                return LaunchAwait::Failed(ErrorInfo::new(
                    error_code::TIMEOUT,
                    format!("插件 {} 拉起结束但未完成握手", plugin_id),
                ));
            }
            if tokio::time::Instant::now() >= deadline {
                return LaunchAwait::Timeout;
            }
            // 票据结束时会被立刻唤醒；轮询只是兜底（防止漏掉通知）。
            tokio::select! {
                _ = ticket.ready.notified() => {}
                _ = tokio::time::sleep(LAUNCH_POLL_INTERVAL) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> Arc<PluginManager> {
        let dir = std::env::temp_dir().join(format!(
            "mclink-launch-{}",
            crate::plugin::crypto::random_hex(6)
        ));
        PluginManager::new(&dir).unwrap()
    }

    #[test]
    fn different_plugins_do_not_share_a_ticket() {
        let mgr = manager();
        let (_ta, owner_a) = mgr.acquire_launch("plugin-a");
        assert!(owner_a, "第一个调用应当抢到拉起权");
        let (tb, owner_b) = mgr.acquire_launch("plugin-b");
        assert!(owner_b, "不同插件之间不应互相阻塞");
        mgr.complete_launch("plugin-b", None);
        assert!(tb.is_done());
        assert!(tb.error().is_none());
    }

    #[test]
    fn same_plugin_reuses_the_in_flight_ticket() {
        let mgr = manager();
        let (first, owner) = mgr.acquire_launch("plugin-x");
        assert!(owner);
        let (second, owner_again) = mgr.acquire_launch("plugin-x");
        assert!(!owner_again, "同一插件的并发拉起应复用同一次拉起");
        assert!(Arc::ptr_eq(&first, &second), "应当复用同一张票据");
    }

    #[test]
    fn a_waiting_handshake_does_not_block_other_launches() {
        let mgr = manager();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("应能创建测试运行时");
        rt.block_on(async {
            let (ticket, owner) = mgr.acquire_launch("plugin-a");
            assert!(owner);

            let waiter_mgr = mgr.clone();
            let waiter_ticket = ticket.clone();
            let handle = tokio::spawn(async move {
                waiter_mgr.await_launch("plugin-a", &waiter_ticket).await
            });
            // 让等待者先跑起来
            tokio::time::sleep(Duration::from_millis(50)).await;

            // 等待握手期间，全局拉起锁必须仍然是空的：
            // 否则一个握手失败的插件会把所有插件调用都堵住 20 秒。
            let guard =
                tokio::time::timeout(Duration::from_millis(500), mgr.inner.launch_lock.lock())
                    .await
                    .expect("等待握手期间全局拉起锁必须可用");
            drop(guard);

            mgr.complete_launch("plugin-a", None);
            let _ = handle.await;
        });
    }

    #[test]
    fn waiter_is_woken_when_the_launch_fails() {
        let mgr = manager();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("应能创建测试运行时");
        rt.block_on(async {
            let (ticket, owner) = mgr.acquire_launch("plugin-y");
            assert!(owner);

            let waiter_mgr = mgr.clone();
            let waiter_ticket = ticket.clone();
            let handle = tokio::spawn(async move {
                waiter_mgr.await_launch("plugin-y", &waiter_ticket).await
            });
            tokio::time::sleep(Duration::from_millis(50)).await;

            mgr.complete_launch("plugin-y", Some(ErrorInfo::new(error_code::TIMEOUT, "boom")));
            match handle.await.unwrap() {
                LaunchAwait::Failed(err) => assert_eq!(err.message, "boom"),
                _ => panic!("等待者应当拿到失败结果"),
            }
        });
    }
}
