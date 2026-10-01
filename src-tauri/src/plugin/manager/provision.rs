//! 插件供给：准入表刷新、外部插件进程拉起与握手等待。
//!
//! 这些方法都挂在 [`PluginManager`] 上，但实现细节与调度无关，因此单独成文件。
//! 作为 `manager` 的子模块，这里可以直接访问 `Inner` 的私有字段，无需为了拆分
//! 而把内部状态改成 `pub(crate)`。

use std::time::Duration;

use crate::plugin::capability::Provider;
use crate::plugin::gateway::AuthEntry;
use crate::plugin::launcher;
use crate::plugin::protocol::{error_code, ErrorInfo};
use crate::plugin::registry::{PluginRecord, PluginSource};

use super::PluginManager;

/// 外部插件从进程启动到完成握手的等待上限。
const PLUGIN_READY_TIMEOUT: Duration = Duration::from_secs(20);

/// 等待握手的轮询间隔。
const READY_POLL_INTERVAL: Duration = Duration::from_millis(200);

impl PluginManager {
    /// 依据注册表重建网关准入表。
    ///
    /// 只为"外部 + 已启用 + 未被拉黑"的插件建立准入项：这些条件之外插件即使连上
    /// 网关也会被 `gateway::handle_connection` 判定为未知插件而拒绝。
    pub(super) fn refresh_auth(&self) -> Result<(), String> {
        let records: Vec<PluginRecord> = self
            .inner
            .registry
            .read()
            .map_err(|_| "插件注册表锁不可用".to_string())?
            .list()
            .into_iter()
            .cloned()
            .collect();

        let mut table = std::collections::HashMap::new();
        for record in records {
            if record.source != PluginSource::External
                || !record.enabled
                || !record.trust.is_runnable()
            {
                continue;
            }
            let psk = record.ensure_secret()?;
            table.insert(
                record.manifest.id.clone(),
                AuthEntry {
                    plugin_id: record.manifest.id.clone(),
                    psk,
                    permissions: record.permissions.clone(),
                    limits: record.manifest.limits.clone(),
                    max_auth_failures: record.manifest.limits.max_auth_failures,
                },
            );
        }
        *self
            .inner
            .auth
            .write()
            .map_err(|_| "插件准入表锁不可用".to_string())? = table;
        Ok(())
    }

    /// 取网关地址与一次性启动令牌。
    pub(super) fn gateway_endpoint(&self) -> Result<(String, String), ErrorInfo> {
        let gateway = self
            .inner
            .gateway
            .read()
            .ok()
            .and_then(|g| g.clone())
            .ok_or_else(|| ErrorInfo::new(error_code::UNAVAILABLE, "插件网关尚未启动"))?;
        Ok((gateway.endpoint(), gateway.token().to_string()))
    }

    /// 拉起外部插件进程。
    pub(super) async fn launch_external(&self, record: &PluginRecord) -> Result<(), ErrorInfo> {
        let (endpoint, token) = self.gateway_endpoint()?;
        let runtime_dir = self.inner.data_dir.join("Plugins").join("runtime");
        let (child, session) = launcher::spawn(record, &endpoint, &token, &runtime_dir)
            .map_err(|e| ErrorInfo::new(error_code::INTERNAL, e))?;
        let pid = child.id();

        // 同一插件若已有进程，先收掉旧的。正常情况下不会走到（`provider` 已串行化），
        // 但"先 kill 再存"的顺序能保证表里永远只有一个句柄，不会漏掉孤儿。
        self.reap_child(record.id(), "重复拉起");
        if let Ok(mut children) = self.inner.children.write() {
            children.insert(record.id().to_string(), child);
        }

        println!(
            "[插件] 已启动 {} (pid {}), 会话文件 {}",
            record.id(),
            pid,
            session.display()
        );
        Ok(())
    }

    /// 结束并回收指定插件的进程（若存在）。
    ///
    /// 三处都需要它：握手超时、插件被停用/拉黑、核心退出。少了它会分别表现为
    /// "握手失败的进程永远挂着"、"停用了插件却还在跑"、"退出后进程残留 / Unix 僵尸"。
    pub(super) fn reap_child(&self, plugin_id: &str, why: &str) {
        let child = self
            .inner
            .children
            .write()
            .ok()
            .and_then(|mut map| map.remove(plugin_id));
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
            println!("[插件] 已结束 {} 的进程（{}）", plugin_id, why);
        }
    }

    /// 等待插件完成握手并注册为可用提供者。
    pub(super) async fn wait_ready(&self, plugin_id: &str) -> Result<Provider, ErrorInfo> {
        let deadline = tokio::time::Instant::now() + PLUGIN_READY_TIMEOUT;
        loop {
            if let Some(provider) = self.cached_provider(plugin_id) {
                return Ok(provider);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ErrorInfo::new(
                    error_code::TIMEOUT,
                    format!(
                        "等待插件 {} 握手超时（{} 秒）",
                        plugin_id,
                        PLUGIN_READY_TIMEOUT.as_secs()
                    ),
                ));
            }
            tokio::time::sleep(READY_POLL_INTERVAL).await;
        }
    }
}
