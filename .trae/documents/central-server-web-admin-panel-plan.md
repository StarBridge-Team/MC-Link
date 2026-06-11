# 中央服务器 Web 管理面板 — 实现计划

## 概要

给中央服务器添加嵌入式 Web 管理面板，通过浏览器实时查看中继、房间、玩家、路径、流量等统计数据。同时在中继端添加流量统计上报功能，让面板能追踪转发流量。

***

## 一、统计项清单

### 无需中继配合（中央服务器已有数据）

| 统计项        | 数据来源                                     | 说明             |
| ---------- | ---------------------------------------- | -------------- |
| 中继总数/在线数   | `relays` + `heartbeat_instants` 超时检测     | 区分在线/离线        |
| 中继类型分布     | `relay.service_type`、`private`、`transit` | 普通/私人/孤岛/各服务类型 |
| 房间数/活跃房间   | `rooms` HashMap                          | 含24小时过期清理      |
| 玩家总数/每房间玩家 | `players` HashMap                        | 各房间玩家列表        |
| 路径数/平均跳数   | `active_paths` + `room_paths`            | 总路径、平均延迟、平均跳数  |
| 中继拓扑连接数    | `topology.edges`                         | 中继间连接数         |
| 中继测速数据     | `latencies` / `topology.edges`           | 中继间延迟/丢包率      |

### 需中继配合（新增流量上报协议）

| 统计项        | 数据来源                   | 说明            |
| ---------- | ---------------------- | ------------- |
| 每个中继的转发流量  | 中继计字节 → 0x38 上报 → 中央存储 | 字节数（发送/接收）    |
| 每个中继的当前连接数 | 中继统计 → 0x38 上报         | 活跃客户端连接数      |
| 总转发流量      | 汇总所有中继                 | 跨所有中继的累计/实时流量 |

***

## 二、修改文件清单

### 文件 1: `central-server/Cargo.toml`

**修改**: +1 行依赖

```toml
tiny_http = "0.12"
```

* 嵌入式 HTTP 服务器，零依赖，单线程即可胜任

### 文件 2: `central-server/src/config.rs`

**修改**: Config 增加 2 个字段

```rust
#[serde(default = "default_web_admin_port")]
pub web_admin_port: u16,
#[serde(default = "default_web_admin_bind")]
pub web_admin_bind: String,
```

* 默认 `web_admin_port=8879`，`web_admin_bind="0.0.0.0"`

### 文件 3: `central-server/src/types.rs`

**修改**: +3 个结构体 + CentralState 加 3 个字段

新增结构体：

```rust
/// 中继上报的流量/连接数据
pub struct TrafficReport {
    pub relay_id: String,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub connections: u32,
    pub timestamp: u64,
}

/// 中央服务器累计的流量统计
pub struct TrafficStats {
    pub bytes_sent_total: u64,
    pub bytes_recv_total: u64,
    pub last_report: u64,
    pub current_connections: u32,
    pub history: VecDeque<(u64, u64, u64)>,  // (timestamp, sent, recv)
}

/// 定时快照（用于面板历史曲线）
pub struct StatsSnapshot {
    pub timestamp: u64,
    pub relay_count: usize,
    pub room_count: usize,
    pub player_count: usize,
    pub path_count: usize,
    pub total_traffic_bytes: u64,
}
```

CentralState 新增：

```rust
pub traffic_reports: Mutex<HashMap<String, TrafficStats>>,
pub stats_history: Mutex<VecDeque<StatsSnapshot>>,  // 保留最近 60 个快照(5分钟)
```

### 文件 4: `central-server/src/handlers.rs`

**修改**: +2 个函数 + 注册命令 0x38

```rust
// handle_packet 中添加:
0x38 => handle_traffic_report(state, payload),

// 新增函数:
pub fn handle_traffic_report(state: &CentralState, data: &[u8]) {
    // 解析 TrafficReport，更新 traffic_reports
}
```

### 文件 5: `central-server/src/stats_snapshot.rs` (新建)

**作用**: 每 5 秒拍一次快照存入 stats\_history

```rust
pub fn stats_snapshot_thread(state: Arc<CentralState>) {
    loop {
        thread::sleep(Duration::from_secs(5));
        // 拿走各 Mutex 快照，计算:
        //   relay_count, room_count, player_count, path_count, total_traffic
        // 推入 stats_history，保留最近 60 个
    }
}
```

### 文件 6: `central-server/src/web_admin.rs` (新建)

