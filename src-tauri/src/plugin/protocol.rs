//! 插件控制面协议（WebSocket / JSON）。
//!
//! # 分层
//!
//! ```text
//! ┌─ 握手层（明文 JSON，一次性） ──────────────────────────┐
//! │  hello  →  hello_ack  →  auth_ok                      │
//! └───────────────────────────────────────────────────────┘
//! ┌─ 会话层（SecureFrame 信封，AES-256-GCM）───────────────┐
//! │  request / response / event / ping / pong / bye       │
//! └───────────────────────────────────────────────────────┘
//! ```
//!
//! # 为什么控制面不等于数据面
//!
//! 控制面只承载**编排指令**（谁打洞、往哪连、拉起了哪个进程），不承载游戏流量。
//! 游戏数据由插件自己的 socket 直接转发，因此控制面使用 JSON 文本帧带来的
//! 体积开销可以忽略，却换来协议可读、可审计、易实现的收益。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// 协议版本。不匹配时核心直接拒绝连接。
pub const PROTOCOL_VERSION: u16 = 1;

/// 单帧最大字节数（含信封）。超出即判定为恶意/异常并断开。
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// 握手阶段单帧上限（握手不该携带大数据）。
pub const MAX_HANDSHAKE_BYTES: usize = 16 * 1024;

/// WebSocket 子协议名。
pub const WS_SUBPROTOCOL: &str = "mclink-plugin";

/// 核心 → 插件的方法名。
pub mod core_method {
    /// 通用存活探测。
    pub const PING: &str = "core.ping";
    /// 获取核心版本与能力声明。
    pub const INFO: &str = "core.info";
    /// 插件写日志（受 `ui.notify` 权限约束转发到前端）。
    pub const LOG: &str = "core.log";
    /// 推送上下文（游戏信息 / 房间信息 / 其他插件信息）。
    pub const CONTEXT_UPDATE: &str = "core.context.update";
    /// 请求插件优雅退出。
    pub const SHUTDOWN: &str = "core.shutdown";
}

/// 适配类插件方法名。
pub mod adapter_method {
    /// 初始化适配器，返回自身网络能力描述。
    pub const INIT: &str = "adapter.init";
    /// 以房主身份创建联机端点。
    pub const HOST_START: &str = "adapter.host.start";
    /// 停止房主端点。
    pub const HOST_STOP: &str = "adapter.host.stop";
    /// 以访客身份加入房间，返回本机可用入口地址。
    pub const JOIN: &str = "adapter.join";
    /// 离开房间。
    pub const LEAVE: &str = "adapter.leave";
    /// 查询当前状态。
    pub const STATUS: &str = "adapter.status";
    /// 探测与对端/中继的可达性。
    pub const PROBE: &str = "adapter.probe";
    /// 通知适配器其运行环境已完成安装/更新（由核心在下载落地后调用）。
    pub const INSTALL: &str = "adapter.install";
}

/// 检测类插件方法名。
pub mod detector_method {
    /// 一次性扫描本机游戏实例。
    pub const SCAN: &str = "detector.scan";
    /// 开始持续监听游戏实例变化。
    pub const WATCH_START: &str = "detector.watch.start";
    /// 停止监听。
    pub const WATCH_STOP: &str = "detector.watch.stop";
}

/// 耦合类插件方法名。
pub mod coupler_method {
    /// 让游戏接入指定的本机入口地址。
    pub const ATTACH: &str = "coupler.attach";
    /// 解除接入。
    pub const DETACH: &str = "coupler.detach";
    /// 拉起游戏进程/客户端。
    pub const LAUNCH: &str = "coupler.launch";
    /// 查询状态。
    pub const STATUS: &str = "coupler.status";
}

/// 插件 → 核心的事件主题。
pub mod event_topic {
    /// 适配器状态变化。
    pub const ADAPTER_STATE: &str = "adapter.state";
    /// 检测到新的游戏实例。
    pub const DETECTOR_FOUND: &str = "detector.found";
    /// 游戏实例消失。
    pub const DETECTOR_LOST: &str = "detector.lost";
    /// 耦合状态变化。
    pub const COUPLER_STATE: &str = "coupler.state";
    /// 插件自定义日志。
    pub const LOG: &str = "plugin.log";
}

/// 错误码。用于让核心在不解析文案的前提下判断失败语义。
pub mod error_code {
    pub const BAD_REQUEST: &str = "bad_request";
    pub const UNAUTHORIZED: &str = "unauthorized";
    pub const FORBIDDEN: &str = "forbidden";
    pub const NOT_SUPPORTED: &str = "not_supported";
    pub const TIMEOUT: &str = "timeout";
    pub const RATE_LIMITED: &str = "rate_limited";
    pub const INTERNAL: &str = "internal";
    pub const UNAVAILABLE: &str = "unavailable";
}

/// 握手报文（明文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Handshake {
    /// 核心 → 插件：发起握手。
    Hello {
        /// 协议版本。
        v: u16,
        /// 核心实例标识（便于插件区分多开/测试环境）。
        server: String,
        /// 本次连接的唯一会话 ID。
        session: String,
        /// 核心随机数（hex）。
        nonce_s: String,
        /// 允许的认证方式，当前仅 `psk_hmac`。
        methods: Vec<String>,
        /// 要求插件在 `hello_ack` 中回填的插件 ID。
        expect_plugin: String,
    },
    /// 插件 → 核心：声明身份并给出证明。
    HelloAck {
        /// 插件 ID，必须与清单一致。
        plugin_id: String,
        /// 插件版本。
        plugin_version: String,
        /// 插件随机数（hex）。
        nonce_p: String,
        /// HMAC-SHA256(psk, transcript) 的 hex 值。
        proof: String,
    },
    /// 核心 → 插件：认证通过，开始加密会话。
    AuthOk {
        /// 核心侧的证明（插件可验证对端确实持有 PSK）。
        proof: String,
        /// 心跳间隔（毫秒）。
        heartbeat_ms: u64,
    },
    /// 任一方 → 对方：握手失败。
    Denied { code: String, message: String },
}

