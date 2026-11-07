# MC Link 插件开发者指南

> 面向**第三方插件作者**。描述"今天真实可用的协议与契约"，不含未实现的规划。
> 核心侧架构见 [`插件系统设计.md`](../插件系统设计.md)；作者侧工程约束（可执行文件、
> 体积预算）见 [`插件开发规范.md`](../插件开发规范.md)。
>
> 版本：协议 v1（2026-11）。

---

## 0. 30 秒理解模型

```text
MC Link 核心 ──(拉起子进程)──▶ 你的插件（独立进程，任意语言）
      ▲                              │
      └──────── 回环 WebSocket ───────┘
              控制面（请求/响应/事件），AES-256-GCM 加密
```

三个必须记住的结论：

1. **插件是独立进程**，由核心拉起（`runtime.kind = "process"`），通过回环 WebSocket
   与核心通信。核心不加载你的代码、不解析你的协议。
2. **控制面只传编排指令**，不传游戏流量。游戏数据由你自己的 socket 转发。
3. **核心只信任清单 + 权限**，不信任插件自述。你声明的 `permissions` 决定核心会
   为你做哪些事。

> 内置插件（`runtime.kind = "builtin"`）不走这条链路，它们是核心进程内的实现，
> 与本文无关。

---

## 1. 插件目录与清单

```text
<data_dir>/Plugins/<plugin_id>/
├── plugin.json          # 清单（必需，UTF-8 无 BOM）
├── integrity.txt        # 可选：文件哈希清单（签名用）
├── integrity.sig        # 可选：对 integrity.txt 的 minisign 签名
└── bin/your-plugin.exe  # 你的可执行文件（入口，必须在插件目录内）
```

`<data_dir>` 是 MC Link 的数据目录（便携版在 exe 同目录，否则是系统应用数据目录）。

### 1.1 清单字段

```jsonc
{
  "id": "dev.example.my-adapter",     // 必填，反向域名，小写字母/数字/./-/_，至少两段
  "name": "示例适配器",                // 必填，展示名
  "version": "1.0.0",                 // 必填，≤32 字符
  "kind": "adapter",                  // 必填：adapter|detector|coupler|bridge|widget|utility
  "protocol_version": 1,              // 必须等于 1，不匹配直接拒绝加载
  "author": "you",                    // 可选
  "description": "一句话说明",         // 可选
  "games": ["minecraft-java"],        // 可选，默认 ["*"]；* 表示与具体游戏无关
  "platforms": ["windows"],           // 可选：windows|linux|macos；未知值被丢弃并告警
  "methods": ["p2p", "relay"],        // 可选：bundled|p2p|relay|port-mapping
  "tags": ["低延迟"],                  // 可选，≤12 个、每个 ≤24 字符
  "permissions": ["net_connect_any"], // 你要的能力，见第 7 节
  "priority": 0,                      // 可选，外部插件被夹到 ±50（只能同档微调）
  "fallback": [],                     // 可选，同种类回退链（合法的插件 ID）
  "runtime": {
    "kind": "process",                // process（原生可执行文件）| node（Bun 脚本）
    "entry": "bin/your-plugin.exe",   // 必填，相对路径，不得越出插件目录
    "args": [],                       // 可选，支持 {endpoint} / {token_file} 占位符
    "workdir": ".",                   // 可选，相对路径
    "env": {}                         // 可选，附加环境变量
  },
  "limits": {                         // 可选，全部有默认值（见第 8 节）
    "max_rpc_per_sec": 32,
    "max_frame_bytes": 1048576,
    "heartbeat_ms": 5000,
    "idle_timeout_ms": 20000,
    "rpc_timeout_ms": 30000,
    "max_auth_failures": 5
  }
}
```

**路径安全**：`runtime.entry` / `runtime.workdir` 会被逐字符校验——拒绝绝对路径、
盘符（`C:`）、UNC、`..`、反斜杠。合法相对路径只用 `/`。

**写错不致命**：`platforms` / `methods` 里的未知取值只被丢弃并留痕，不会让插件加载失败；
但 `id` / `version` / `protocol_version` / `runtime.entry` 非法会让清单校验失败、插件被跳过。

### 1.2 信任与启用（务必先读）