**作用**: 启动 HTTP 服务器，提供 REST API + 嵌入式 HTML 面板

```rust
pub fn start_web_admin(state: Arc<CentralState>, bind_addr: SocketAddr) {
    let server = tiny_http::Server::http(bind_addr).unwrap();
    loop {
        // 接受 HTTP 请求 → 路由分发
        // 路由表:
        //   GET /          → HTML dashboard (include_str!("web_admin.html"))
        //   GET /api/stats → JSON 概览
        //   GET /api/relays → JSON 中继列表（含在线状态、流量）
        //   GET /api/rooms  → JSON 房间列表（含玩家）
        //   GET /api/paths  → JSON 路径列表
        //   GET /api/topology → JSON 拓扑数据
        //   GET /api/traffic → JSON 流量统计
        //   GET /api/history → JSON 历史快照曲线
    }
}
```

每个 API 返回格式统一：

```json
{"code": 0, "data": {...}, "timestamp": 1234567890}
```

### 文件 7: `central-server/src/web_admin.html` (新建)

**作用**: 单页 HTML 仪表盘，使用 Chart.js (CDN) 绘制图表

页面布局 (4 列卡片布局)：

* 顶部导航标签：概览 / 中继 / 房间 / 路径 / 拓扑

* 概览页：数字卡片（在线中继、房间数、玩家数、路径数）+ 实时流量曲线

* 中继页：表格（名称、地址、类型、流量、状态、最后活跃时间）

* 房间页：表格（房间名、房主中继、密码、创建时间、玩家列表）

* 路径页：表格（路径ID、房间名、跳数、延迟、路径链）

* 拓扑页：使用 vis-network 或简单力导向图

### 文件 8: `central-server/src/main.rs`

**修改**: 启动时创建 web\_admin 线程 + stats\_snapshot 线程

```rust
// 在 start_server 附近添加:
let web_bind = format!("{}:{}", config.web_admin_bind, config.web_admin_port);
let web_state = state.clone();
thread::spawn(move || {
    web_admin::start_web_admin(web_state, web_bind.parse().unwrap());
});
```

### 文件 9: `mc-link-relay/src/RelayState` (流量统计字段)

**修改**: `main.rs` 中的 `RelayState` 结构体新增 3 个 `AtomicU64` 计数器

```rust
pub traffic_bytes_sent: AtomicU64,   // 累计发送字节
pub traffic_bytes_recv: AtomicU64,   // 累计接收字节
pub traffic_connections: AtomicU32,  // 当前连接数（由 client.rs 维护）
```

### 文件 10: `mc-link-relay/src/relay.rs` (流量计数)

**修改**: 在 `handle_tunnel_frame`、`send_tunnel_direct`、`tunnel_packet_to_relay` 中增加字节计数

```rust
state.traffic_bytes_sent.fetch_add(packet.len(), Ordering::Relaxed);
state.traffic_bytes_recv.fetch_add(received_len, Ordering::Relaxed);
```

### 文件 11: `mc-link-relay/src/client.rs` (连接计数)

**修改**: 在客户端连接/断开时增减 `traffic_connections`

```rust
state.traffic_connections.fetch_add(1, Ordering::Relaxed);  // 连接建立时
state.traffic_connections.fetch_sub(1, Ordering::Relaxed);  // 连接断开时
```

### 文件 12: `mc-link-relay/src/main.rs` (流量上报线程)

**修改**: 新增一个线程，每 30 秒上报流量到中央服务器

```rust
// 新增:
thread::spawn(move || {
    // 每30秒构造 0x38 包发送给中央
    // payload: {"relay_id": ..., "bytes_sent": ..., "bytes_recv": ..., "connections": ...}
    // 然后重置计数器(或继续累计)
});
```

***

## 三、API 设计详情

### `GET /api/stats`

```json
{
  "relays_online": 5,
  "relays_total": 8,
  "rooms_active": 3,
  "players_total": 12,
  "paths_active": 3,
  "avg_latency_ms": 45,
  "avg_hops": 2.3,
  "total_traffic_bytes": 123456789
}
```

### `GET /api/relays`

```json
{
  "relays": [
    {
      "id": "...",
      "name": "Relay-1",
      "address": "1.2.3.4:8877",
      "service_type": "relay",
      "private": false,
      "transit": true,
      "online": true,
      "last_seen": 1234567890,
      "traffic_bytes_sent": 1000000,
      "traffic_bytes_recv": 2000000,
      "connections": 5,
      "latency_entries": { "relay-2": 12, "relay-3": 34 }
    }
  ]
}
```

