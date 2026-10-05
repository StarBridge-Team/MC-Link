//! 插件网关：只监听回环地址的 WebSocket 服务端。
//!
//! # 为什么只用回环
//!
//! 插件与核心在同一台机器上，没有任何理由把控制面暴露到网络上。只绑
//! `127.0.0.1` 把"插件协议被远程利用"这一整类风险直接删除；如果未来确实
//! 需要跨机部署插件（例如把计算-heavy 的适配器放到另一台机器），应当新增
//! 一层显式的 TLS + 双向证书配置，而不是放开这里的绑定地址。
//!
//! # 准入两道门
//!
//! 1. **启动令牌**：由核心在拉起插件进程时生成，写入会话文件，插件读取后
//!    在 WebSocket URL 上回传。校验用常量时间比较，防止时序探测。
//! 2. **PSK 挑战-应答**：见 [`crate::plugin::auth`]。

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as SyncMutex, RwLock};
use std::time::Duration;

use tokio::net::TcpListener;

use crate::plugin::auth::FailureTracker;
use crate::plugin::crypto;
use crate::plugin::manifest::LimitsSpec;
use crate::plugin::permission::PermissionSet;
use crate::plugin::session::{self, InboundTx};

/// 连接准入所需的静态信息。全部来自核心内的清单与注册表，绝不来自插件自述。
#[derive(Clone)]
pub struct AuthEntry {
    pub plugin_id: String,
    pub psk: [u8; 32],
    pub permissions: PermissionSet,
    pub limits: LimitsSpec,
    /// 该插件连续握手失败多少次后进入冷却。
    pub max_auth_failures: u32,
}

/// 插件 ID → 准入信息。
pub type AuthTable = Arc<RwLock<HashMap<String, AuthEntry>>>;

/// 握手冷却时长。
const AUTH_COOLDOWN: Duration = Duration::from_secs(60);

/// 同时处理的入站连接上限。
///
/// 插件只有本机那几个（每个一条长连接），给 32 已远超正常用量。没有上限时，
/// 同机任意进程都能循环连网关，每连一次就起一个 tokio 任务并分配握手缓冲，
/// 直到把任务/内存耗尽（DoS）。上限之外的连接直接拒绝，不排队。
const MAX_INFLIGHT_CONNECTIONS: usize = 32;

/// 网关实例。
pub struct Gateway {
    listener: TcpListener,
    port: u16,
    /// 一次性启动令牌（每次核心启动重新生成）。
    token: String,
}

