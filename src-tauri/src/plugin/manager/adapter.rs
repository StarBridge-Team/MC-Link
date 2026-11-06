//! 适配器能力的便捷门面。
//!
//! 旧版本这里还负责"拉起内置陶瓦 exe / 定位落地目录"（`adapter_launch_all`、
//! `adapter_dir`）。陶瓦改为原生实现后，这两件事分别变成了：
//!
//! - 引擎的就位与下载 → `engine/easytier/provision.rs`（由适配器按需触发）；
//! - 落地目录 → 数据目录本身（`<data_dir>/Engine/…`）。
//!
//! 因此这里只剩"停掉所有适配器"这一条：它服务的是"关窗口"这类**不该关掉整个
//! 插件系统**的场景。

use crate::plugin::capability::Provider;
use crate::plugin::manifest::PluginKind;

use super::PluginManager;

impl PluginManager {
    /// 停止所有内置适配器托管的进程，**不关停**插件系统本身。
    ///
    /// 与 [`PluginManager::shutdown`] 的区别：后者会同时关闭网关与远程插件，
    /// 适用于应用退出；本方法适用于"关闭窗口"这类只应停止联机链路的场景。
    ///
    /// 外部适配器插件由各自的会话负责收尾（`close`），这里只管内置实现。
    pub fn shutdown_adapters(&self) -> Result<(), String> {
        let providers: Vec<Provider> = self
            .inner
            .providers
            .read()
            .map(|map| map.values().cloned().collect())
            .unwrap_or_default();

        for provider in providers {
            if provider.kind() != PluginKind::Adapter {
                continue;
            }
            if let Provider::Builtin(builtin) = &provider {
                builtin.shutdown();
            }
        }
        Ok(())
    }
}
