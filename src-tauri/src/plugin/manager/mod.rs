//! 插件管理器：调度中枢。
//!
//! 职责边界：
//!
//! | 该做 | 不该做 |
//! |---|---|
//! | 加载清单、维护注册表与生效权限 | 解析游戏协议 |
//! | 启动网关、拉起插件进程、管理会话 | 实现打洞算法 |
//! | 按"能力 + 游戏"路由并执行回退链 | 直接读写游戏的存档/配置 |
//! | 把插件事件转成前端可见通知 | 替插件访问网络或文件 |
//!
//! 换句话说：管理器只做**编排**，任何实际动作都必须由插件以能力方法的形式提供，
//! 并在核心侧通过权限校验。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as SyncMutex, RwLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::adapter::AdapterManager;
use crate::plugin::builtin::{TerracottaProvider, TERRACOTTA_PLUGIN_ID};
use crate::plugin::capability::Provider;
use crate::plugin::game::GameRegistry;
use crate::plugin::gateway::{self, AuthTable, Gateway};
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::{required_for, Permission};
use crate::plugin::protocol::{error_code, ErrorInfo};
use crate::plugin::registry::{PluginRecord, PluginRegistry, PluginSource};
use crate::plugin::router::{self, RoutePlan};
use crate::plugin::session::{Inbound, InboundTx};

/// 插件管理器。
pub struct PluginManager {
    inner: Arc<Inner>,
}

struct Inner {
    data_dir: PathBuf,
    registry: RwLock<PluginRegistry>,
    games: RwLock<GameRegistry>,
    /// 已就绪的能力提供者（内置 + 已握手的远程插件）。
    providers: RwLock<HashMap<String, Provider>>,
    /// 已拉起的外部插件进程。
    ///
    /// **必须持有 `Child`**：只记 pid 的话，既无法 `wait` 回收（Unix 上留下僵尸进程），
    /// 也无法在退出或握手超时时 `kill`（Windows 上应用退了插件还在跑）。
    children: RwLock<HashMap<String, std::process::Child>>,
    /// "拉起插件 + 等握手"的串行锁。
    ///
    /// 两个并发 invoke 若各自 spawn，会得到两个进程：后者覆盖 `providers` 里的前者，
    /// 前者就成了没人管的孤儿。加锁后重查一次缓存即可（双检），不必引入更重的东西。
    launch_lock: tokio::sync::Mutex<()>,
    gateway: RwLock<Option<Arc<Gateway>>>,
    auth: AuthTable,
    inbound_tx: InboundTx,
    inbound_rx: SyncMutex<Option<mpsc::UnboundedReceiver<Inbound>>>,
    shutdown: Arc<AtomicBool>,
    app: RwLock<Option<AppHandle>>,
    adapter: Arc<SyncMutex<AdapterManager>>,
    warnings: RwLock<Vec<String>>,
}

impl PluginManager {
    /// 同步构造：加载注册表、内置插件与游戏画像，但不启动网络。
    pub fn new(data_dir: &Path) -> Result<Arc<Self>, String> {
        let adapter = Arc::new(SyncMutex::new(AdapterManager::new(data_dir)));
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();

        let mut registry = PluginRegistry::new(data_dir);
        let mut warnings = registry.reload(builtin::builtin_records()?);

        let mut games = GameRegistry::with_builtins();
        warnings.extend(games.load_dir(&data_dir.join("Games")));

        let mut providers: HashMap<String, Provider> = HashMap::new();
        providers.insert(
            TERRACOTTA_PLUGIN_ID.to_string(),
            Provider::Builtin(Arc::new(TerracottaProvider::new(adapter.clone())?)),
        );

        let inner = Inner {
            data_dir: data_dir.to_path_buf(),
            registry: RwLock::new(registry),
            games: RwLock::new(games),
            providers: RwLock::new(providers),
            children: RwLock::new(HashMap::new()),
            launch_lock: tokio::sync::Mutex::new(()),
            gateway: RwLock::new(None),
            auth: Arc::new(RwLock::new(HashMap::new())),
            inbound_tx,
            inbound_rx: SyncMutex::new(Some(inbound_rx)),
            shutdown: Arc::new(AtomicBool::new(false)),
            app: RwLock::new(None),
            adapter,
            warnings: RwLock::new(warnings),
        };

        Ok(Arc::new(Self {
            inner: Arc::new(inner),
        }))
    }

    pub fn data_dir(&self) -> &Path {
        &self.inner.data_dir
    }