- 插件目录会被**每次扫描时验签**：有 `integrity.txt` + 有效 `integrity.sig` → `Verified`；
  没有 → `Unsigned`；有清单但校验不过 / 夹带清单外文件 → **直接忽略该目录**。
- **未签名的插件默认是停用的**，必须由用户在插件管理里显式启用；启用后也只能拿到
  最小权限集（第 7 节）。
- 想让插件开箱即用（并能拿到 `net_listen_public` / `proc_spawn_game` / `ui_notify` 等），
  需要发布者用官方私钥签出 `integrity.txt`。签名工具尚未随仓库发布，当前请按
  "未签名插件"开发与调试。

### 1.3 你的进程会被怎样启动

核心执行：

```text
# 原生轨道
<plugins>/<id>/bin/your-plugin.exe --mclink-session <data_dir>/Plugins/runtime/<id>.session.json

# Node 轨道（解释器由核心提供，清单无权指定）
<data_dir>/Runtime/bun.exe <plugins>/<id>/dist/main.js --mclink-session <…>.session.json
```

- 工作目录 = 清单 `workdir`（默认插件目录）。
- **环境变量被清空后重新注入**：只保留 `PATH`、`SYSTEMROOT`、`WINDIR`、`TEMP`、`TMP`、
  `LANG`、`HOME`，加上清单 `runtime.env`。不要依赖其它环境变量。
- Windows 上以 `CREATE_NO_WINDOW` 启动，不需要写 `#![windows_subsystem = "windows"]`。
- 若清单 `args` 里已含 `{token_file}`，核心不再自动追加 `--mclink-session`。

### 1.4 两条轨道怎么选

| | `runtime.kind = "process"` | `runtime.kind = "node"` |
|---|---|---|
| 交付物 | 原生可执行文件（Rust / Go / Zig / C++） | `.js` / `.mjs` / `.cjs` 脚本 |
| 入口 | `bin/x.exe` | `dist/main.js`（**不接受 `.ts`**） |
| 运行时 | 无（只依赖系统组件） | 核心提供的共享 Bun，**版本由核心锁定** |
| 体积 | 通常 1–5 MB | 几十 KB ~ 1 MB |
| 适合 | 打洞、协议实现等需要原生 socket 的场景 | 业务编排、数据处理、快速迭代 |

Node 轨道的可执行文件是**核心安装的**，不在你的插件目录里：

```text
<data_dir>/Runtime/bun.exe        # 由核心下载并 SHA256 校验
<data_dir>/Runtime/bun.json       # 版本记录
```

用户机器上没有运行时时，核心会明确报错（"共享 Node 运行时未安装"），界面据此提示
安装。**不要**在插件里自带 Node/Bun，也不要要求用户装——那会让"插件体积"和
"运行时矩阵"同时失控。

开发期可以用环境变量 `MC_LINK_BUN` 指向你本机的 bun（核心会打日志提示；发行版
行为不变）。

---

## 2. 建立连接

### 2.1 读取会话文件

```jsonc
{
  "protocol": 1,
  "plugin_id": "dev.example.my-adapter",
  "endpoint": "ws://127.0.0.1:49152/plugin",  // 不含 query
  "token": "<一次性启动令牌，64 hex>",
  "psk": "<预共享密钥，64 hex>",                // 握手用
  "heartbeat_ms": 5000
}
```

连接地址 = `endpoint + "?plugin=" + plugin_id + "&token=" + token`，
并带子协议头 `Sec-WebSocket-Protocol: mclink-plugin`。

> 令牌与 PSK 都在这个文件里、不在命令行里——进程列表可被同机其它进程读取。

### 2.2 握手（明文 JSON，一次性）

核心**先发**：

```json
{"t":"hello","v":1,"server":"mc-link/0.5.4","session":"<sid>",
 "nonce_s":"<32字节hex>","methods":["psk_hmac"],"expect_plugin":"dev.example.my-adapter"}
```

你回（`nonce_p` 自选 32 字节随机，hex）：

```json
{"t":"hello_ack","plugin_id":"dev.example.my-adapter","plugin_version":"1.0.0",
 "nonce_p":"<32字节hex>","proof":"<HMAC hex>"}
```

