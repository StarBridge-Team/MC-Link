# `mclink-plugin-sdk`（Rust）

MC Link 插件的 Rust SDK。把控制面协议（握手、HKDF、AES-GCM 信封、序号、心跳、
方法分发、插件间通信）封装成 [`Plugin`]，插件只写业务逻辑。

适用于 `runtime.kind = "process"` 的原生轨道：SDK 负责与核心说话，你的业务代码
自己开 socket / 起进程。

```rust
use serde_json::json;
use mclink_plugin_sdk::Plugin;

#[tokio::main]
async fn main() -> Result<(), mclink_plugin_sdk::Error> {
    let mut plugin = Plugin::connect_from_args().await?;

    plugin.handle("adapter.init", |_params, _ctx| async {
        Ok(json!({ "ready": true, "host_fields": [], "join_fields": [] }))
    });

    plugin.handle("adapter.status", |_params, _ctx| async {
        Ok(json!({ "phase": "waiting" }))
    });

    // 插件间通信
    plugin.on_peer("room.get", |_params, ctx| async move {
        Ok(json!({ "role": ctx.context().role }))
    });

    plugin.run().await
}
```

```toml
[dependencies]
mclink-plugin-sdk = { path = "../sdk/rust" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

## 能力

| API | 作用 |
|---|---|
| `Plugin::connect_from_args()` / `Plugin::connect(path)` | 建连并完成握手 |
| `Plugin::handle(method, f)` | 实现能力方法（`adapter.*` / `detector.scan` …） |
| `Plugin::on_peer(method, f)` | 实现插件间方法 |
| `Plugin::on_context(f)` / `on_event(topic, f)` / `on_peer_event(f)` | 订阅推送 |
| `Plugin::emit` / `log` | 发事件 / 结构化日志 |
| `Context::peer_list` / `peer_call` / `peer_publish` | 插件间通信 |
| `Plugin::run()` | 主循环，直到断开 |

处理器签名统一是 `Fn(Value, Context) -> Future<Output = Result<Value, Error>>`；
`Context` 带 `plugin_id()`、`from()`、`depth()`、`context()`，`peer_call` 默认沿用
当前调用的深度（三层转发不会误触嵌套上限）。

## 测试

```bash
cd sdk/rust && cargo test
```

两层验证：

1. **固定向量**（`src/wire.rs`）——HMAC / HKDF 的期望值由 `node:crypto` 这个独立
   实现生成，两边必须逐字节一致；
2. **假核心端到端**（`src/e2e.rs`）——起一个本地 WS 服务扮演核心，验证握手顺序、
   信封、分发、`peer.call` 与 `pong`。

跨实现验证（真实核心 + 真实进程）在 `src-tauri/src/plugin/node_e2e.rs`。

## 协议与安全

- 完整协议见 [`docs/plugin-developer-guide.md`](../../docs/plugin-developer-guide.md)。
- SDK 会校验核心的 `auth_ok.proof`（防假核心），不要绕过。
- 权限是**调度级**的：SDK 不会、也无法替核心阻止你开 socket。