    /// 绑定前端事件通道（可在任意时刻调用）。
    pub fn attach_app(&self, app: AppHandle) {
        if let Ok(mut slot) = self.inner.app.write() {
            *slot = Some(app);
        }
    }

    /// 异步启动：绑定回环网关、刷新准入表、开始接收插件连接。
    pub async fn start(self: &Arc<Self>) -> Result<(), String> {
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return Err("插件管理器已关闭".to_string());
        }
        if self
            .inner
            .gateway
            .read()
            .map(|g| g.is_some())
            .unwrap_or(false)
        {
            return Ok(());
        }

        let gateway = Arc::new(Gateway::bind(0).await?);
        self.refresh_auth()?;

        if let Ok(mut slot) = self.inner.gateway.write() {
            *slot = Some(gateway.clone());
        }

        let rx = self.inner.inbound_rx.lock().ok().and_then(|mut g| g.take());

        if let Some(rx) = rx {
            let inner = self.inner.clone();
            tauri::async_runtime::spawn(inbound::consume_inbound(inner, rx));
        }

        let auth = self.inner.auth.clone();
        let inbound = self.inner.inbound_tx.clone();
        let shutdown = self.inner.shutdown.clone();
        tauri::async_runtime::spawn(async move {
            gateway::serve(gateway, auth, inbound, shutdown).await;
        });