计算方式：

```text
transcript = "mclink-plugin-auth-v1|{session}|{plugin_id}|{nonce_s}|{nonce_p}|plugin"
proof      = HMAC-SHA256(key = PSK 的 32 字节原始值, msg = transcript 的 UTF-8 字节)
             → 小写 hex
```

> PSK 在会话文件里是 **hex 字符串**，要先 `hex::decode`/`Buffer.from(psk,'hex')` 再当
> HMAC 的 key。直接用 hex 文本当 key 会算出不同的 proof。

核心校验通过后回：

```json
{"t":"auth_ok","proof":"<核心侧 proof>","heartbeat_ms":5000}
```

核心侧 proof 用 `role = "core"`，同样的拼法。**你必须校验它**——否则连到假核心不报错。

失败时核心发 `{"t":"denied","code":"unauthorized","message":"..."}` 并断开。
握手总超时 15 秒（可在设置里调，范围 5–60 秒），单帧上限 16 KiB。

### 2.3 派生会话密钥

```text
salt = nonce_s || nonce_p            (两个 hex 字符串直接拼接后的 ASCII 字节)
transcript = "mclink-plugin-auth-v1|{session}|{plugin_id}|{nonce_s}|{nonce_p}|session"

c2s = HKDF-SHA256(ikm = PSK 原始字节, salt, info = "mclink-plugin-c2s-v1" || transcript, 32 字节)
s2c = HKDF-SHA256(ikm = PSK 原始字节, salt, info = "mclink-plugin-s2c-v1" || transcript, 32 字节)
```

- 你发 → 核心用 `c2s`；核心发 → 你用 `s2c`。
- 注意 transcript 的 role 是 `"session"`，**不是** `plugin`/`core`。
- `info` 是**两段字节的拼接**（HKDF 的多段 info），等价于把两段直接连起来。

### 2.4 会话信封

`auth_ok` 之后每一帧都是这个 JSON：

```json
{"v":1,"sid":"<握手时的 session>","seq":42,"n":"<base64 12字节 nonce>","ct":"<base64 密文>"}
```

- 明文 = 下表 `Message` 的 JSON（UTF-8）。
- **AAD = `"v1|" + sid + "|" + seq`**（ASCII，`v` 固定 1）。
- `seq` 每发一帧递增（从 0 开始）；接收侧有 64 位滑动窗口，重复/过旧帧一律拒绝，
  连续 8 次非法帧断开。
- 单帧上限 = `min(清单 max_frame_bytes, 4 MiB)`。

### 2.5 报文（`Message`）

| 类型 | JSON | 说明 |
|---|---|---|
| 请求 | `{"t":"request","id":"1","method":"adapter.init","params":{}}` | 需要回 `response` |
| 响应 | `{"t":"response","id":"1","ok":true,"result":{}}` | `ok:false` 时给 `error:{code,message}` |
| 事件 | `{"t":"event","topic":"adapter.state","data":{}}` | 单向，无回执 |
| 心跳 | `{"t":"ping","at":<ms>}` / `{"t":"pong","at":<ms>}` | 见 2.6 |
| 断开 | `{"t":"bye","reason":"..."}` | 优雅关闭 |

### 2.6 心跳与失联

- 核心每 `heartbeat_ms` 发 `ping`，你必须回 `pong`（原样带回 `at`）。
- **任何入站报文都会刷新 `last_seen`**；但只有在 `idle_timeout_ms`（默认 20 秒）内
  收到过报文才算活着。也就是说：不回 pong 就会被判定失联并断开。

---

### 2.7 用 SDK 写插件（推荐）

手写握手与信封很容易在细节上翻车（PSK 是不是 hex、transcript 的 role、AAD 格式、
seq 起点）。仓库里已经有两套**零依赖** SDK，直接用：

| SDK | 位置 | 适用于 |
|---|---|---|
| `mclink-plugin-sdk`（Rust） | [`sdk/rust`](../sdk/rust) | 原生轨道（`runtime.kind = "process"`） |
| `@mclink/plugin-sdk`（JS） | [`sdk/node`](../sdk/node) | Node 轨道（`runtime.kind = "node"`） |