impl Gateway {
    /// 绑定回环地址。
    ///
    /// `preferred_port` 为 0 时由系统分配空闲端口。
    pub async fn bind(preferred_port: u16) -> Result<Self, String> {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), preferred_port);
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| format!("绑定插件网关失败 {}: {}", addr, e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("读取网关地址失败: {}", e))?
            .port();
        Ok(Self {
            listener,
            port,
            token: crypto::random_hex(32),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// 插件的连接地址模板。
    pub fn endpoint(&self) -> String {
        format!("ws://127.0.0.1:{}/plugin", self.port)
    }

    /// 生成某个插件专用的连接地址（含令牌与插件标识）。
    pub fn endpoint_for(&self, plugin_id: &str) -> String {
        format!(
            "{}?plugin={}&token={}",
            self.endpoint(),
            plugin_id,
            self.token
        )
    }
}

/// 在后台持续接受插件连接。
///
/// 该函数会一直运行到 `shutdown` 置位；调用方负责用 `tokio::spawn` 驱动它。
pub async fn serve(
    gateway: Arc<Gateway>,
    auth: AuthTable,
    inbound: InboundTx,
    shutdown: Arc<AtomicBool>,
) {
    let tracker = Arc::new(SyncMutex::new(FailureTracker::new()));
    let token = gateway.token.clone();
    // 在途连接计数：许可在连接处理结束时归还。
    let inflight = Arc::new(tokio::sync::Semaphore::new(MAX_INFLIGHT_CONNECTIONS));

    while !shutdown.load(Ordering::SeqCst) {
        let accepted = tokio::select! {
            r = gateway.listener.accept() => r,
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                continue;
            }
        };

        let (stream, peer) = match accepted {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[插件网关] 接受连接失败: {}", e);
                continue;
            }
        };

        // 纵深防御：即使绑定被误改为 0.0.0.0，也拒绝非回环来源。
        if !peer.ip().is_loopback() {
            eprintln!("[插件网关] 拒绝非回环连接: {}", peer);
            continue;
        }

        // 超过并发上限直接拒绝：不排队，避免"任务/内存被无界连接耗尽"。
        let Ok(permit) = inflight.clone().try_acquire_owned() else {
            eprintln!("[插件网关] 并发连接已达上限（{}），拒绝 {}", MAX_INFLIGHT_CONNECTIONS, peer);
            continue;
        };

        let auth = auth.clone();
        let inbound = inbound.clone();
        let token = token.clone();
        let tracker = tracker.clone();

        tokio::spawn(async move {
            // permit 随任务结束释放
            let _permit = permit;
            handle_connection(stream, token, auth, inbound, tracker).await;
        });
    }

    println!("[插件网关] 已停止监听");
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    token: String,
    auth: AuthTable,
    inbound: InboundTx,
    tracker: Arc<SyncMutex<FailureTracker>>,
) {
    let accepted = match session::accept_ws(stream, token).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[插件网关] 准入失败: {}", e);
            return;
        }
    };

    let entry = auth
        .read()
        .ok()
        .and_then(|m| m.get(&accepted.plugin_id).cloned());
    let Some(entry) = entry else {
        // 未知 ID 也要计数：否则"猜一个未登记的 plugin_id + 乱填令牌"的连接
        // 永远不进入冷却，可以无限重试（`FailureTracker` 只按 plugin_id 记）。
        // 这里按插件 ID 记一次失败，冷却逻辑与已登记插件一致。
        let count = tracker
            .lock()
            .map(|mut t| t.record_failure(&accepted.plugin_id))
            .unwrap_or(0);
        eprintln!(
            "[插件网关] 未知或未启用的插件尝试连接: {}（累计 {} 次）",
            accepted.plugin_id, count
        );
        return;
    };

    let cooling = tracker
        .lock()
        .map(|t| t.is_cooling_down(&entry.plugin_id, entry.max_auth_failures, AUTH_COOLDOWN))
        .unwrap_or(false);
    if cooling {
        eprintln!(
            "[插件网关] 插件 {} 处于握手冷却期，拒绝连接",
            entry.plugin_id
        );
        return;
    }

    match session::serve(
        accepted.ws,
        entry.plugin_id.clone(),
        entry.psk,
        entry.permissions,
        entry.limits,
        inbound,
    )
    .await
    {
        Ok(_) => {
            if let Ok(mut t) = tracker.lock() {
                t.record_success(&entry.plugin_id);
            }
        }
        Err(e) => {
            let count = tracker
                .lock()
                .map(|mut t| t.record_failure(&entry.plugin_id))
                .unwrap_or(0);
            eprintln!(
                "[插件网关] 插件 {} 建连失败（累计 {} 次）: {}",
                entry.plugin_id, count, e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn gateway_binds_to_loopback_only() {
        let gw = Gateway::bind(0).await.unwrap();
        assert!(gw.port() > 0);
        assert!(gw.endpoint().starts_with("ws://127.0.0.1:"));
        assert!(gw.endpoint_for("dev.a.b").contains("token="));
    }

    #[tokio::test]
    async fn gateway_tokens_are_unique_per_instance() {
        let a = Gateway::bind(0).await.unwrap();
        let b = Gateway::bind(0).await.unwrap();
        assert_ne!(a.token(), b.token());
    }
}
