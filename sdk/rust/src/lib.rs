//! MC Link 插件 SDK（Rust 轨道）。
//!
//! 把控制面协议包装成 [`Plugin`]：握手、密钥派生、信封、序号、心跳与分发全部
//! 由 SDK 处理，插件只写"收到什么方法、回什么值"。
//!
//! ```no_run
//! use serde_json::json;
//! use mclink_plugin_sdk::Plugin;
//!
//! # async fn demo() -> Result<(), mclink_plugin_sdk::Error> {
//! let mut plugin = Plugin::connect_from_args().await?;
//!
//! plugin.handle("adapter.init", |_params, _ctx| async {
//!     Ok(json!({ "ready": true, "host_fields": [], "join_fields": [] }))
//! });
//!
//! plugin.handle("adapter.status", |_params, _ctx| async {
//!     Ok(json!({ "phase": "waiting" }))
//! });
//!
//! // 插件间通信：别的插件 `core.peer.call { method: "room.get" }` 时命中
//! plugin.on_peer("room.get", |_params, ctx| async move {
//!     Ok(json!({ "role": ctx.context().role }))
//! });
//!
//! plugin.run().await
//! # }
//! ```
//!
//! # 与核心的对应关系
//!
//! | 这里 | 核心 |
//! |---|---|
//! | [`Plugin::handle`] | `adapter.*` / `detector.*` / `coupler.*` 等能力方法 |
//! | [`Plugin::on_peer`] | `peer.call` 的载荷方法名（命名空间隔离，见 `plugin::peer`） |
//! | [`Plugin::on_context`] | `core.context.update` 单向推送 |
//! | [`Context::emit`] | 插件 → 核心的 `event`（转发到界面） |
//! | [`Context::peer_call`] | `core.peer.call`（由核心中转） |
//!
//! 协议细节见 `docs/plugin-developer-guide.md`。

mod context;
mod session;
mod wire;

#[cfg(test)]
mod e2e;

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use wire::{Handshake, Message, SessionDescriptor, PROTOCOL_VERSION, WS_SUBPROTOCOL};

use session::{manifest_version, read_handshake, send_handshake};

/// 握手等待上限。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// peer 调用默认超时（与核心默认值一致）。
pub const DEFAULT_PEER_TIMEOUT_MS: u64 = 10_000;

/// 插件侧错误：`code` 会被原样放进 `response.error.code`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new("bad_request", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new("forbidden", message)
    }

    pub fn not_supported(message: impl Into<String>) -> Self {
        Self::new("not_supported", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new("unavailable", message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new("timeout", message)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}（{}）", self.message, self.code)
    }
}

impl std::error::Error for Error {}

impl From<Error> for wire::ErrorInfo {
    fn from(value: Error) -> Self {
        wire::ErrorInfo {
            code: value.code,
            message: value.message,
        }
    }
}

/// 核心下发的上下文（`core.context.update` 的载荷）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginContext {
    /// `host` / `guest` / `idle`。
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub game: Option<Value>,
    #[serde(default)]
    pub room: Option<Value>,
    #[serde(default)]
    pub members: Vec<Value>,
    #[serde(default)]
    pub peers: Vec<PeerInfo>,
}

/// 同机其它插件的摘要（`core.peer.list` 的元素 / 上下文里的 `peers`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerInfo {
    pub plugin_id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub games: Vec<String>,
    #[serde(default)]
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub alive: bool,
}

/// 别的插件发来的广播（`peer.event`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerEvent {
    pub from: String,
    pub topic: String,
    #[serde(default)]
    pub data: Value,
}

/// peer 调用选项。
#[derive(Debug, Clone)]
pub struct PeerCallOptions {
    /// 目标未连接时是否请求核心拉起。
    pub launch: bool,
    /// 超时（毫秒），核心夹取 100–30000。
    pub timeout_ms: u64,
    /// 嵌套深度；一般不用手填，默认沿用当前调用的深度。
    pub depth: Option<u32>,
}