两者接口对称：同样的方法名、同样的 `context` / `peer` 语义，换语言不用重新学模型。

#### Node（推荐用于业务插件）

```js
import { Plugin } from "@mclink/plugin-sdk";

const plugin = await Plugin.connectFromArgs();

plugin.handle("adapter.init", async () => ({
  ready: true,
  host_fields: [{ key: "port", label: "游戏端口", type: "text", required: true }],
  join_fields: [{ key: "room_code", label: "房间码", type: "text", autofill_from_invite: true }],
}));

plugin.handle("adapter.status", async () => ({ phase: "waiting" }));

// 插件间通信：别的插件 `core.peer.call { method: "room.get" }` 时命中
plugin.onPeer("room.get", async (_params, ctx) => ({ role: ctx.context.role }));

plugin.onContext((ctx) => plugin.log("info", `role=${ctx.role}`));

await plugin.run();
```

`plugin.json` 里对应：

```jsonc
{
  "runtime": { "kind": "node", "entry": "main.js" },
  "permissions": ["net_listen_local", "net_connect_any", "peer_message"]
}
```

#### Rust（推荐用于需要原生 socket 的插件）

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
    plugin.on_peer("room.get", |_params, ctx| async move {
        Ok(json!({ "role": ctx.context().role }))
    });

    plugin.run().await
}
```

`Cargo.toml`：

```toml
[dependencies]
mclink-plugin-sdk = { path = "../../sdk/rust" }   # 或你自己的 git/path 依赖
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde_json = "1"
```

#### SDK 帮你处理了什么

| 事项 | 说明 |
|---|---|
| 会话文件 | 自动找 `--mclink-session`，解析 PSK / 令牌 / 地址 |
| 握手 | 双向证明、`nonce_p` 生成、核心证明校验（防假核心） |
| 密钥 | HKDF 派生与方向隔离 |
| 信封 | AES-256-GCM、AAD、`seq` 单调、重放拒绝 |
| 心跳 | 自动回 `pong`；断线时 `run()` 结束 |
| 分发 | 方法表、未知方法自动回 `not_supported`、异常 → 错误码 |
| peer | `peerList` / `peerCall` / `peerPublish`，以及**深度自动回传** |
| 上下文 | `core.context.update` 的解析与订阅 |

仍然要你自己做的：业务逻辑、长连接管理、持久化（写自己的插件目录）。

---

## 3. 核心 → 插件：你要实现的方法

只有 `kind` 匹配的方法会被调用（`adapter` 插件只会收到 `adapter.*`）。

### 3.1 `adapter`（网络适配器）

| 方法 | 入参 | 返回 | 调用时机 |
|---|---|---|---|
| `adapter.init` | `{}` | 见下 | 每次打开联机页，对**每个**候选适配器各调一次 |
| `adapter.host.start` | 表单字段 + `player_name` + `role:"host"` + `game:{port}` | `{room, port?, players?}` | 房主建房 |
| `adapter.join` | 表单字段 + `player_name` + `role:"guest"` | 同上的形状；`port` = **本机接出端口** | 访客加入 |
| `adapter.host.stop` | `{}` | 任意 | 断开时（与 `leave` 都会发） |
| `adapter.leave` | `{}` | 任意 | 断开时 |
| `adapter.status` | `{}` | `{phase, port?, players?}` | 房间窗口低频轮询（约 3 秒） |

`adapter.init` 的返回**直接决定联机页表单长什么样**：

```jsonc
{
  "ready": true,              // 引擎是否就绪；false 时前端提示"还没准备好"
  "engine": { "name": "EasyTier", "version": "2.4.5", "state": "ready" },  // 可选
  "host_fields": [
    { "key": "port", "label": "游戏端口", "type": "text", "required": true },
    { "key": "room_code", "label": "房间码", "type": "text", "generated": true }
  ],
  "join_fields": [
    { "key": "room_code", "label": "房间码", "type": "text",
      "autofill_from_invite": true, "max_length": 32 }
  ]
}
```

字段对象支持的键：`key`（必填）、`label`、`type`（`text` / `password`）、`required`、
`pattern`、`placeholder`、`autofill_from_invite`、`phase`（`primary` / `password`）、
`generated`（由你生成、房主不用填）、`note`。

> `autofill_from_invite` / `max_length` 也接受 camelCase 写法（`autofillFromInvite`）。

**`phase` 是"状态"契约**：核心用 `phase.ends_with("-ok")` 判断房间是否可用。请回
`host-ok` / `guest-ok`，失败时回其它值（如 `exception`、`waiting`）。

**`players` 是成员契约**（camelCase）：

```json
[{ "name": "Steve", "kind": "HOST", "machineId": "<唯一标识>", "self": true }]
```

跳过匿名项（`name` 与 `machineId` 都为空）会被忽略；`machineId` 用于区分同名玩家。

### 3.2 `detector`（游戏探测）

| 方法 | 入参 | 返回 |
|---|---|---|
| `detector.scan` | `{}` | **数组** `[{ "id": "minecraft-java", "process": "javaw.exe", "name": "我的世界" }]` |

`id` 用于路由（找适配器）；`process` 是展示用的进程名；`name` 是展示名。返回单对象也
兼容，但推荐数组（一次可能扫到多个实例）。

### 3.3 `coupler`（把游戏接进链路）

| 方法 | 入参 | 返回 |
|---|---|---|
| `coupler.attach` | `{ "motd": "…", "port": 25565 }` | 任意（返回值不解析） |
| `coupler.detach` | `{}` | 任意 |

`port` 是**玩家该连的那个端口**（房主=游戏端口；访客=本机接出端口）。

### 3.4 `bridge` / `widget` / `utility`

方法名已定义（`bridge.init|connect|disconnect|status`、`widget.mount|unmount|update`、
`utility.run|status`），但**核心当前不会调用它们**：没有渲染宿主，也没有调用路径。
在这些能力落地前，这三类插件只能靠事件与前端交互。

---

## 4. 插件 → 核心：你能调用的方法

核心对插件发起的 `request` 只接受下面这些，其余返回 `not_supported`。

| 方法 | 入参 | 返回 | 权限 |
|---|---|---|---|
| `core.ping` | — | `{at: <ms>}` | — |
| `core.info` | — | `{server, protocol, pluginId, peerApi:true}` | — |
| `core.peer.list` | — | `PeerPluginInfo[]` | `peer_message` |
| `core.peer.call` | `{to, method, params?, depth?, launch?, timeout_ms?}` | 目标插件返回值 | `peer_message` |
| `core.peer.publish` | `{to?, topic, data?}` | `{accepted:true}` | `peer_message` |

### 4.1 事件（单向）

你可以随时发 `event`，主题自定（1–128 字符）。核心会把它们转发到界面
（`plugin-event`），`data` 原样透传。约定的主题：

`adapter.state`、`detector.found`、`detector.lost`、`coupler.state`、`bridge.state`、
`widget.state`、`utility.state`、`plugin.log`。

> 事件与请求共用**同一个**入站速率桶：默认 32 条/秒。刷事件会被判定超速并断开。

---

## 5. 插件间通信（peer 信道）

插件之间不能直连——它们互相没有身份凭据。所有消息由核心中转，两端仍然只信任核心。

### 5.1 发现

```json
// → 
{"t":"request","id":"1","method":"core.peer.list","params":{}}

