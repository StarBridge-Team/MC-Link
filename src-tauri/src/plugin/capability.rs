//! 能力提供者抽象：把"内置实现"与"外部插件"统一成同一个调用面。
//!
//! 核心调度代码不应该关心某个能力是进程内实现的还是通过 WebSocket 连过来的。
//! [`Provider`] 就是这两者的联合体：
//!
//! - [`Provider::Builtin`]：官方实现，进程内直调。用同步 trait 是因为这类实现
//!   本质上在跟外部进程/文件系统打交道（例如陶瓦适配器要拉起 exe 并轮询端口），
//!   调度方在 `spawn_blocking` 里调用，避免阻塞异步运行时。
//! - [`Provider::Remote`]：第三方插件，通过 [`SessionHandle`] 走加密 WebSocket。

use serde_json::Value;
use std::sync::Arc;

use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::{Permission, PermissionSet};
use crate::plugin::protocol::{error_code, ErrorInfo};
use crate::plugin::session::SessionHandle;

/// 内置插件实现的能力接口。
pub trait BuiltinCapability: Send + Sync {
    /// 插件 ID，必须与清单一致。
    fn plugin_id(&self) -> &str;
    /// 插件种类。
    fn kind(&self) -> PluginKind;
    /// 生效权限。核心对内置插件同样按此判定，不开后门。
    fn permissions(&self) -> &PermissionSet;
    /// 处理一次调用。实现方必须自行处理超时与错误，不能长时间不返回。
    fn invoke(&self, method: &str, params: Value) -> Result<Value, ErrorInfo>;
    /// 进程退出前的清理钩子。
    fn shutdown(&self) {}
}

/// 能力提供者。
pub enum Provider {
    Builtin(Arc<dyn BuiltinCapability>),
    Remote(Arc<SessionHandle>),
}

/// 克隆是廉价的（两份 `Arc`），调度时可以放心地把提供者复制出去用。
impl Clone for Provider {
    fn clone(&self) -> Self {
        match self {
            Provider::Builtin(b) => Provider::Builtin(b.clone()),
            Provider::Remote(s) => Provider::Remote(s.clone()),
        }
    }
}

impl Provider {
    pub fn plugin_id(&self) -> &str {
        match self {
            Provider::Builtin(b) => b.plugin_id(),
            Provider::Remote(s) => &s.plugin_id,
        }
    }

    pub fn kind(&self) -> PluginKind {
        match self {
            Provider::Builtin(b) => b.kind(),
            Provider::Remote(_) => PluginKind::Adapter,
        }
    }

    /// 是否仍然可用（外部插件断线后即不可用）。
    pub fn is_available(&self) -> bool {
        match self {
            Provider::Builtin(_) => true,
            Provider::Remote(s) => !s.is_closed(),
        }
    }

    /// 调用一次能力方法。
    pub async fn invoke(&self, method: &str, params: Value) -> Result<Value, ErrorInfo> {
        match self {
            Provider::Builtin(builtin) => {
                let builtin = builtin.clone();
                let method = method.to_string();
                tokio::task::spawn_blocking(move || builtin.invoke(&method, params))
                    .await
                    .map_err(|e| {
                        ErrorInfo::new(
                            error_code::INTERNAL,
                            format!("内置插件任务异常: {}", e),
                        )
                    })?
            }
            Provider::Remote(session) => session.call(method, params).await,
        }
    }

    /// 调用前先校验核心侧生效权限。
    ///
    /// 对 `Builtin` 同样生效——即使是我们自己的实现，也不应该绕过统一的权限判定
    /// 路径，否则权限模型会因为"自己人特例"而逐渐失效。
    pub async fn invoke_checked(
        &self,
        required: Permission,
        method: &str,
        params: Value,
    ) -> Result<Value, ErrorInfo> {
        match self {
            Provider::Builtin(builtin) => {
                builtin.permissions().require(required)?;
                self.invoke(method, params).await
            }
            Provider::Remote(session) => session.call_checked(required, method, params).await,
        }
    }

    /// 关闭（外部插件发 `bye`，内置实现走 shutdown 钩子）。
    pub fn close(&self, reason: &str) {
        match self {
            Provider::Builtin(_) => {}
            Provider::Remote(session) => session.close(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fake {
        fail: bool,
        permissions: PermissionSet,
    }

    impl Fake {
        fn new(fail: bool, allowed: &[Permission]) -> Self {
            Self {
                fail,
                permissions: PermissionSet::from_declared(allowed),
            }
        }
    }

    impl BuiltinCapability for Fake {
        fn plugin_id(&self) -> &str {
            "dev.mclink.builtin.fake"
        }
        fn kind(&self) -> PluginKind {
            PluginKind::Adapter
        }
        fn permissions(&self) -> &PermissionSet {
            &self.permissions
        }
        fn invoke(&self, method: &str, params: Value) -> Result<Value, ErrorInfo> {
            if self.fail {
                return Err(ErrorInfo::new(error_code::INTERNAL, "boom"));
            }
            Ok(json!({ "method": method, "params": params }))
        }
    }

    #[tokio::test]
    async fn builtin_provider_runs_off_the_async_thread() {
        let provider = Provider::Builtin(Arc::new(Fake::new(false, &[Permission::NetConnectAny])));
        let out = provider
            .invoke_checked(Permission::NetConnectAny, "adapter.init", json!({}))
            .await
            .unwrap();
        assert_eq!(out["method"], "adapter.init");
        assert!(provider.is_available());
        assert_eq!(provider.kind(), PluginKind::Adapter);
        assert_eq!(provider.plugin_id(), "dev.mclink.builtin.fake");
    }

    #[tokio::test]
    async fn builtin_provider_surfaces_errors() {
        let provider = Provider::Builtin(Arc::new(Fake::new(true, &[Permission::NetConnectAny])));
        let err = provider
            .invoke("adapter.init", json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, error_code::INTERNAL);
    }

    #[tokio::test]
    async fn builtin_provider_still_enforces_permissions() {
        let provider = Provider::Builtin(Arc::new(Fake::new(false, &[Permission::NetListenLocal])));
        let err = provider
            .invoke_checked(Permission::NetConnectAny, "adapter.init", json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, error_code::FORBIDDEN);
    }
}
