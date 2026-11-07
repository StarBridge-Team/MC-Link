# `@mclink/plugin-sdk`（Node / Bun）

MC Link 插件的 JavaScript SDK：**零依赖**，Node 22+ 与 Bun 都能直接跑。

它把控制面协议（握手、密钥派生、AES-GCM 信封、序号、心跳、方法分发、插件间通信）
全部封装掉，插件只写业务逻辑。

```js
import { Plugin } from "@mclink/plugin-sdk";

const plugin = await Plugin.connectFromArgs();

plugin.handle("adapter.init", async () => ({
  ready: true,
  host_fields: [{ key: "port", label: "游戏端口", type: "text", required: true }],
  join_fields: [],
}));

plugin.handle("adapter.status", async () => ({ phase: "waiting" }));

// 插件间通信
plugin.onPeer("room.get", async (_params, ctx) => ({ role: ctx.context.role }));
plugin.onPeerEvent(async ({ from, topic }) => {
  await plugin.log("debug", `${from}: ${topic}`);
});

await plugin.run();
```

## 能力

| API | 作用 |
|---|---|
| `Plugin.connectFromArgs()` / `Plugin.connect({ sessionPath })` | 建连并完成握手 |
| `plugin.handle(method, fn)` | 实现能力方法（`adapter.*` / `detector.scan` …） |
| `plugin.onPeer(method, fn)` | 实现插件间方法 |
| `plugin.onContext(fn)` / `plugin.onEvent(topic, fn)` / `plugin.onPeerEvent(fn)` | 订阅推送 |
| `plugin.emit(topic, data)` / `plugin.log(level, msg)` | 发事件 / 结构化日志 |
| `plugin.peerList()` / `peerCall()` / `peerPublish()` | 插件间通信 |
| `plugin.run()` / `plugin.close()` | 主循环 / 主动退出 |

处理器签名统一是 `(params, ctx)`；`ctx` 带 `pluginId`、`from`、`depth`、`context`，
并提供同一套 `emit` / `log` / `peer*` 方法。**`peerCall` 会自动带上当前调用的深度**，
所以三层转发也不会触发"嵌套过深"的误判。

## 作为插件依赖

插件用 `bun build --target=bun --outfile dist/main.js src/main.js` 打包时，SDK 会被
内联进单文件产物——这正是核心要求的分发形态（插件目录里不要出现 `node_modules`）。

开发调试时可以直接相对路径引用本目录（见 [`examples/echo-adapter`](./examples/echo-adapter)）。

## 测试

```bash
node --test sdk/node/test/crypto.test.js
```

用 `node:crypto` 作为**独立参考实现**校验 HMAC / HKDF / AES-GCM，避免"自洽但错误"。
端到端（真实核心网关）由 `src-tauri/src/plugin/node_e2e.rs` 覆盖：它会用 Bun 拉起
`examples/echo-adapter` 并跑完整握手。

## 协议与安全

- 完整协议见 [`docs/plugin-developer-guide.md`](../../docs/plugin-developer-guide.md)。
- 不校验核心证明就等于允许假核心窃取 PSK——SDK 默认校验，不要绕过。
- 权限是**调度级**的：SDK 不会、也无法替核心阻止你开 socket。少声明权限是为了让
  用户看懂你要做什么。
