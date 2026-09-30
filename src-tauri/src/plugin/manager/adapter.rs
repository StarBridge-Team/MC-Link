//! 适配器能力的便捷门面。
//!
//! 旧的 Tauri 命令需要"确定要调适配器"这一层语义（而不是泛指"某类能力"），
//! 这里把它收敛成三个方法，避免命令层自己去拼 `PluginKind::Adapter`。

use serde_json::Value;
use std::path::PathBuf;

use crate::plugin::manifest::PluginKind;

use super::PluginManager;

impl PluginManager {
    /// 启动内置适配器（与旧 `adapter_startup_init` 命令行为一致）。
    ///
    /// 内部会轮询端口（最长约 12 秒），调用方应当把它放到阻塞线程上执行。
    pub fn adapter_launch_all(&self) -> Result<String, String> {
        let manager = self
            .inner
            .adapter
            .lock()
            .map_err(|_| "适配器管理器锁不可用".to_string())?;
        manager.launch_all();
        let status = manager.get_status();
        Ok(if status.running {
            "适配器已就绪".to_string()
        } else if status.installed {
            "适配器已安装，正在启动...".to_string()
        } else {
            "适配器正在安装...".to_string()
        })
    }

    /// 内置适配器的数据目录（供下载/安装流程落地文件）。
    pub fn adapter_dir(&self) -> Result<PathBuf, String> {
        let manager = self
            .inner
            .adapter
            .lock()
            .map_err(|_| "适配器管理器锁不可用".to_string())?;
        Ok(manager.adapter_dir.clone())
    }

    /// 停止内置适配器及其托管的第三方进程，**不关停**插件系统本身。
    ///
    /// 与 [`PluginManager::shutdown`] 的区别：后者会同时关闭网关与远程插件，
    /// 适用于应用退出；本方法适用于"关闭窗口"这类只应停止适配器的场景。
    pub fn shutdown_adapters(&self) -> Result<(), String> {
        let manager = self
            .inner
            .adapter
            .lock()
            .map_err(|_| "适配器管理器锁不可用".to_string())?;
        manager.shutdown_all();
        Ok(())
    }

    /// 调用适配器能力方法，返回插件返回的原始负载。
    ///
    /// 权限判定、路由与回退都在 [`PluginManager::invoke`] 内部完成。
    pub async fn adapter_invoke(
        &self,
        method: &str,
        game_id: Option<&str>,
        params: Value,
    ) -> Result<Value, String> {
        self.invoke(PluginKind::Adapter, game_id, method, params)
            .await
            .map(|(_, value)| value)
            .map_err(|e| format!("{}: {}", e.code, e.message))
    }
}
