//! Illusion 客户端注册表（同步版本）
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion\src\server\registry.rs

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::lock_or_recover;

/// 代理任务：通知客户端建立新代理连接
#[derive(Debug)]
pub struct ProxyTask {
    pub conn_id: u64,
    pub domain: String,
}

/// 挂起的访问者连接，等待客户端数据通道来领取
pub struct PendingConn {
    pub stream: TcpStream,
    pub domain: String,
}

struct RegistryInner {
    /// domain → (client_id, domains)
    domain_map: HashMap<String, u64>,
    /// client_id → (domain list, token)
    clients: HashMap<u64, (Vec<String>, String)>,
    /// conn_id → pending visitor connection
    pending: HashMap<u64, PendingConn>,
    /// Notifier channels: client_id → sender for NewProxy tasks
    notifiers: HashMap<u64, std::sync::mpsc::Sender<ProxyTask>>,
}

/// 域名注册表（线程安全）
pub struct Registry {
    inner: Arc<Mutex<RegistryInner>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RegistryInner {
                domain_map: HashMap::new(),
                clients: HashMap::new(),
                pending: HashMap::new(),
                notifiers: HashMap::new(),
            })),
        }
    }

    /// 注册客户端的域名
    pub fn register(
        &self,
        client_id: u64,
        domains: Vec<String>,
        token: String,
        notifier: std::sync::mpsc::Sender<ProxyTask>,
    ) -> Vec<String> {
        let mut inner = lock_or_recover(&self.inner, "[Illusion] registry");
        let mut registered = Vec::new();

        for domain in &domains {
            let domain_lower = domain.to_lowercase();
            inner.domain_map.insert(domain_lower.clone(), client_id);
            registered.push(domain.clone());
            log(LogLevel::Info, &format!(
                "[Illusion] 域名 {} → 客户端 {}", domain_lower, client_id
            ));
        }

        inner.clients.insert(client_id, (domains.clone(), token));
        inner.notifiers.insert(client_id, notifier);
        log(LogLevel::Info, &format!(
            "[Illusion] 客户端 {} 注册域名: {:?}", client_id, registered
        ));
        registered
    }

    /// 注销客户端及其所有域名
    pub fn unregister(&self, client_id: u64) {
        let mut inner = lock_or_recover(&self.inner, "[Illusion] registry");
        inner.domain_map.retain(|_, v| *v != client_id);
        inner.clients.remove(&client_id);
        inner.notifiers.remove(&client_id);
        log(LogLevel::Info, &format!("[Illusion] 客户端 {} 已注销", client_id));
    }

    /// 验证 token 有效性
    pub fn validate_token(&self, token: &str) -> Option<u64> {
        let inner = lock_or_recover(&self.inner, "[Illusion] registry");
        // Search for a client with matching token
        inner.clients.iter()
            .find(|(_, (_, t))| t == token)
            .map(|(id, _)| *id)
    }

    /// 根据域名查找客户端，获取通知通道
    pub fn find_client(&self, domain: &str) -> Option<std::sync::mpsc::Sender<ProxyTask>> {
        let inner = lock_or_recover(&self.inner, "[Illusion] registry");
        let domain_lower = domain.to_lowercase();

        // Try exact match first
        if let Some(&client_id) = inner.domain_map.get(&domain_lower) {
            return inner.notifiers.get(&client_id).cloned();
        }

        // Try wildcard: *.example.com
        if let Some(dot_pos) = domain_lower.find('.') {
            let wildcard = format!("*{}", &domain_lower[dot_pos..]);
            if let Some(&client_id) = inner.domain_map.get(&wildcard) {
                return inner.notifiers.get(&client_id).cloned();
            }
        }

        None
    }

    /// 存储挂起的访问者连接
    pub fn store_pending(&self, conn_id: u64, stream: TcpStream, domain: String) {
        let mut inner = lock_or_recover(&self.inner, "[Illusion] registry");
        inner.pending.insert(conn_id, PendingConn { stream, domain });
    }

    /// 取出挂起的访问者连接（数据通道到达时）
    pub fn take_pending(&self, conn_id: u64) -> Option<PendingConn> {
        let mut inner = lock_or_recover(&self.inner, "[Illusion] registry");
        inner.pending.remove(&conn_id)
    }

    /// 删除挂起的访问者连接
    pub fn remove_pending(&self, conn_id: u64) {
        let mut inner = lock_or_recover(&self.inner, "[Illusion] registry");
        inner.pending.remove(&conn_id);
    }
}