// ←
{"t":"response","id":"1","ok":true,"result":[
  {"plugin_id":"dev.example.rooms","kind":"utility","games":["*"],
   "endpoints":[],"version":"1.2.0","alive":true}
]}
```

只列出**可调用**的插件：已启用、未被拉黑、生效了 `peer_message`，且不是你自己。

### 5.2 调用（请求/响应）

```json
{"t":"request","id":"2","method":"core.peer.call","params":{
  "to": "dev.example.rooms",
  "method": "room.directory",         // 你的私有方法名
  "params": { "query": "mc" },
  "timeout_ms": 5000,                 // 可选，默认 10000，夹取 100–30000
  "launch": false,                    // 可选，目标未连接时是否请求核心拉起
  "depth": 0                          // 可选，见 5.4
}}
```

目标插件收到的是**固定方法名**：

```json
{"t":"request","id":"<核心生成>","method":"peer.call","params":{
  "from": "dev.example.my-adapter",
  "method": "room.directory",
  "params": { "query": "mc" },
  "depth": 1,
  "trace": "t7"
}}
```

目标插件正常回 `response` 即可，核心把它的 `result` / `error` 原样转回发起方。

### 5.3 广播（无回执）

```json
{"t":"request","id":"3","method":"core.peer.publish","params":{
  "topic": "room.changed",     // 1–128 字符
  "data": { "code": "ABCD" },
  "to": "dev.example.rooms"    // 省略即广播给所有可调用插件
}}
```

接收方收到：

```json
{"t":"event","topic":"peer.event","data":{
  "from": "dev.example.my-adapter",
  "topic": "room.changed",
  "data": { "code": "ABCD" }
}}
```

### 5.4 规则与限制

| 规则 | 说明 |
|---|---|
| 双向闸门 | 你和目标都必须声明 `peer_message`；目标没声明就收不到任何 peer 消息 |
| 方法名 | 小写字母/数字/`.`/`-`/`_`，≤96 字符；**禁止** `core.` `peer.` `adapter.` `detector.` `coupler.` `bridge.` `widget.` `utility.` 前缀 |
| 不能调自己 | `to` 等于自己的 ID 会被拒 |
| 深度 | 每次中转 +1；你收到 `depth` 后若继续转发，请把它作为 `depth` 传回去。超过 8 层被拒 |
| 并发 | 全局 128 个在途调用，超出立即 `rate_limited` |
| 离线 | 目标不在线时默认失败；`launch: true` 会请求核心拉起（最长等 20 秒） |

> **命名空间为什么这么严**：目标插件永远只看到 `peer.call`，方法名只是载荷里的字符串。
> 所以你无法"借"目标插件的身份去执行 `adapter.host.start` 之类的能力方法——核心从不
> 把 peer 请求当能力方法派发。这条隔离是权限模型不被打穿的前提。

### 5.5 一个可用的协作模式

```text
适配器 A（打洞）                          目录插件 B（房间登记）
  │  拉到一个新对端                          │
  │── core.peer.call {to:B, method:"room.peer_joined",
  │                   params:{room, peer}} ─▶│
  │                                          │  更新内存索引
  │◀───────────── {ok:true, count:3} ────────│
  │  另一个插件 C 询问房间状态                │
  │── core.peer.call {to:B, method:"room.get"} ─▶ │
