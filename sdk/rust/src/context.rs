//! 处理器上下文：只读状态 + 主动发送（emit / log / peer）。
//!
//! 拆出 `impl Context` 是为了让 `lib.rs` 只放"类型与连接"，分发与收尾放在
//! `session.rs`——三个文件都在 500 行以内，改动时更容易定位。

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::wire::Message;
use crate::{Context, Error, PeerCallOptions, PeerInfo, PluginContext};
impl Context {
    /// 自己的插件 ID。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// 本次调用来自哪枚插件（顶层调用为 `None`）。
    pub fn from(&self) -> Option<&str> {
        self.from.as_deref()
    }

    /// 本次调用所处的 peer 嵌套深度。
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// 插件可写的数据目录（核心在拉起时创建）。
    ///
    /// **不要**把运行期状态写进插件安装目录：那个目录被整包验签覆盖，
    /// 多一个文件就会让核心下次扫描时忽略整个插件。
    pub fn data_dir(&self) -> Option<&str> {
        self.data_dir.as_deref()
    }

    /// 最近一次上下文快照。
    pub fn context(&self) -> PluginContext {
        self.context
            .try_read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// 发一个单向事件。
    pub fn emit(&self, topic: &str, data: Value) -> Result<(), Error> {
        self.out
            .send(Message::Event {
                topic: topic.to_string(),
                data,
            })
            .map_err(|_| Error::unavailable("插件已断开"))
    }

    /// 结构化日志（`plugin.log` 主题）。
    pub fn log(&self, level: &str, message: &str) -> Result<(), Error> {
        self.emit(
            "plugin.log",
            json!({ "level": level, "message": message, "plugin": &*self.plugin_id }),
        )
    }

    /// 列出可调用的同机插件。
    pub async fn peer_list(&self) -> Result<Vec<PeerInfo>, Error> {
        let value = self
            .call_core("core.peer.list", Value::Null, Duration::from_secs(10))
            .await?;
        serde_json::from_value(value)
            .map_err(|e| Error::internal(format!("core.peer.list 返回格式异常: {e}")))
    }

    /// 调用另一枚插件。
    pub async fn peer_call(
        &self,
        to: &str,
        method: &str,
        params: Value,
        options: PeerCallOptions,
    ) -> Result<Value, Error> {
        let payload = json!({
            "to": to,
            "method": method,
            "params": params,
            "depth": options.depth.unwrap_or(self.depth),
            "launch": options.launch,
            "timeout_ms": options.timeout_ms,
        });
        let timeout = Duration::from_millis(options.timeout_ms.saturating_add(500));
        self.call_core("core.peer.call", payload, timeout).await
    }

    /// 广播（或定向投递）一条事件，不等待回执。
    pub async fn peer_publish(&self, topic: &str, data: Value, to: Option<&str>) -> Result<(), Error> {
        let mut payload = json!({ "topic": topic, "data": data });
        if let Some(target) = to {
            payload["to"] = json!(target);
        }
        self.call_core("core.peer.publish", payload, Duration::from_secs(10))
            .await
            .map(|_| ())
    }

    async fn call_core(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, Error> {
        let id = format!("r{}", self.rpc.fetch_add(1, Ordering::SeqCst));
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        if self
            .out
            .send(Message::Request {
                id: id.clone(),
                method: method.to_string(),
                params,
            })
            .is_err()
        {
            self.pending.lock().await.remove(&id);
            return Err(Error::unavailable("插件已断开"));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::unavailable("核心响应通道已关闭")),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(Error::timeout(format!("等待核心响应超时: {method}")))
            }
        }
    }
}
