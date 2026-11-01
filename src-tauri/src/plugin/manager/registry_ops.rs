//! 注册表与插件启停：加载清单、增删授权、启用/停用/拉黑。
//!
//! 这些动作都只碰“注册表 + 准入表 + 已就绪的提供者”，与调度中枢的生命周期
//! 无关，因此单独成文件，让 [`super::PluginManager`] 只保留拉起与路由。

use crate::plugin::permission::Permission;
use crate::plugin::registry::PluginRecord;

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
            if let Ok(mut providers) = self.inner.providers.write() {
                if let Some(p) = providers.get(plugin_id) {
                    p.close("插件已被停用");
                }
                providers.remove(plugin_id);
            }
            // 进程也要收掉：只关会话时插件可以选择不理会，进程会继续跑
            self.reap_child(plugin_id, "插件已被停用");
        }
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
            if let Ok(mut providers) = self.inner.providers.write() {
                if let Some(p) = providers.remove(plugin_id) {
                    p.close("插件已被拉黑");
                }
            }
        }
        self.refresh_auth()
    }
}