```

要"掉线也能收"的数据请自己落盘（插件目录可写），peer 信道不做持久化。

---

## 6. 错误码

`response.error.code`（也是核心回给界面的语义）：

| code | 含义 |
|---|---|
| `bad_request` | 参数不合法（缺字段、方法名非法、深度超限） |
| `unauthorized` | 握手/鉴权失败 |
| `forbidden` | 权限不足，或目标插件未声明 `peer_message` |
| `not_supported` | 该插件/该方法不支持 |
| `timeout` | 对端超时 |
| `rate_limited` | 超速或被断开 |
| `internal` | 对端内部错误 |
| `unavailable` | 连接已断开 / 目标未连接 / 插件未注册 |

---

## 7. 权限（15 项）

生效权限 = **清单声明 ∩ 信任度上限 ∩（用户授予 或 自动放行集）**。

| 权限 | 含义 | 高风险 | 未签名插件可用 |
|---|---|---|---|
| `net_listen_local` | 回环监听 | | ✅ 默认 |
| `net_connect_local` | 连本机端口 | | ✅ 默认 |
| `fs_plugin_data` | 读写自身数据 | | ✅ 默认 |
| `net_connect_any` | 连任意地址 | ✅ | ✅ 需授权 |
| `net_udp` | 收发 UDP | ✅ | ✅ 需授权 |
| `peer_message` | 与其它插件收发消息 | | ✅ 需授权 |
| `fs_read_game_dirs` | 读游戏目录 | ✅ | ✅ 需授权 |
| `proc_spawn_self` | 启动自身可执行文件 | | ✅ 需授权 |
| `game_scan` | 扫描本机游戏 | ✅ | ✅ 需授权 |
| `proc_inspect` | 枚举进程 | ✅ | ✅ 需授权 |
| `net_listen_public` | 公网/局域网监听 | ✅ | ❌ 需验签 |
| `net_nat_mapping` | UPnP / NAT-PMP | ✅ | ❌ 需验签 |
| `proc_spawn_game` | 启动游戏 | ✅ | ❌ 需验签 |
| `registry_read` | 只读注册表 | ✅ | ❌ 需验签 |
| `ui_notify` | 向界面推通知 | | ❌ 需验签 |

- "默认"指未签名插件在用户**尚未授权**时自动获得的 `MINIMAL` 集（前 3 项）。
- 用户授权后，`Unsigned` 上限内的项全部可用；上限之外（❌）必须 `Verified`。
- **每类插件有"能力基线"**，拿不到就不会被路由选中：`adapter`→`net_connect_any`、
  `detector`→`game_scan`、`coupler`→`net_connect_local`、`bridge`→`net_connect_local`、
  `widget`→`ui_notify`、`utility`→`fs_plugin_data`。
- ⚠️ **权限是调度级的，不是 OS 沙箱**：插件是普通子进程，自己开 socket 核心拦不住。
  少声明权限是为了让用户看懂你要做什么，不是"反正核心会拦"。

---

## 8. 配额

| 项 | 默认 | 允许范围（核心夹取） |
|---|---|---|
| `max_rpc_per_sec` | 32 | 1–256（**所有**入站报文共用一个桶） |
| `max_frame_bytes` | 1 MiB | 1 B–64 MiB，且会话期再 `min(值, 4 MiB)` |
| `heartbeat_ms` | 5000 | 500–60000 |
| `idle_timeout_ms` | 20000 | 2000–300000 |
| `rpc_timeout_ms` | 30000 | 1000–300000 |
| `max_auth_failures` | 5 | 1–10（超过后进入 60 秒冷却） |
| 网关并发连接 | 32 | 4–128（全局，与单插件无关） |

---

## 9. 手写客户端：把它跑起来的最短路径

> 有 SDK 时**不需要**看这一节。只有在你要为第三种语言写 SDK、或者要验证 SDK 行为时
> 才需要手写；下面的伪码同时是这份文档的"可执行规格"。

没有官方 SDK 时的最小实现要点，按顺序照做即可。

```ts
// 1) 读会话文件（路径来自 --mclink-session 或最后一个参数）
const s = JSON.parse(readFileSync(process.argv.at(-1)!, 'utf8'));

