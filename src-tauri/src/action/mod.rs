//! 应用打开时动作管理器。
//!
//! 职责：维护一组"打开动作"，在应用启动完成后按注册顺序依次执行。
//! 目前注册的打开动作是「扫描局域网内已开启的游戏」，实际扫描由插件系统里所有
//! 已启用的检测类(detector)插件完成，核心只负责编排。

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::plugin::manager::PluginManager;
use crate::plugin::protocol::LocalGameFound;

/// 单个打开动作：拿到 `AppHandle`，跑一段异步逻辑。
/// 用 `Arc` 包裹以便复制（无需在持锁期间跨 await）。
pub type OpenAction = Arc<
    dyn Fn(AppHandle) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
>;

/// 动作管理器：持有打开动作列表，应用打开时统一触发。
pub struct ActionManager {
    open_actions: RwLock<Vec<OpenAction>>,
}

impl ActionManager {
    pub fn new() -> Self {
        Self {
            open_actions: RwLock::new(Vec::new()),
        }
    }

    /// 注册一个打开动作。
    pub fn register_open(&self, action: OpenAction) {
        if let Ok(mut v) = self.open_actions.write() {
            v.push(action);
        }
    }

    /// 依次执行所有打开动作（共享同一个 AppHandle）。
    pub async fn run_open_actions(&self, app: AppHandle) {
        // 取出列表后立即释放读锁，避免持锁跨 await（RwLockReadGuard 非 Send）。
        let actions: Vec<OpenAction> = self
            .open_actions
            .read()
            .map(|v| v.clone())
            .unwrap_or_default();
        for action in actions {
            action(app.clone()).await;
        }
    }
}

/// 注册内置的打开动作。
pub fn register_builtin_open_actions(mgr: &ActionManager) {
    mgr.register_open(Arc::new(|app: AppHandle| {
        Box::pin(async move {
            let _ = app.emit("local-game-status", json!({ "status": "scanning" }));
            let found: Vec<LocalGameFound> = {
                // 必须取 `Arc<PluginManager>`：`lib.rs` 注册的就是 `Arc<PluginManager>`，
                // 向 Tauri 要裸 `PluginManager` 会 panic（`state() called before manage()`），
                // 整个动作随之静默失效——表现为首页扫描永远不出结果。
                // 取完立刻 clone 出 `Arc`，不把 `State` 守卫带过 `.await`。
                let mgr: Arc<PluginManager> = app.state::<Arc<PluginManager>>().inner().clone();
                mgr.scan_local_games().await
            };
            for item in &found {
                let _ = app.emit(
                    "local-game-found",
                    serde_json::to_value(item).unwrap_or(Value::Null),
                );
            }
            let _ = app.emit(
                "local-game-status",
                json!({ "status": "done", "count": found.len() }),
            );
        })
    }));
}
