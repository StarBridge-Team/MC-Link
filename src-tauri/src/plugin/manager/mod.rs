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

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as SyncMutex, RwLock};
use tauri::AppHandle;
use tokio::sync::mpsc;

use crate::plugin::builtin::{
    EasyTierProvider, MinecraftCouplerProvider, MinecraftScannerProvider, TerracottaProvider,
    EASYTIER_PLUGIN_ID, MINECRAFT_COUPLER_PLUGIN_ID, MINECRAFT_DETECTOR_PLUGIN_ID,
    TERRACOTTA_PLUGIN_ID,
};
use crate::plugin::capability::Provider;
use crate::plugin::game::GameRegistry;
use crate::plugin::gateway::{self, AuthTable, Gateway};
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::{required_for, Permission};
use crate::plugin::protocol::{error_code, ErrorInfo};
use crate::plugin::registry::{PluginRegistry, PluginSource};
use crate::plugin::localgames::LocalGamesCache;
use crate::plugin::session::{Inbound, InboundTx};

use self::launch::{LaunchAwait, LaunchTicket};

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
    /// peer 中枢的通道句柄（`None` = 尚未 `start`）。
    peer_hub: RwLock<Option<crate::plugin::peer::PeerHandle>>,
    /// 已拉起的外部插件进程。
    ///
    /// **必须持有 `Child`**：只记 pid 的话，既无法 `wait` 回收（Unix 上留下僵尸进程），
    /// 也无法在退出或握手超时时 `kill`（Windows 上应用退了插件还在跑）。
    children: RwLock<HashMap<String, std::process::Child>>,
    /// "拉起插件进程"这一小段的串行锁。
    ///
    /// **只覆盖 `launch_external`**：等待握手（最多 20 秒）在锁外进行，
    /// 否则一个握手失败的插件会把**所有**插件调用都堵住（见 `launch` 模块）。
    ///
    /// 两个并发 invoke 若各自 spawn，会得到两个进程：后者覆盖 `providers` 里的前者，
    /// 前者就成了没人管的孤儿。加锁后重查一次缓存即可（双检），不必引入更重的东西。
    launch_lock: tokio::sync::Mutex<()>,
    /// 正在拉起中的外部插件（插件 ID → 拉起票据）。
    ///
    /// 让同一个插件的并发请求复用同一次拉起，而不是各拉各的进程互相 kill。
    launching: RwLock<HashMap<String, Arc<LaunchTicket>>>,
    gateway: RwLock<Option<Arc<Gateway>>>,
    auth: AuthTable,
    /// 最近一次扫描到的本地游戏，供前端主动拉取（事件补发不了，见 `plugin::localgames`）。
    local_games: LocalGamesCache,
    inbound_tx: InboundTx,
    inbound_rx: SyncMutex<Option<mpsc::UnboundedReceiver<Inbound>>>,
    shutdown: Arc<AtomicBool>,
    app: RwLock<Option<AppHandle>>,
    warnings: RwLock<Vec<String>>,
}