// 2) 建连接
const ws = new WebSocket(`${s.endpoint}?plugin=${s.plugin_id}&token=${s.token}`,
                         'mclink-plugin');

// 3) 握手（第一条消息是 hello）
const hello = JSON.parse(await nextText());
const nonceP = randomHex(32);
const psk = Buffer.from(s.psk, 'hex');                    // 注意：hex → 原始字节
const t1 = `mclink-plugin-auth-v1|${hello.session}|${s.plugin_id}|${hello.nonce_s}|${nonceP}|plugin`;
const proof = createHmac('sha256', psk).update(t1).digest('hex');
sendJson({ t: 'hello_ack', plugin_id: s.plugin_id, plugin_version: '1.0.0',
           nonce_p: nonceP, proof });

// 4) 校验 auth_ok（role = "core"），然后派生密钥
const ok = JSON.parse(await nextText());
const t2 = `mclink-plugin-auth-v1|${hello.session}|${s.plugin_id}|${hello.nonce_s}|${nonceP}|core`;
assert(createHmac('sha256', psk).update(t2).digest('hex') === ok.proof);

const salt = hello.nonce_s + nonceP;                      // ASCII 拼接
const tSess = `mclink-plugin-auth-v1|${hello.session}|${s.plugin_id}|${hello.nonce_s}|${nonceP}|session`;
const c2s = hkdfSha256(psk, salt, Buffer.concat([Buffer.from('mclink-plugin-c2s-v1'), Buffer.from(tSess)]), 32);
const s2c = hkdfSha256(psk, salt, Buffer.concat([Buffer.from('mclink-plugin-s2c-v1'), Buffer.from(tSess)]), 32);

