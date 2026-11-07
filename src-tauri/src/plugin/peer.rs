//! 插件间通信（peer 信道）：由**核心中转**的插件 ↔ 插件消息通道。
//!
//! # 为什么必须中转，而不是让插件直连
//!
//! 插件进程之间没有任何身份凭据：它们各自只与核心完成过一次 PSK 挑战-应答。
//! 一旦允许"A 直接连 B 的 socket"，B 就无法知道对端是不是它以为的那个插件，
//! 核心的权限判定、限流、心跳与审计也全部失效——那等于在插件层重开一个
//! 无认证的局域网。因此本模块只做一件事：**把插件发给核心的消息，按插件 ID
//! 转交给另一枚插件**，两端都仍然只信任核心。
//!
//! # 命名空间隔离（这是权限模型不被打穿的关键）
//!
//! 目标插件收到的永远是 `peer.call` / `peer.event` 这两个**固定**的线上方法名，
//! 发起方想调用的方法名只作为**载荷字段**出现：
//!
//! ```text
//! A ──request core.peer.call { to:"B", method:"x.y", params:{...} }──▶ 核心
//! 核心 ──request peer.call { from:"A", method:"x.y", params:{...} }──▶ B
//! B  ──response { ok:true, result:{...} }───────────────────────────▶ 核心 ──▶ A
//! ```
//!
//! 因此 A **不可能**借 B 的身份去执行 `adapter.host.start` 之类的能力方法：
//! 核心从不把 peer 请求当作能力方法派发，`required_for` / 路由 / 回退链都
//! 走不到这条路上。B 只是在自己的 `peer.call` 处理分支里看到一串字符串。
//!
//! # 三道闸门
//!
//! 1. **发起方**必须持有 [`Permission::PeerMessage`]；
//! 2. **目标方**也必须声明 `peer_message`（没声明的插件收不到任何 peer 消息，
//!    避免"被当成工具人"）；
//! 3. **目标方必须已连接**（除非显式 `launch: true`，此时由核心按需拉起）。
//!
//! 另有四道资源闸：嵌套深度、单次超时、全局并发、主题/方法名长度。
//!
//! [`Permission::PeerMessage`]: crate::plugin::permission::Permission::PeerMessage

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::plugin::protocol::{error_code, ErrorInfo, PeerPluginInfo};

/// 目标插件收到的请求方法名（固定，见模块文档）。
pub const PEER_CALL: &str = "peer.call";
/// 目标插件收到的事件主题（固定）。
pub const PEER_EVENT: &str = "peer.event";

/// 方法名与主题名的长度上限。
pub const MAX_METHOD_LEN: usize = 96;
pub const MAX_TOPIC_LEN: usize = 128;

/// 允许的最大嵌套深度。
///
/// A→B→C→… 的链路本身靠"每次中转都起一个任务"避免了死锁，但没有上限时
/// 两个互相调用的插件可以无限乒乓，把核心的 CPU 吃满。8 层足够覆盖
/// "适配器 → 房间目录 → 成员索引"这类真实编排。
pub const MAX_DEPTH: u32 = 8;

/// 单次 peer 调用的默认超时与上限（毫秒）。
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
pub const MAX_TIMEOUT_MS: u64 = 30_000;

/// 不能被 peer 借用的前缀：这些属于核心能力方法或 peer 自身。
const RESERVED_PREFIXES: &[&str] = &[
    "core.",
    "peer.",
    "adapter.",
    "detector.",
    "coupler.",
    "bridge.",
    "widget.",
    "utility.",
];

/// 全局自增的调用序号，仅用于日志与排错对账。
static TRACE_SEQ: AtomicU64 = AtomicU64::new(1);

/// 一次 peer 调用的请求参数。
#[derive(Debug, Clone)]
pub struct CallRequest {
    /// 目标插件 ID。
    pub to: String,
    /// 目标插件侧的方法名（私有命名空间，不得借用能力方法前缀）。
    pub method: String,
    /// 透传给目标插件的参数。
    pub params: Value,
    /// 当前嵌套深度（0 = 由发起方直接调用）。
    pub depth: u32,
    /// 目标未连接时是否请求核心按需拉起。
    pub launch: bool,
    /// 超时（毫秒），会被夹取到 `100..=MAX_TIMEOUT_MS`。
    pub timeout_ms: u64,
}