impl PluginManager {
    /// 同步构造：加载注册表、内置插件与游戏画像，但不启动网络。
    pub fn new(data_dir: &Path) -> Result<Arc<Self>, String> {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();

        let mut registry = PluginRegistry::new(data_dir);
        let mut warnings = registry.reload(builtin::builtin_records()?);

        let mut games = GameRegistry::with_builtins();
        warnings.extend(games.load_dir(&data_dir.join("Games")));

        // 内置适配器都只依赖数据目录（引擎安装、机器标识都落在它下面），
        // 不再共享一个"适配器管理器"——那是陶瓦 exe 时代的载体，已随 exe 一起删除。
        let mut providers: HashMap<String, Provider> = HashMap::new();
        providers.insert(
            TERRACOTTA_PLUGIN_ID.to_string(),
            Provider::Builtin(Arc::new(TerracottaProvider::new(data_dir)?)),
        );
        providers.insert(
            EASYTIER_PLUGIN_ID.to_string(),
            Provider::Builtin(Arc::new(EasyTierProvider::new(data_dir)?)),
        );
        providers.insert(
            MINECRAFT_DETECTOR_PLUGIN_ID.to_string(),
            Provider::Builtin(Arc::new(MinecraftScannerProvider::new())),
        );
        providers.insert(
            MINECRAFT_COUPLER_PLUGIN_ID.to_string(),
            Provider::Builtin(Arc::new(MinecraftCouplerProvider::new())),
        );

        let inner = Inner {
            data_dir: data_dir.to_path_buf(),
            registry: RwLock::new(registry),
            games: RwLock::new(games),
            providers: RwLock::new(providers),
            peer_hub: RwLock::new(None),
            children: RwLock::new(HashMap::new()),
            launch_lock: tokio::sync::Mutex::new(()),
            launching: RwLock::new(HashMap::new()),
            gateway: RwLock::new(None),
            auth: Arc::new(RwLock::new(HashMap::new())),
            local_games: LocalGamesCache::new(),
            inbound_tx,
            inbound_rx: SyncMutex::new(Some(inbound_rx)),
            shutdown: Arc::new(AtomicBool::new(false)),
            app: RwLock::new(None),
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

        // peer 中枢：必须在网关开始接受连接前就绪，否则第一批会话拿不到通道
        let peer_hub = self.start_peer_hub();
        if let Ok(mut slot) = self.inner.peer_hub.write() {
            *slot = Some(peer_hub.clone());
        }

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
            gateway::serve(gateway, auth, peer_hub, inbound, shutdown).await;
        });

        println!("[插件] 网关已就绪（仅回环）");

