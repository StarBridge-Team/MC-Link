//! # Pending Inbox：纯内存可靠投递
//!
//! 每个队伍拥有一个待收消息池。消息送达后被 ACK 移除，
//! 未 ACK 的消息在 TTL 到达后清理。新成员上线时拉取所有待收消息。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::protocol::PendingMessage;

/// 队伍 ID → 消息池
type InboxMap = HashMap<String, Vec<PendingMessage>>;

/// 线程安全的待收消息存储器
#[derive(Clone)]
pub struct InboxStore {
    inner: Arc<RwLock<InboxMap>>,
    ttl: Duration,
}

impl InboxStore {
    /// 创建新的存储器，指定消息 TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    /// 插入一条待收消息，返回该消息（已补充 TTL 元数据）
    pub async fn insert(&self, team_id: &str, mut msg: PendingMessage) -> PendingMessage {
        msg.ttl = self.ttl;
        msg.created_at = std::time::Instant::now();

        let mut map = self.inner.write().await;
        map.entry(team_id.to_string()).or_default().push(msg.clone());
        msg
    }

    /// 批量 ACK 移除消息，返回实际移除了多少条
    pub async fn acknowledge(&self, team_id: &str, msg_ids: &[String]) -> usize {
        let mut map = self.inner.write().await;
        let Some(inbox) = map.get_mut(team_id) else {
            return 0;
        };

        let before = inbox.len();
        inbox.retain(|m| !msg_ids.contains(&m.msg_id));
        before - inbox.len()
    }

    /// 取出并清空队伍的所有待收消息（成员上线拉取后清空）
    pub async fn drain(&self, team_id: &str) -> Vec<PendingMessage> {
        let mut map = self.inner.write().await;
        map.remove(team_id).unwrap_or_default()
    }

    /// 清除所有队伍中已过期的消息（返回本次清理的总数）
    pub async fn cleanup_expired(&self) -> usize {
        let mut map = self.inner.write().await;
        let mut total = 0;

        map.retain(|_team_id, inbox| {
            let before = inbox.len();
            inbox.retain(|m| !m.is_expired());
            total += before - inbox.len();
            !inbox.is_empty()
        });

        total
    }
}

/// 启动周期性 TTL 清理任务
pub fn spawn_ttl_cleaner(inbox: InboxStore, interval: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;
            let count = inbox.cleanup_expired().await;
            if count > 0 {
                log::info!("[清理] 过期消息 {} 条", count);
            }
        }
    });
}

const TTL_SECS: u64 = 300;

impl Default for InboxStore {
    fn default() -> Self {
        Self::new(std::time::Duration::from_secs(TTL_SECS))
    }
}