impl CallRequest {
    /// 超时夹取：清单与请求参数都是"自述"的，不能由它们决定核心等多久。
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.clamp(100, MAX_TIMEOUT_MS))
    }

    /// 下一跳的深度。
    pub fn next_depth(&self) -> u32 {
        self.depth.saturating_add(1)
    }

    /// 组装目标插件收到的 `peer.call` 载荷。
    pub fn wire_payload(&self, from: &str, trace: &str) -> Value {
        serde_json::json!({
            "from": from,
            "method": self.method,
            "params": self.params,
            "depth": self.next_depth(),
            "trace": trace,
        })
    }
}

/// 会话侧发给中枢的操作。
///
/// 之所以走通道而不是直接持 `Arc<PluginManager>`：管理器持有全部会话，
/// 会话再持管理器就成了强引用环（永不释放）。通道是单向的，天然无环。
#[derive(Debug)]
pub enum PeerOp {
    /// 调用另一枚插件。
    Call {
        from: String,
        request: CallRequest,
        reply: oneshot::Sender<Result<Value, ErrorInfo>>,
    },
    /// 一次性广播（或定向投递）事件，不需要回执。
    Publish {
        from: String,
        to: Option<String>,
        topic: String,
        data: Value,
    },
    /// 列出当前可见的同机插件。
    List {
        from: String,
        reply: oneshot::Sender<Vec<PeerPluginInfo>>,
    },
}

/// 会话侧持有的 peer 通道句柄（可廉价克隆，随会话一起投递）。
#[derive(Clone)]
pub struct PeerHandle {
    tx: mpsc::UnboundedSender<PeerOp>,
}

impl PeerHandle {
    pub fn new(tx: mpsc::UnboundedSender<PeerOp>) -> Self {
        Self { tx }
    }

    /// 建一对句柄与接收端（测试与装配用）。
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<PeerOp>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// 调用另一枚插件并等待结果。
    pub async fn call(&self, from: &str, request: CallRequest) -> Result<Value, ErrorInfo> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PeerOp::Call {
                from: from.to_string(),
                request,
                reply,
            })
            .map_err(|_| unavailable())?;
        rx.await.map_err(|_| unavailable())?
    }

    /// 广播/定向投递事件，不等待任何回执。
    pub fn publish(
        &self,
        from: &str,
        to: Option<String>,
        topic: &str,
        data: Value,
    ) -> Result<(), ErrorInfo> {
        self.tx
            .send(PeerOp::Publish {
                from: from.to_string(),
                to,
                topic: topic.to_string(),
                data,
            })
            .map_err(|_| unavailable())
    }

    /// 列出同机可见插件（不含自己）。
    pub async fn list(&self, from: &str) -> Result<Vec<PeerPluginInfo>, ErrorInfo> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PeerOp::List {
                from: from.to_string(),
                reply,
            })
            .map_err(|_| unavailable())?;
        rx.await.map_err(|_| unavailable())
    }
}

/// 生成本次调用的追踪号（日志里用它把 A/B 两侧的同一跳对上）。
pub fn next_trace() -> String {
    format!("t{}", TRACE_SEQ.fetch_add(1, Ordering::Relaxed))
}

/// 校验 peer 方法名。
///
/// 规则：非空、≤[`MAX_METHOD_LEN`]、仅 `[a-z0-9._-]`、且不落在保留前缀里。
/// 保留前缀的意义是**心智模型清晰**：读到 `peer.call { method: "adapter.x" }`
/// 时不该有人怀疑"是不是能借道调用适配器"。
pub fn validate_method(method: &str) -> Result<(), ErrorInfo> {
    let m = method.trim();
    if m.is_empty() || m.len() > MAX_METHOD_LEN {
        return Err(bad_request(format!(
            "peer 方法名非法（长度 1..={}）",
            MAX_METHOD_LEN
        )));
    }
    if !m
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
    {
        return Err(bad_request(
            "peer 方法名只允许小写字母、数字与 . - _".to_string(),
        ));
    }
    if let Some(prefix) = RESERVED_PREFIXES.iter().find(|p| m.starts_with(**p)) {
        return Err(bad_request(format!(
            "peer 方法名不得使用保留前缀 `{}`（那是核心能力方法的命名空间）",
            prefix
        )));
    }
    Ok(())
}