impl Default for PeerCallOptions {
    fn default() -> Self {
        Self {
            launch: false,
            timeout_ms: DEFAULT_PEER_TIMEOUT_MS,
            depth: None,
        }
    }
}

pub(crate) type HandlerResult = Result<Value, Error>;
pub(crate) type Handler = Arc<dyn Fn(Value, Context) -> BoxFuture<'static, HandlerResult> + Send + Sync>;

/// 处理器拿到的上下文（每次调用一份，`depth`/`from` 不会串台）。
#[derive(Clone)]
pub struct Context {
    plugin_id: Arc<str>,
    depth: u32,
    from: Option<String>,
    context: Arc<RwLock<PluginContext>>,
    out: mpsc::UnboundedSender<Message>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<HandlerResult>>>>,
    rpc: Arc<AtomicU64>,
}


#[derive(Default)]
pub(crate) struct Registry {
    handlers: HashMap<String, Handler>,
    peer_handlers: HashMap<String, Handler>,
    context_handlers: Vec<Arc<dyn Fn(&PluginContext) + Send + Sync>>,
    event_handlers: HashMap<String, Vec<Arc<dyn Fn(Value) + Send + Sync>>>,
    peer_event_handlers: Vec<Arc<dyn Fn(PeerEvent) + Send + Sync>>,
}

/// 一枚插件。
pub struct Plugin {
    plugin_id: String,
    sid: String,
    keys: wire::Keys,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    out: mpsc::UnboundedSender<Message>,
    out_rx: Option<mpsc::UnboundedReceiver<Message>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<HandlerResult>>>>,
    registry: Registry,
    context: Arc<RwLock<PluginContext>>,
    rpc: Arc<AtomicU64>,
}

impl Plugin {
    /// 从 `--mclink-session <file>` 建连（核心拉起插件时就是这么传的）。
    pub async fn connect_from_args() -> Result<Self, Error> {
        let args: Vec<String> = std::env::args().collect();
        let path = args
            .iter()
            .position(|a| a == "--mclink-session")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .or_else(|| {
                args.iter()
                    .rev()
                    .find(|a| a.ends_with(".session.json"))
                    .cloned()
            })
            .ok_or_else(|| Error::bad_request("缺少 --mclink-session 参数（应由核心拉起）"))?;
        Self::connect(path).await
    }

    /// 按会话文件建连并完成握手。
    pub async fn connect(session_path: impl AsRef<Path>) -> Result<Self, Error> {
        let session = SessionDescriptor::load(session_path.as_ref())?;
        let psk = session.psk_bytes()?;

        let mut request = session
            .websocket_url()
            .into_client_request()
            .map_err(|e| Error::bad_request(format!("连接地址非法: {e}")))?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_static(WS_SUBPROTOCOL),
        );
        let (mut ws, _) = connect_async(request)
            .await
            .map_err(|e| Error::unavailable(format!("连接核心网关失败: {e}")))?;