// 5) 收发统一走信封
function seal(msg, seq) {
  const plaintext = Buffer.from(JSON.stringify(msg));
  const nonce = randomBytes(12);
  const aad = Buffer.from(`v1|${hello.session}|${seq}`);
  const cipher = createCipheriv('aes-256-gcm', c2s, nonce);
  cipher.setAAD(aad);
  const ct = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  return JSON.stringify({ v: 1, sid: hello.session, seq, n: nonce.toString('base64'),
                          ct: Buffer.concat([ct, cipher.getAuthTag()]).toString('base64') });
}
// 解密：setAuthTag(最后 16 字节) → decipher.update + final
```

最容易踩的坑，按出现频率排序：

1. **PSK 用成了 hex 文本**（要先解码成 32 字节）。
2. **transcript 的 role 写错**：proof 用 `plugin`/`core`，派生密钥用 `session`。
3. **AAD 拼错**：必须是 `v1|sid|seq`，没有多余空格。
4. **`seq` 不是从 0 开始或不是每帧 +1**：会被当成重放直接断开。
5. **GCM 的 auth tag 位置**：核心把它接在密文末尾（`ct || tag`）。
6. **不回 pong**：20 秒后核心判定失联。

---

## 10. 调试

| 现象 | 先查什么 |
|---|---|
| 插件列表里根本没有它 | 清单是否通过校验（`plugin_list` 返回的 `warnings` 里有原因） |
| 显示了但"停用" | 未验签插件必须由用户显式启用 |
| 启用后联机页看不到 | 权限不足导致过不了能力基线（`plugin_route_plan` 会告诉你为什么没被选中） |
| 连不上 | 会话文件里的 `psk`/`token` 是否被改、握手 transcript 是否与 2.2 完全一致 |
| 连上又立刻断 | 是否回了 `pong`、是否 32 条/秒超速、是否发了未加密帧 |
| 房间一直转圈 | `adapter.status` 的 `phase` 是否以 `-ok` 结尾、`port` 是否是玩家该连的那个 |
| 成员列表是空的 | `players` 的键必须是 `name` / `machineId` / `kind` / `self`（camelCase） |

调试建议：

- 用 `plugin.log` 主题发结构化日志，核心会转发到界面（`plugin-*` 事件里可见）。
- `plugin_route_plan { kind, game_id }` 能看到候选、分值与**入选理由**。
- 生产环境不要依赖 stdout（Windows 上被 `CREATE_NO_WINDOW` 吞掉）。

---

## 11. 现状与未实现（诚实清单）

写插件前请先看这张表，避免按"应该存在"去设计：

| 能力 | 状态 |
|---|---|
| 进程型插件（`runtime.kind = "process"`） | ✅ 可用 |
| Node 型插件（`runtime.kind = "node"`，共享 Bun） | ✅ 可用（运行时需先安装，见 §1.4） |
| 回环 WebSocket + PSK 握手 + AES-GCM 会话 | ✅ 可用 |
| 适配器 / 探测器 / 耦合器的调用链路 | ✅ 可用 |
| 插件间通信（`core.peer.*`） | ✅ 可用 |
| 官方 SDK（Rust / JavaScript） | ✅ 可用，见 [`sdk/rust`](../sdk/rust) 与 [`sdk/node`](../sdk/node) |
| PSK 通过会话文件交付 | ✅ 可用（见 §2.1） |
| 第三方插件安装器 / 签名打包工具 | ❌ 不存在（可手放目录、用户手动启用） |
| `bridge` / `widget` / `utility` 的调用与渲染 | ❌ 无调用路径、无渲染宿主 |
| `detector.watch.start/stop`、`coupler.launch`、`adapter.probe`、`adapter.install` | ❌ 已定名，核心不调用 |
| 插件数据目录 | ❌ 核心不创建；请用插件自身目录或自己的子目录 |
| OS 级沙箱（AppContainer / 受限令牌） | ❌ 未实现 |
| 官方 CLI（`init` / `check` / `pack` / `sign`） | ❌ 不存在，需手工组包 |

---

## 12. 相关文档

| 文档 | 内容 |
|---|---|
| [`插件系统设计.md`](../插件系统设计.md) | 核心侧：协议细节、安全模型、路由与多游戏架构 |
| [`插件开发规范.md`](../插件开发规范.md) | 作者侧工程约束：体积预算、依赖限制、发布流程 |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | 0.6.x 整体分层与批次状态 |
