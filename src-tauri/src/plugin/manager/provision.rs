//! 插件供给：准入表刷新、外部插件进程拉起与握手等待。
//!
//! 这些方法都挂在 [`PluginManager`] 上，但实现细节与调度无关，因此单独成文件。
//! 作为 `manager` 的子模块，这里可以直接访问 `Inner` 的私有字段，无需为了拆分
//! 而把内部状态改成 `pub(crate)`。

use std::sync::Arc;
use std::time::Duration;

use crate::plugin::capability::Provider;
use crate::plugin::gateway::AuthEntry;
use crate::plugin::launcher;
use crate::plugin::manifest::RuntimeKind;
use crate::plugin::protocol::{error_code, ErrorInfo};
use crate::plugin::registry::{PluginRecord, PluginSource};
use crate::plugin::runtime;

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
            let psk = record.ensure_secret(&self.inner.data_dir)?;
            // 夹取到核心允许的范围：清单是插件自述的，不夹取等于把限流开关交给它
            let limits = record.manifest.limits.clamped();
            table.insert(
                record.manifest.id.clone(),
                AuthEntry {
                    plugin_id: record.manifest.id.clone(),
                    psk,
                    permissions: record.permissions.clone(),
                    max_auth_failures: limits.max_auth_failures,
                    limits,
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
        // Node 轨道要先确认解释器就位：缺失时**在线安装**（复用 downloader/verified），
        // 而不是直接失败让用户去猜。
        if record.manifest.runtime.kind == RuntimeKind::Node {
            self.ensure_node_runtime().await?;
        }
        // PSK 与准入表同源（同一个 `ensure_secret`），否则插件算出的 proof
        // 与核心期望值不一致，握手必然失败。
        let psk = record
            .ensure_secret(&self.inner.data_dir)
            .map_err(|e| ErrorInfo::new(error_code::INTERNAL, e))?;
        let (child, session) = launcher::spawn(record, &endpoint, &token, &psk, &self.inner.data_dir)
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

    /// 确保共享 Node 运行时可用（缺失时在线安装）。
    ///
    /// # 为什么在这里触发
    ///
    /// 运行时有 90 MB 级，"应用一启动就下载"会让没有 Node 插件的用户白等一次网络；
    /// "等用户点安装"又会让插件在首次使用时莫名其妙地失败。折中是**按需**：
    /// 真正要拉起 Node 插件时才下载（本函数），外加启动时的一次**后台预热**
    /// （[`PluginManager::prefetch_node_runtime`]），这样两种路径用户都不用等第二次。
    ///
    /// 失败语义：抛 `unavailable`，消息里带"去设置里安装运行时"的指引；
    /// 界面据此展示可操作提示，而不是"插件加载失败"。
    pub(super) async fn ensure_node_runtime(&self) -> Result<(), ErrorInfo> {
        let data_dir = &self.inner.data_dir;
        if runtime::resolve(data_dir).is_ok() {
            return Ok(());
        }

        self.emit_runtime("downloading", None, None);
        match runtime::ensure(data_dir).await {
            Ok(version) => {
                self.emit_runtime("ready", Some(version), None);
                Ok(())
            }
            Err(e) => {
                self.emit_runtime("failed", None, Some(e.clone()));
                Err(ErrorInfo::new(error_code::UNAVAILABLE, e))
            }
        }
    }

    /// 启动时的后台预热：**只**在"确有已启用的 Node 插件且运行时缺失"时发生。
    ///
    /// 不阻塞启动：下载在独立任务里跑，失败只留日志（下次按需拉起时会再试）。
    pub(super) fn prefetch_node_runtime(self: &Arc<Self>) {
        if runtime::resolve(&self.inner.data_dir).is_ok() {
            return;
        }
        if !runtime::needed_by_enabled_plugins(&self.list()) {
            return;
        }

        let weak = Arc::downgrade(self);
        tauri::async_runtime::spawn(async move {
            let Some(manager) = weak.upgrade() else { return };
            println!("[插件] 检测到已启用的 Node 插件，开始后台安装共享运行时");
            if let Err(e) = manager.ensure_node_runtime().await {
                eprintln!("[插件] 运行时后台安装失败（首次使用时仍会重试）: {}", e.message);
            }
        });
    }

    /// 广播运行时状态（`plugin-event { type: "runtime" }`）。
    fn emit_runtime(&self, state: &str, version: Option<String>, message: Option<String>) {
        self.emit(serde_json::json!({
            "type": "runtime",
            "state": state,
            "version": version,
            "message": message,
        }));
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