        let hello = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_handshake(&mut ws))
            .await
            .map_err(|_| Error::timeout("等待 hello 超时"))??;

        let (sid, nonce_s) = match hello {
            Handshake::Hello {
                v,
                session: session_id,
                nonce_s,
                expect_plugin,
                ..
            } => {
                if v != PROTOCOL_VERSION {
                    return Err(Error::new(
                        "unauthorized",
                        format!("协议版本不匹配：核心 {v}"),
                    ));
                }
                if expect_plugin != session.plugin_id {
                    return Err(Error::new("unauthorized", "核心期望的插件 ID 与会话文件不一致"));
                }
                (session_id, nonce_s)
            }
            other => {
                return Err(Error::new(
                    "unauthorized",
                    format!("握手期望 hello，收到 {other:?}"),
                ))
            }
        };

        let nonce_p = hex::encode({
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut bytes);
            bytes
        });
        let proof = wire::hmac_hex(
            &psk,
            wire::transcript(&sid, &session.plugin_id, &nonce_s, &nonce_p, "plugin").as_bytes(),
        );
        send_handshake(
            &mut ws,
            &Handshake::HelloAck {
                plugin_id: session.plugin_id.clone(),
                plugin_version: manifest_version().unwrap_or_else(|| "0.0.0".to_string()),
                nonce_p: nonce_p.clone(),
                proof,
            },
        )
        .await?;

        let auth = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_handshake(&mut ws))
            .await
            .map_err(|_| Error::timeout("等待 auth_ok 超时"))??;
        match auth {
            Handshake::AuthOk { proof, .. } => {
                let expected = wire::hmac_hex(
                    &psk,
                    wire::transcript(&sid, &session.plugin_id, &nonce_s, &nonce_p, "core")
                        .as_bytes(),
                );
                if !wire::ct_eq_hex(&expected, &proof) {
                    return Err(Error::new(
                        "unauthorized",
                        "核心证明无效（可能连到了假核心）",
                    ));
                }
            }
            Handshake::Denied { code, message } => {
                return Err(Error::new(code, message));
            }
            other => {
                return Err(Error::new(
                    "unauthorized",
                    format!("握手期望 auth_ok，收到 {other:?}"),
                ))
            }
        }

        let keys = wire::Keys::derive(&psk, &nonce_s, &nonce_p, &sid, &session.plugin_id)?;
        let (out, out_rx) = mpsc::unbounded_channel();

        Ok(Self {
            plugin_id: session.plugin_id,
            sid,
            keys,
            ws,
            out,
            out_rx: Some(out_rx),
            pending: Arc::new(Mutex::new(HashMap::new())),
            registry: Registry::default(),
            context: Arc::new(RwLock::new(PluginContext::default())),
            rpc: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// 当前上下文快照。
    pub fn context(&self) -> PluginContext {
        self.context
            .try_read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// 注册一个能力方法。
    pub fn handle<F, Fut>(&mut self, method: impl Into<String>, handler: F) -> &mut Self
    where
        F: Fn(Value, Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HandlerResult> + Send + 'static,
    {
        let boxed: Handler = Arc::new(move |params, ctx| Box::pin(handler(params, ctx)));
        self.registry.handlers.insert(method.into(), boxed);
        self
    }

    /// 注册一个插件间方法（`peer.call` 的方法名命中）。
    pub fn on_peer<F, Fut>(&mut self, method: impl Into<String>, handler: F) -> &mut Self
    where
        F: Fn(Value, Context) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HandlerResult> + Send + 'static,
    {
        let boxed: Handler = Arc::new(move |params, ctx| Box::pin(handler(params, ctx)));
        self.registry.peer_handlers.insert(method.into(), boxed);
        self
    }

    /// 订阅 `core.context.update`。
    pub fn on_context<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&PluginContext) + Send + Sync + 'static,
    {
        self.registry.context_handlers.push(Arc::new(handler));
        self
    }

    /// 按主题订阅核心推来的事件。
    pub fn on_event<F>(&mut self, topic: impl Into<String>, handler: F) -> &mut Self
    where
        F: Fn(Value) + Send + Sync + 'static,
    {
        self.registry
            .event_handlers
            .entry(topic.into())
            .or_default()
            .push(Arc::new(handler));
        self
    }

    /// 订阅别的插件的 `peer.event`。
    pub fn on_peer_event<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(PeerEvent) + Send + Sync + 'static,
    {
        self.registry.peer_event_handlers.push(Arc::new(handler));
        self
    }

    /// 发一个单向事件（`run` 之前调用会先排队）。
    pub fn emit(&self, topic: &str, data: Value) -> Result<(), Error> {
        self.out
            .send(Message::Event {
                topic: topic.to_string(),
                data,
            })
            .map_err(|_| Error::unavailable("插件已断开"))
    }

    /// 结构化日志。
    pub fn log(&self, level: &str, message: &str) -> Result<(), Error> {
        self.emit(
            "plugin.log",
            json!({ "level": level, "message": message, "plugin": self.plugin_id }),
        )
    }
}