        // 运行时不阻塞启动：只在确有已启用的 Node 插件时后台装，缺省用户零感知
        self.prefetch_node_runtime();
        Ok(())
    }

    /// 关闭全部插件并停止网关。
    ///
    /// # 顺序为什么重要
    ///
    /// 先置 `shutdown` 标志再收进程：`provider()` 会在拉起前检查该标志，
    /// 避免"关闭的同时又有 invoke 拉起一个新进程，然后它落进已被 drain 的 children 表"
    /// ——那会让插件进程活过核心（Windows 上表现为"应用退了插件还在跑"）。
    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        // 关闭期间不允许再拉起新进程：`launch_lock` 把 `provider()` 的
        // "拉起 + 等握手"整段串行化，拿到锁即意味着没有在途的拉起动作，
        // 此时再收进程就不会有新的 `Child` 落进表里。
        let _launch_guard = self.inner.launch_lock.blocking_lock();
        self.close_providers();
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
        // 关掉 peer 中枢：丢弃句柄后中枢任务随通道关闭自然退出，
        // 在途调用会以 `unavailable` 失败而不是永久挂起。
        if let Ok(mut slot) = self.inner.peer_hub.write() {
            *slot = None;
        }
    }

    /// 关闭所有提供者（会话 + 内置资源）。
    ///
    /// 与 `shutdown()` 拆开：`platform_supported` 之外还需要在"停用/拉黑单个插件"时
    /// 复用同一套关闭动作，避免两处逻辑漂移。
    fn close_providers(&self) {
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
    }

    // ---------------------------------------------------------------- 路由与调用

    /// 获取指定插件的提供者，必要时启动外部进程并等待握手完成。
    ///
    /// # 两段式：拉起与握手分开
    ///
    /// - **拉起段**（持 `launch_lock`）：只保护"拉起进程"，避免并发 spawn 出两个进程；
    /// - **握手等待段**（不持锁）：最长 20 秒，期间其它插件的调用照常进行。
    ///
    /// 旧实现把两段串在一起（持锁等握手），一个握手失败的插件会把所有插件
    /// 调用都堵住 20 秒。
    pub async fn provider(&self, plugin_id: &str) -> Result<Provider, ErrorInfo> {
        if let Some(existing) = self.cached_provider(plugin_id) {
            return Ok(existing);
        }

        // 已进入关闭流程就拒绝拉起：否则新进程会落进已被 drain 的 children 表，
        // 活过本进程（Windows 上表现为"应用退了插件还在跑"）。
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return Err(ErrorInfo::new(
                error_code::UNAVAILABLE,
                "核心正在退出，不再启动插件".to_string(),
            ));
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

        // 同一个插件若已有拉起在进行中，复用它（见 `launch` 模块）：
        // 否则并发请求会各拉各的进程，互相 kill，最后谁都握手不成。
        let (ticket, owner) = self.acquire_launch(plugin_id);
        if !owner {
            return match self.await_launch(plugin_id, &ticket).await {
                LaunchAwait::Ready(provider) => Ok(provider),
                LaunchAwait::Failed(error) => Err(error),
                LaunchAwait::Timeout => Err(ErrorInfo::new(
                    error_code::TIMEOUT,
                    format!(
                        "等待插件 {} 握手超时（{} 秒）",
                        plugin_id,
                        launch::LAUNCH_WAIT_TIMEOUT.as_secs()
                    ),
                )),
            };
        }

        // ---- 拉起段：只有这一小段持锁 ----
        {
            let _launch_guard = self.inner.launch_lock.lock().await;
            // 双检：等锁期间可能已有别的调用把它拉起来并握手完成了
            if let Some(existing) = self.cached_provider(plugin_id) {
                self.complete_launch(plugin_id, None);
                return Ok(existing);
            }
            if let Err(e) = self.launch_external(&record).await {
                self.complete_launch(plugin_id, Some(e.clone()));
                self.reap_child(plugin_id, "拉起失败");
                return Err(e);
            }
        }

        // ---- 握手等待段：不持锁 ----
        // 20 秒的等待期间，其它插件的调用不会被堵住。
        match self.wait_ready(plugin_id).await {
            Ok(provider) => {
                self.complete_launch(plugin_id, None);
                Ok(provider)
            }
            Err(e) => {
                self.complete_launch(plugin_id, Some(e.clone()));
                // 握手没成功就把刚拉起的进程收掉，否则它会一直挂着等一个不会来的连接
                self.reap_child(plugin_id, "握手未完成");
                Err(e)
            }
        }
    }

    /// 取已就绪的提供者。
    ///
    /// # 锁中毒时为什么"当作已就绪"而不是返回 `None`
    ///
    /// 返回 `None` 会让 `provider()` 认为插件尚未拉起，于是**再拉起一个进程**——
    /// 而 `children` 表里那个旧进程没人管，直接变成孤儿。返回一个"可疑但存在"的
    /// 句柄反而更安全：调用方会在真正使用时失败，不会复制出进程。
    /// （`plugin/utils.rs` 的 `lock_or_err` 是同一取向：安全状态宁可失败也不要恢复。）
    fn cached_provider(&self, plugin_id: &str) -> Option<Provider> {
        match self.inner.providers.read() {
            Ok(m) => m.get(plugin_id).cloned().filter(|p| p.is_available()),
            Err(_) => self
                .inner
                .providers
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(plugin_id)
                .cloned(),
        }
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

    /// 定向调用**指定**插件的能力方法（不走路由回退链）。
    ///
    /// 用于调用方已经确定要哪个插件的场景（如"遍历所有检测器各扫一次"）。
    /// 走 `invoke` 会重新路由并对全部候选依次尝试，导致重复调用。
    /// 权限判定与 `invoke` 一致：候选身份不构成任何豁免。
    pub async fn invoke_plugin(
        &self,
        plugin_id: &str,
        required: Permission,
        method: &str,
        params: Value,
    ) -> Result<Value, ErrorInfo> {
        let provider = self.provider(plugin_id).await?;
        provider.invoke_checked(required, method, params).await
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
mod frontend;
mod inbound;
mod launch;
mod peer_hub;
mod provision;
mod registry_ops;
#[cfg(test)]
mod tests;
mod widget;