        println!("[插件] 网关已就绪（仅回环）");
        Ok(())
    }

    /// 关闭全部插件并停止网关。
    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        let providers: Vec<Provider> = self
            .inner
            .providers
            .read()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        for provider in providers {
            provider.close("核心正在退出");
            if let Provider::Builtin(builtin) = &provider {
                builtin.shutdown();
            }
        }
        // 结束仍存活的外部插件进程：只 `close` 会话是不够的——插件可以不理会，
        // 或者根本没握手成功。Windows 上它们会活过本进程，Unix 上会成为僵尸。
        if let Ok(mut children) = self.inner.children.write() {
            for (id, mut child) in children.drain() {
                let _ = child.kill();
                let _ = child.wait();
                println!("[插件] 已结束 {} 的进程（核心退出）", id);
            }
        }
        if let Ok(mut slot) = self.inner.gateway.write() {
            *slot = None;
        }
    }

    // ---------------------------------------------------------------- 注册表

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
        self.inner
            .registry
            .read()
            .map(|r| r.list().into_iter().cloned().collect())
            .unwrap_or_default()
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

    /// 网关运行状态（不回传令牌）。
    pub fn gateway_info(&self) -> Value {
        let gateway = self.inner.gateway.read().ok().and_then(|g| g.clone());
        match gateway {
            Some(g) => json!({
                "running": true,
                "port": g.port(),
                "endpoint": g.endpoint(),
                "externalPlugins": self
                    .inner
                    .providers
                    .read()
                    .map(|m| m.values().filter(|p| matches!(p, Provider::Remote(_))).count())
                    .unwrap_or(0),
            }),
            None => json!({ "running": false, "port": Value::Null }),
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        self.inner
            .warnings
            .read()
            .map(|w| w.clone())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------- 游戏画像

    pub fn games(&self) -> Vec<crate::plugin::game::GameProfile> {
        self.inner
            .games
            .read()
            .map(|g| g.all().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------- 路由

    pub fn plan(&self, kind: PluginKind, game_id: Option<&str>) -> RoutePlan {
        let registry = match self.inner.registry.read() {
            Ok(r) => r,
            Err(_) => {
                return RoutePlan {
                    kind,
                    game_id: game_id.map(|s| s.to_string()),
                    candidates: Vec::new(),
                }
            }
        };
        let games = match self.inner.games.read() {
            Ok(g) => g,
            Err(_) => {
                return RoutePlan {
                    kind,
                    game_id: game_id.map(|s| s.to_string()),
                    candidates: Vec::new(),
                }
            }
        };
        router::plan(&registry, &games, kind, game_id)
    }

    /// 获取指定插件的提供者，必要时启动外部进程并等待握手完成。
    pub async fn provider(&self, plugin_id: &str) -> Result<Provider, ErrorInfo> {
        if let Some(existing) = self.cached_provider(plugin_id) {
            return Ok(existing);
        }

        let Some(record) = self.get(plugin_id) else {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("插件未注册: {}", plugin_id),
            ));
        };
        if !record.enabled || !record.trust.is_runnable() {
            return Err(ErrorInfo::new(
                error_code::FORBIDDEN,
                format!("插件 {} 未启用或已被拉黑", plugin_id),
            ));
        }
        if record.source != PluginSource::External {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("内置插件 {} 尚未注册提供者", plugin_id),
            ));
        }

        // 串行化"拉起 + 等握手"：并发 invoke 各自 spawn 的话，后来的会覆盖
        // providers 里的前一个，前一个即成为无人管理的孤儿进程。
        let _launch_guard = self.inner.launch_lock.lock().await;
        // 双检：等锁期间可能已有别的调用把它拉起来并握手完成了
        if let Some(existing) = self.cached_provider(plugin_id) {
            return Ok(existing);
        }

        self.launch_external(&record).await?;
        match self.wait_ready(plugin_id).await {
            Ok(provider) => Ok(provider),
            Err(e) => {
                // 握手没成功就把刚拉起的进程收掉，否则它会一直挂着等一个不会来的连接
                self.reap_child(plugin_id, "握手未完成");
                Err(e)
            }
        }
    }

    fn cached_provider(&self, plugin_id: &str) -> Option<Provider> {
        self.inner
            .providers
            .read()
            .ok()
            .and_then(|m| m.get(plugin_id).cloned())
            .filter(|p| p.is_available())
    }

    /// 按候选顺序调用能力方法，失败自动降级到下一个插件。
    ///
    /// 每次调用都会先做核心侧权限判定（见 [`required_for`]），因此插件既不能
    /// 靠"没连上"逃避检查，也不能靠回退链绕过检查——每个候选各自独立校验。
    pub async fn invoke(
        &self,
        kind: PluginKind,
        game_id: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<(String, Value), ErrorInfo> {
        let plan = self.plan(kind, game_id);
        if plan.is_empty() {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                format!("没有可用的 {} 类插件", kind.as_str()),
            ));
        }

        let required = required_for(kind, method);
        let mut last_error: Option<ErrorInfo> = None;

        for candidate in &plan.candidates {
            let provider = match self.provider(&candidate.plugin_id).await {
                Ok(p) => p,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            match provider
                .invoke_checked(required, method, params.clone())
                .await
            {
                Ok(value) => return Ok((candidate.plugin_id.clone(), value)),
                Err(e) => {
                    eprintln!(
                        "[插件] {} 执行 {} 失败（{}），尝试回退",
                        provider.plugin_id(),
                        method,
                        e.message
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| ErrorInfo::new(error_code::UNAVAILABLE, "全部候选插件均调用失败")))
    }

    /// 向所有已就绪插件广播上下文（游戏信息 / 房间信息 / 其他插件信息）。
    pub fn broadcast_context(&self, context: &crate::plugin::protocol::PluginContext) {
        let peers = self.peer_summaries();
        let payload = json!({
            "role": context.role,
            "game": context.game,
            "room": context.room,
            "peers": peers,
        });
        let providers: Vec<Provider> = self
            .inner
            .providers
            .read()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        for provider in providers {
            if let Provider::Remote(session) = &provider {
                let _ = session.notify(
                    crate::plugin::protocol::core_method::CONTEXT_UPDATE,
                    payload.clone(),
                );
            }
        }
    }

    /// 汇总其他插件的能力信息，供适配类插件决定如何协作。
    pub fn peer_summaries(&self) -> Vec<crate::plugin::protocol::PeerPluginInfo> {
        self.list()
            .into_iter()
            .filter(|r| r.enabled && r.trust.is_runnable())
            .map(|r| crate::plugin::protocol::PeerPluginInfo {
                plugin_id: r.manifest.id.clone(),
                kind: r.manifest.kind.as_str().to_string(),
                games: r.manifest.games.clone(),
                endpoints: Vec::new(),
            })
            .collect()
    }

    // ---------------------------------------------------------------- 前端事件

    /// 向前端广播 `plugin-event`（未绑定 AppHandle 时静默丢弃）。
    pub fn emit(&self, payload: Value) {
        let app = self.inner.app.read().ok().and_then(|g| g.clone());
        if let Some(app) = app {
            let _ = app.emit("plugin-event", payload);
        }
    }
}

// ---------------------------------------------------------------------------
// 模块划分
//
// `mod.rs` 只放调度中枢本身（生命周期、注册表、路由、前端事件）；
// 内部实现细节按职责拆到子模块。子模块作为 `manager` 的后代可以直接访问
// `Inner` 的私有字段，因此不需要为拆分而放宽任何可见性。
// ---------------------------------------------------------------------------

mod adapter;
mod builtin;
mod inbound;
mod provision;
#[cfg(test)]
mod tests;