impl Handshake {
    /// 构造握手转录串。双方必须使用完全一致的拼接规则。
    pub fn transcript(
        session: &str,
        plugin_id: &str,
        nonce_s: &str,
        nonce_p: &str,
        role: &str,
    ) -> String {
        format!("mclink-plugin-auth-v1|{session}|{plugin_id}|{nonce_s}|{nonce_p}|{role}")
    }
}

/// 加密信封。所有会话期报文都以此结构承载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureFrame {
    /// 协议版本。
    pub v: u8,
    /// 会话 ID，防止跨会话串帧。
    pub sid: String,
    /// 单调递增序号，用于防重放。
    pub seq: u64,
    /// AES-GCM 随机数（base64）。
    pub n: String,
    /// 密文（base64）。
    pub ct: String,
}

impl SecureFrame {
    /// 构造 AAD：绑定版本、会话与序号，任一被篡改都会解密失败。
    pub fn aad(&self) -> Vec<u8> {
        format!("v{}|{}|{}", self.v, self.sid, self.seq).into_bytes()
    }
}

/// 解密后的会话消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Message {
    /// 请求（需要对方回 response）。
    Request {
        id: String,
        method: String,
        #[serde(default)]
        params: Value,
    },
    /// 响应。
    Response {
        id: String,
        ok: bool,
        #[serde(default)]
        result: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ErrorInfo>,
    },
    /// 单向事件通知。
    Event {
        topic: String,
        #[serde(default)]
        data: Value,
    },
    /// 心跳。
    Ping { at: u64 },
    /// 心跳回执。
    Pong { at: u64 },
    /// 主动断开。
    Bye { reason: String },
}

/// 结构化错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl ErrorInfo {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(error_code::FORBIDDEN, message)
    }

    pub fn not_supported(message: impl Into<String>) -> Self {
        Self::new(error_code::NOT_SUPPORTED, message)
    }
}

// ---------------------------------------------------------------------------
// 负载类型：核心下发给插件的上下文
// ---------------------------------------------------------------------------

/// 游戏信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameInfo {
    /// 游戏唯一 ID，如 `minecraft-java`。
    pub id: String,
    /// 展示名。
    #[serde(default)]
    pub name: String,
    /// 游戏进程可执行文件名（用于检出）。
    #[serde(default)]
    pub process: Option<String>,
    /// 游戏监听端口。
    #[serde(default)]
    pub port: Option<u16>,
    /// 传输层协议：`tcp` / `udp`。
    #[serde(default)]
    pub transport: Option<String>,
    /// 是否支持局域网广播发现。
    #[serde(default)]
    pub lan_broadcast: Option<bool>,
    /// 版本号等附加信息。
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

/// 本地游戏发现结果（核心编排扫描后回传前端首页展示）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalGameFound {
    /// 游戏进程可执行文件名。
    pub process: String,
    /// 游戏展示名。
    pub game_name: String,
    /// 命中的扫描器（detector）插件展示名。
    pub scanner: String,
    /// 推荐的适配器（adapter）插件展示名。
    pub adapter: String,
}

/// 房间信息。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoomInfo {
    /// 房间码。
    pub code: String,
    /// 房主标识（玩家名或设备 ID）。
    #[serde(default)]
    pub host: String,
    /// 期望的对端数量。
    #[serde(default)]
    pub capacity: Option<u32>,
    /// 房间口令是否受保护。
    #[serde(default)]
    pub password_protected: bool,
}

/// 同一主机上其他已就绪插件的能力摘要，用于插件间协作。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerPluginInfo {
    pub plugin_id: String,
    /// 插件种类：`adapter` / `detector` / `coupler`。
    pub kind: String,
    /// 该插件覆盖的游戏 ID 列表，`*` 表示不限。
    pub games: Vec<String>,
    /// 插件可用的本机入口（适配器通常提供转发端口）。
    #[serde(default)]
    pub endpoints: Vec<String>,
}

/// 上下文快照：`core.context.update` 事件的负载。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginContext {
    /// 本轮联机扮演的角色：`host` / `guest` / `idle`。
    pub role: String,
    #[serde(default)]
    pub game: Option<GameInfo>,
    #[serde(default)]
    pub room: Option<RoomInfo>,
    #[serde(default)]
    pub peers: Vec<PeerPluginInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_frame_aad_binds_seq() {
        let f = SecureFrame {
            v: 1,
            sid: "s".into(),
            seq: 3,
            n: String::new(),
            ct: String::new(),
        };
        assert_eq!(f.aad(), b"v1|s|3".to_vec());
    }

    #[test]
    fn handshake_transcript_is_stable() {
        let a = Handshake::transcript("s1", "dev.x", "aa", "bb", "plugin");
        let b = Handshake::transcript("s1", "dev.x", "aa", "bb", "plugin");
        assert_eq!(a, b);
        assert_ne!(a, Handshake::transcript("s1", "dev.x", "aa", "bb", "core"));
    }

    #[test]
    fn message_roundtrip() {
        let msg = Message::Request {
            id: "1".into(),
            method: adapter_method::INIT.into(),
            params: serde_json::json!({ "game": "minecraft-java" }),
        };
        let text = serde_json::to_string(&msg).unwrap();
        let back: Message = serde_json::from_str(&text).unwrap();
        matches!(back, Message::Request { .. });
    }
}