### `GET /api/rooms`

```json
{
  "rooms": [
    {
      "name": "test123",
      "host_relay_id": "...",
      "password_hash": "sha256hex",
      "created_at": 1234567890,
      "players": [
        { "name": "Player1", "role": "host", "joined_at": ... }
      ]
    }
  ]
}
```

### `GET /api/paths`

```json
{
  "paths": [
    {
      "path_id": "uuid",
      "room_name": "test123",
      "hops": [
        {"node_id": "...", "address": "1.2.3.4:8877"},
        {"node_id": "...", "address": "5.6.7.8:8877"}
      ],
      "total_latency_ms": 45,
      "score": 45.0
    }
  ]
}
```

### `GET /api/topology` = 现有 0x16 命令返回格式的 JSON 化

### `GET /api/history`

```json
{
  "snapshots": [
    {"timestamp": ..., "relay_count": 5, "room_count": 3, ...},
    ...
  ]
}
```

***

## 四、HTML 仪表盘设计

使用 CDN 加载的库：

* Chart.js 4.x → 折线图（流量/在线趋势）

* 纯 CSS 响应式布局，不依赖 UI 框架

页面结构：

```
┌─────────────────────────────────────────────────────┐
│  MC Link Central Admin                     [刷新]   │
├─────────────────────────────────────────────────────┤
│ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌───────────┐ │
│ │中继在线│ │房间数 │ │玩家数 │ │路径数 │ │本日流量    │ │
│ │  12   │ │   5  │ │  18  │ │   4  │ │ 2.3 GB    │ │
│ └──────┘ └──────┘ └──────┘ └──────┘ └───────────┘ │
├─────────────────────────────────────────────────────┤
│  中继在线趋势（过去5分钟）  ┌─────────────┐          │
│  ╱‾╲    ╱╲               │房间分布      │          │
│ ╱  ╲‾╲  ╱ ╲              │ ■ 房间A   3人│          │
│╱    ╲ ╲╱  ╲             │ ■ 房间B   2人│          │
│                        │ ■ 房间C   5人│          │
│                        └─────────────┘          │
├─────────────────────────────────────────────────────┤
│  中继列表 [表格]     搜索: [    ]                   │
│ ┌──────┬──────┬──────┬──────┬──────┬──────┐        │
│ │ 名称 │ 地址 │ 类型 │ 状态 │ 流量 │ 连接 │        │
│ ├──────┼──────┼──────┼──────┼──────┼──────┤        │
│ │ ...  │ ...  │ ...  │ 在线 │ 1.2G │  5  │        │
│ └──────┴──────┴──────┴──────┴──────┴──────┘        │
└─────────────────────────────────────────────────────┘
```

***

## 五、修改步骤（执行顺序）

1. `Cargo.toml` — 添加 `tiny_http` 依赖
2. `config.rs` — 添加 `web_admin_port` 和 `web_admin_bind` 字段
3. `types.rs` — 添加 `TrafficReport`、`TrafficStats`、`StatsSnapshot` 结构体和 CentralState 字段
4. `handlers.rs` — 添加 `handle_traffic_report` (0x38)
5. `stats_snapshot.rs` — 新建定时快照线程
6. `web_admin.rs` — 新建 HTTP 服务器模块
7. `web_admin.html` — 新建 HTML 仪表盘
8. `main.rs` — 启动 web\_admin 和 stats\_snapshot 线程
9. `mc-link-relay/src/main.rs` — RelayState 添加流量计数器
10. `mc-link-relay/src/relay.rs` — 转发函数中插入字节计数
11. `mc-link-relay/src/client.rs` — 连接数增减
12. `mc-link-relay/src/main.rs` — 新增流量上报线程

***

## 六、验收标准

* [ ] 编译通过：`cargo build -p mc-link-central`

* [ ] 编译通过：`cargo build -p mc-link-relay`

* [ ] 启动中央服务器后，浏览器访问 `http://<ip>:8879/` 显示仪表盘

* [ ] 概览页数字卡片显示实时数据

* [ ] 中继页面表格显示所有中继，在线状态正确

* [ ] 房间页面显示房间及玩家列表

* [ ] 路径页面显示路径链

* [ ] 流量曲线随中继上报更新

* [ ] 拓扑页显示中继间连接

* [ ] 中继启动后，中央服务器收到 0x38 流量上报