/// 校验 peer 事件主题：非空、≤[`MAX_TOPIC_LEN`]、不含控制字符。
pub fn validate_topic(topic: &str) -> Result<(), ErrorInfo> {
    let t = topic.trim();
    if t.is_empty() || t.chars().count() > MAX_TOPIC_LEN {
        return Err(bad_request(format!(
            "peer 主题非法（长度 1..={}）",
            MAX_TOPIC_LEN
        )));
    }
    if t.chars().any(|c| c.is_control()) {
        return Err(bad_request("peer 主题不得包含控制字符".to_string()));
    }
    Ok(())
}

fn bad_request(message: String) -> ErrorInfo {
    ErrorInfo::new(error_code::BAD_REQUEST, message)
}

fn unavailable() -> ErrorInfo {
    ErrorInfo::new(error_code::UNAVAILABLE, "peer 通道已关闭")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_rejects_reserved_prefixes() {
        for bad in [
            "adapter.host.start",
            "core.exec",
            "peer.call",
            "widget.mount",
        ] {
            assert!(validate_method(bad).is_err(), "{bad} 应被拒");
        }
        for ok in ["x.y", "room.directory.list", "state_changed"] {
            assert!(validate_method(ok).is_ok(), "{ok} 应通过");
        }
    }

    #[test]
    fn method_rejects_malformed_input() {
        assert!(validate_method("").is_err());
        assert!(validate_method("   ").is_err());
        assert!(validate_method("Upper.Case").is_err());
        assert!(validate_method("a/b").is_err());
        assert!(validate_method(&"a".repeat(MAX_METHOD_LEN + 1)).is_err());
    }

    #[test]
    fn topic_bounds_and_control_chars() {
        assert!(validate_topic("room.changed").is_ok());
        assert!(validate_topic("").is_err());
        assert!(validate_topic(&"t".repeat(MAX_TOPIC_LEN + 1)).is_err());
        assert!(validate_topic("bad\nline").is_err());
        // 中文等非 ASCII 主题是允许的（长度按字符算）
        assert!(validate_topic("房间.变化").is_ok());
    }

    #[test]
    fn timeout_is_clamped() {
        let mut req = CallRequest {
            to: "dev.b.b".into(),
            method: "x.y".into(),
            params: Value::Null,
            depth: 0,
            launch: false,
            timeout_ms: 10,
        };
        assert_eq!(req.timeout(), Duration::from_millis(100));
        req.timeout_ms = u64::MAX;
        assert_eq!(req.timeout(), Duration::from_millis(MAX_TIMEOUT_MS));
    }

    #[test]
    fn wire_payload_carries_depth_and_origin() {
        let req = CallRequest {
            to: "dev.b.b".into(),
            method: "room.directory".into(),
            params: serde_json::json!({ "k": 1 }),
            depth: 2,
            launch: false,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        };
        let payload = req.wire_payload("dev.a.a", "t7");
        assert_eq!(payload["from"], "dev.a.a");
        assert_eq!(payload["method"], "room.directory");
        assert_eq!(payload["depth"], 3, "目标侧看到的是下一跳深度");
        assert_eq!(payload["trace"], "t7");
    }

    #[tokio::test]
    async fn call_roundtrips_through_the_channel() {
        let (handle, mut rx) = PeerHandle::channel();
        tokio::spawn(async move {
            while let Some(op) = rx.recv().await {
                if let PeerOp::Call {
                    from,
                    request,
                    reply,
                } = op
                {
                    let _ = reply.send(Ok(serde_json::json!({
                        "from": from,
                        "to": request.to,
                    })));
                }
            }
        });
        let out = handle
            .call(
                "dev.a.a",
                CallRequest {
                    to: "dev.b.b".into(),
                    method: "x.y".into(),
                    params: Value::Null,
                    depth: 0,
                    launch: false,
                    timeout_ms: 1_000,
                },
            )
            .await
            .unwrap();
        assert_eq!(out["from"], "dev.a.a");
        assert_eq!(out["to"], "dev.b.b");
    }

    #[tokio::test]
    async fn call_fails_when_hub_is_gone() {
        let (handle, rx) = PeerHandle::channel();
        drop(rx);
        let err = handle
            .call(
                "dev.a.a",
                CallRequest {
                    to: "dev.b.b".into(),
                    method: "x.y".into(),
                    params: Value::Null,
                    depth: 0,
                    launch: false,
                    timeout_ms: 1_000,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, error_code::UNAVAILABLE);
    }
}
