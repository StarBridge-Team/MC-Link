//! 注册表与插件启停：加载清单、增删授权、启用/停用/拉黑。
//!
//! 这些动作都只碰“注册表 + 准入表 + 已就绪的提供者”，与调度中枢的生命周期
//! 无关，因此单独成文件，让 [`super::PluginManager`] 只保留拉起与路由。

use crate::plugin::install::{self, Installed};
use crate::plugin::permission::{Permission, TrustLevel};
use crate::plugin::registry::{PluginRecord, PluginSource};

use super::{builtin, PluginManager};

impl PluginManager {
    pub fn reload(&self) -> Vec<String> {
        let mut registry = match self.inner.registry.write() {
            Ok(r) => r,
            Err(_) => return vec!["插件注册表锁不可用".to_string()],
        };
        let builtins = builtin::builtin_records().unwrap_or_default();
        let mut warnings = registry.reload(builtins);
        if let Ok(mut w) = self.inner.warnings.write() {
            warnings.extend(w.drain(..));
        }
        drop(registry);
        let _ = self.refresh_auth();
        warnings
    }

    pub fn list(&self) -> Vec<PluginRecord> {
        match self.inner.registry.read() {
            Ok(r) => r.list().into_iter().cloned().collect(),
            Err(_) => {
                // 静默返回空列表会让"插件全都不见了"变成一个无迹可查的谜
                eprintln!("[插件] 注册表读锁已中毒，无法列出插件");
                Vec::new()
            }
        }
    }

    pub fn get(&self, plugin_id: &str) -> Option<PluginRecord> {
        self.inner
            .registry
            .read()
            .ok()
            .and_then(|r| r.get(plugin_id).cloned())
    }

    pub fn set_enabled(&self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        {
            let mut registry = self
                .inner
                .registry
                .write()
                .map_err(|_| "插件注册表锁不可用".to_string())?;
            registry.set_enabled(plugin_id, enabled)?;
        }
        self.refresh_auth()?;
        if !enabled {
            self.stop_plugin(plugin_id, "插件已被停用");
        }
        Ok(())
    }

    /// 停掉一个插件：关会话、收进程。
    ///
    /// 停用、拉黑、覆盖安装、卸载都走这一处——否则四份"关会话 + kill"的逻辑迟早漂移，
    /// 表现为某一入口下"插件停了但进程还在"。
    fn stop_plugin(&self, plugin_id: &str, why: &str) {
        if let Ok(mut providers) = self.inner.providers.write() {
            if let Some(p) = providers.get(plugin_id) {
                p.close(why);
            }
            providers.remove(plugin_id);
        }
        // 进程也要收掉：只关会话时插件可以选择不理会，进程会继续跑
        self.reap_child(plugin_id, why);
    }

    /// 安装一个本地插件包，返回安装结论。
    ///
    /// 未签名包需要调用方传 `allow_unsigned = true`（界面上的二次确认）；
    /// 通过后按"用户已显式启用"对待——安装本身就是一次明确的用户动作。
    pub fn install_package(&self, path: &std::path::Path, allow_unsigned: bool) -> Result<Installed, String> {
        let mut outcome = install::install_from_file(&self.inner.data_dir, path, allow_unsigned)?;
        // 同 ID 覆盖安装：先把旧进程收掉，否则"文件换了、进程还在跑旧代码"
        self.stop_plugin(&outcome.id, "插件正在覆盖安装");
        outcome.warnings.extend(self.reload());
        if outcome.trust == TrustLevel::Unsigned {
            // 已二次确认安装 = 显式启用（否则用户装完发现"什么都没发生"）
            if let Err(e) = self.set_enabled(&outcome.id, true) {
                outcome.warnings.push(format!("已安装但启用失败: {e}"));
            }
        }
        Ok(outcome)
    }

    /// 卸载插件：停掉、删目录、清注册表状态。
    pub fn uninstall(&self, plugin_id: &str, remove_data: bool) -> Result<(), String> {
        let record = self
            .get(plugin_id)
            .ok_or_else(|| format!("插件不存在: {plugin_id}"))?;
        if record.source != PluginSource::External {
            return Err(format!("{plugin_id} 是内置插件，不能卸载"));
        }

        self.stop_plugin(plugin_id, "插件正在卸载");
        install::uninstall(&self.inner.data_dir, plugin_id, remove_data)?;
        if let Ok(mut registry) = self.inner.registry.write() {
            registry.forget(plugin_id)?;
        }
        let _ = self.refresh_auth();
        // 重扫：让调用方拿到的列表与磁盘一致
        let _ = self.reload();
        Ok(())
    }

    pub fn set_grants(
        &self,
        plugin_id: &str,
        granted: Option<Vec<Permission>>,
    ) -> Result<(), String> {
        // 用块限界先放掉注册表写锁：`refresh_auth` 内部要读同一把锁，
        // 持有写锁时调用会死锁（RwLock 不可重入）。
        {
            let mut registry = self
                .inner
                .registry
                .write()
                .map_err(|_| "插件注册表锁不可用".to_string())?;
            registry.set_grants(plugin_id, granted)?;
        }
        // 必须刷新准入表：握手用的是准入表里的权限快照。不刷新的话"撤销权限"
        // 对重连无效——插件只要重启自身进程就能拿回已被撤销的权限。
        self.refresh_auth()
    }

    pub fn set_blocked(&self, plugin_id: &str, blocked: bool) -> Result<(), String> {
        {
            let mut registry = self
                .inner
                .registry
                .write()
                .map_err(|_| "插件注册表锁不可用".to_string())?;
            registry.set_blocked(plugin_id, blocked)?;
        }
        if blocked {
            self.stop_plugin(plugin_id, "插件已被拉黑");
        }
        self.refresh_auth()
    }
}
