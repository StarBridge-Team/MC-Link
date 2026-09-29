# 实现计划：IPv6 客户端作为中继节点

## 概述

当用户拥有公网 IPv6 地址时，其客户端不仅作为普通客户端，还作为一个轻量中继节点运行。中央服务器管理统一的拓扑管理器，读多写少场景使用 RwLock；根据连接方是否具备 IPv6 能力使用不同的视图计算路径。

---

## 当前状态分析

### 中继注册流程 (central-server)
- [relay.rs](file:///e:/Work%20Files/Code/Tauri/mc-link/central-server/src/relay.rs): `handle_relay_register` 接收 `RelayRegisterReq`，写入 `RelayNode`、`topology`、`relay_streams`
- [handlers.rs](file:///e:/Work%20Files/Code/Tauri/mc-link/central-server/src/handlers.rs): `handle_client` 处理断开 → 清理中继，触发重路由
- [types.rs](file:///e:/Work%20Files/Code/Tauri/mc-link/central-server/src/types.rs): `CentralState` 只有单张 `topology: Mutex<TopologyGraph>`

### 中继列表查询
- `0x12` → `handle_get_relays` 过滤 `!r.private` 返回公开中继
- [central.rs](file:///e:/Work%20Files/Code/Tauri/mc-link/src-tauri/src/central.rs): 客户端用 `get_relays()` 获取公开中继列表，60秒缓存

### 路径计算
- [path.rs](file:///e:/Work%20Files/Code/Tauri/mc-link/central-server/src/path.rs): `assign_room_path` 从单张 `topology` 计算路径，`push_room_route_to_relays` 推给所有中继
- 路径中的 `PathHop` 只使用 `address` + `address_v6`

### 客户端架构
- [host.rs](file:///e:/Work%20Files/Code/Tauri/mc-link/src-tauri/src/host.rs): 房主模式，CONNECT → REGH → 双向转发
- [client.rs](file:///e:/Work%20Files/Code/Tauri/mc-link/src-tauri/src/client.rs): 成员模式，监听本地端口 → REGC → 双向转发
- 两者都没有中继监听或注册功能

---

## 变更方案

### 第一阶段：中央服务器 — TopologyManager + 双视图

#### `types.rs` — 引入 `TopologyManager`，使用 RwLock

不再维护两张分离的 `Mutex<TopologyGraph>`，而是引入统一的 `TopologyManager`，内部以 RwLock 保护：

```rust
use std::sync::RwLock;

/// 拓扑管理器 — 统一管理纯 IPv4 和混合两张拓扑表
/// 读操作（路径计算、列表查询）远多于写操作（注册、断开），故使用 RwLock
pub struct TopologyManager {
    /// 纯 IPv4 拓扑：仅包含官方中继（非 private）
    pub ipv4: TopologyGraph,
    /// 混合拓扑：官方中继 + 有 IPv6 的客户端中继
    pub mixed: TopologyGraph,
}

pub struct CentralState {
    // 替换原有的 topology: Mutex<TopologyGraph>
    pub topology_manager: RwLock<TopologyManager>,
    // ... 其他字段不变
}
```

优势：
- **读写分离**：路径计算等高并发读操作允许多线程并发读取，不受彼此阻塞
- **原子更新**：注册/断开操作一次获取写锁，同时更新两张表，消除死锁风险
- **接口统一**：路径计算时只需要 `topology_manager.read()` 获取对应视图

#### `relay.rs` — 注册时更新两张表

```rust
let mut mgr = state.topology_manager.write().unwrap();
// 始终加入纯 IPv4 拓扑
mgr.ipv4.add_or_update_node(relay.clone());
// 有 IPv6 且 transit 的中继加入混合拓扑
if relay.address_v6.is_some() && relay.transit {
    mgr.mixed.add_or_update_node(relay.clone());
}
```

#### `relay.rs` — 中继断开时统一清理

`handle_client` 断开清理中，一次获取写锁同时清理两张表：

```rust
let mut mgr = state.topology_manager.write().unwrap();
mgr.ipv4.remove_node(relay_id);
mgr.mixed.remove_node(relay_id);
```

#### `relay.rs` — 新增 `handle_get_mixed_relays` 命令 `0x17`

```rust
pub fn handle_get_mixed_relays(stream: &mut TcpStream, state: &CentralState) {
    // 从 relays 表中获取所有有 IPv6 的中继
    let relays = state.relays.lock().unwrap();
    let relay_list: Vec<&RelayNode> = relays.values()
        .filter(|r| r.address_v6.is_some() && r.transit)
        .collect();
    // 响应 0x18
}
```

#### `handlers.rs` — 添加 `0x17` 路由

```rust
0x17 => handle_get_mixed_relays(stream, state),
```

#### `path.rs` — 路径计算从 TopologyManager 获取对应视图

`assign_room_path` 新增 `use_mixed: bool` 参数：

```rust
pub fn assign_room_path(state: &CentralState, room_name: &str,
    host_relay_id: &str, client_relay_id: &str, use_mixed: bool)
{
    let mgr = state.topology_manager.read().unwrap();
    let topology = if use_mixed { &mgr.mixed } else { &mgr.ipv4 };
    // ... find_optimal_path 逻辑不变，从 topology 读取
}
```

由 `handle_join_room`（0x26）和 `handle_create_room`（0x20）根据客户端请求中的 `has_ipv6` 字段传入 `use_mixed`。

#### Cleanup 线程 — 同步清理

```rust
let mut mgr = state.topology_manager.write().unwrap();
for id in &dead_ids {
    mgr.ipv4.remove_node(id);
    mgr.mixed.remove_node(id);
}
```

#### 重路由 Debounce（防抖动 + 信令风暴控制）

在 `handlers.rs` 的 `handle_client` 断开逻辑中，将重路由改为延迟批量处理：

```rust
// 断开时，不立即重路由，而是将受影响的中继 ID 放入待处理队列
state.pending_reroute.lock().unwrap().push(relay_id.clone());

// 在 cleanup_thread 中，每 100ms 批量处理一次:
fn process_pending_reroutes(state: &CentralState) {
    let pending = state.pending_reroute.lock().unwrap().drain(..).collect::<Vec<_>>();
    if pending.is_empty() { return; }
    // 去重
    let mut unique: HashSet<&str> = HashSet::new();
    for id in &pending { unique.insert(id.as_str()); }
    for id in unique {
        reroute_affected_paths(state, id);
    }
}
```

在 `CentralState` 新增：
```rust
pub pending_reroute: Mutex<Vec<String>>,
```

---

### 第二阶段：客户端 — 轻量中继节点 (ClientRelay)

#### 新建 `src-tauri/src/client_relay.rs`

##### 2.1 核心结构 — 异步架构

基于 Tokio 全异步模型，使用 CancellationToken 管理生命周期：

```rust
use tokio_util::sync::CancellationToken;
use tokio::sync::Mutex as AsyncMutex;
use bytes::Bytes;

pub struct ClientRelay {
    relay_id: String,
    listener_port: u16,
    max_connections: u32,       // 连接数上限，默认 8
    max_bandwidth_bps: u64,     // 带宽上限，默认 5 Mbps
    cancel_token: CancellationToken,
    // 路径表（异步 Mutex）
    path_table: Arc<AsyncMutex<HashMap<String, PathAssignment>>>,
    // 房间路由表
    room_routes: Arc<AsyncMutex<(HashMap<String, RoomRoute>, HashMap<String, RoomRoute>)>>,
    // 对等连接
    peer_connections: Arc<AsyncMutex<HashMap<String, Arc<AsyncMutex<TcpStream>>>>>,
    // 日志
    log_callback: Arc<dyn Fn(String) + Send + Sync>,
}
```

##### 2.2 `register_with_central()` — 注册 + 限速标记

```rust
async fn register_with_central(&self) -> Result<()> {
    let central_addr = resolve_address("mk.aini2.cn:8878")?;
    let mut stream = TcpStream::connect(central_addr).await?;

    let req = serde_json::json!({
        "id": self.relay_id,
        "name": format!("Client-{}", &self.relay_id[7..19]),
        "address": format!("{}:{}", local_ip(), self.listener_port),
        "address_v6": format!("[{}]:{}", local_ipv6(), self.listener_port),
        "private": true,
        "transit": true,
        "service_type": "client-relay",
        // 告知中央服务器本客户端中继的容量上限
        "client_caps": {
            "max_connections": self.max_connections,
            "max_bandwidth_bps": self.max_bandwidth_bps,
        }
    });
    // 发送 0x10 包...
}
```

`RelayRegisterReq` 新增可选字段 `client_caps`，中央服务器在路径计算时可根据此数据决定是否将路径分配通过此节点。

##### 2.3 `heartbeat_task()` — 独立异步任务

```rust
async fn heartbeat_task(stream: Arc<AsyncMutex<TcpStream>>, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let packet = serde_json::json!({"id": relay_id});
                let mut buf = vec![0x11];
                buf.extend_from_slice(packet.to_string().as_bytes());
                let mut s = stream.lock().await;
                let _ = write_packet(&mut s, &buf).await;
            }
            _ = cancel.cancelled() => break,
        }
    }
}
```

##### 2.4 `relay_listener_task()` — 异步监听 + 连接上限控制

```rust
async fn relay_listener_task(&self, cancel: CancellationToken) {
    let listener = TcpListener::bind(format!("[::]:{}", self.listener_port)).await
        .or_else(|_| TcpListener::bind(format!("0.0.0.0:{}", self.listener_port)).await)
        .unwrap();

    let active_connections = Arc::new(AtomicU32::new(0));

    loop {
        tokio::select! {
            result = listener.accept() => {
                let current = active_connections.load(Ordering::Relaxed);
                if current >= self.max_connections {
                    log("[ClientRelay] 达到连接数上限，拒绝新连接");
                    continue;
                }
                active_connections.fetch_add(1, Ordering::Relaxed);
                // 处理连接...
            }
            _ = cancel.cancelled() => break,
        }
    }
}
```

##### 2.5 隧道帧处理 — 零拷贝转发

```rust
async fn handle_tunnel_connection(stream: TcpStream, ...) {
    let mut buf = Vec::with_capacity(4096);
    loop {
        // 复用缓冲区，避免每次分配
        buf.clear();
        match read_packet_buf(&mut stream, &mut buf).await {
            Ok(()) => {}
            Err(_) => break,
        }

        if buf.is_empty() || buf[0] != 0x34 {
            continue;
        }

        let packet_data = &buf[1..]; // 注意：这里的 buf 所有权已返回
        let header = match TunnelFrameHeader::decode(packet_data) {
            Some(h) => h,
            None => continue,
        };

        let path_id_hex = hex::encode(header.path_id);
        let path = match path_table.lock().await.get(&path_id_hex) {
            Some(p) => p.clone(),
            None => continue,
        };

        let payload_offset = TunnelFrameHeader::SIZE;
        let payload = &packet_data[payload_offset..];

        let cur_pos = path.hops.iter().position(|h| h.node_id == relay_id);
        // ... 判断 terminal 或转发
        // 转发时直接发送 header.encode() + payload，避免分配中间 Vec
    }
}
```

热路径优化：
- 使用 `Vec::with_capacity` 预分配缓冲区，复用避免重复分配
- 转发时 `write_all(&[header.encode(), payload].concat())` 改为分两次 `write_all` 避免拼接
- 路径表用 `Arc<PathAssignment>` 减少克隆开销

##### 2.6 优雅退出

`CancellationToken` 触发 → 监听器退出 → 发送断开包到中央 → 关闭所有对等连接 → 清理本地端口

```rust
pub async fn shutdown(&self) {
    self.cancel_token.cancel();
    // 发送中央断开通知
    // 等 tokio::time::sleep(Duration::from_millis(200)) 让任务完成
    // 关闭所有 peer_connections
}
```

##### 2.7 带宽限速

在转发热路径中加入简单的令牌桶：

```rust
struct TokenBucket {
    capacity: u64,        // 容量（字节）
    tokens: AtomicU64,
    refill_rate: u64,     // 每毫秒补充字节数
}

impl TokenBucket {
    fn new(max_bps: u64) -> Self {
        Self {
            capacity: max_bps / 8,  // 1 秒的突发量
            tokens: AtomicU64::new(max_bps / 8),
            refill_rate: max_bps / 8 / 1000,  // 每毫秒
        }
    }

    fn try_consume(&self, bytes: u64) -> bool {
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current < bytes { return false; }
            if self.tokens.compare_exchange_weak(current, current - bytes, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                return true;
            }
        }
    }

    fn refill(&self) {
        let now = Instant::now();
        // 每 50ms 补充一次
        let elapsed = now.elapsed().as_millis() as u64;
        let add = elapsed * self.refill_rate;
        if add > 0 {
            self.tokens.fetch_add(add.min(self.capacity), Ordering::Relaxed);
        }
    }
}
```

#### 修改 `lib.rs` — 导出 ClientRelay

```rust
pub mod client_relay;
```

#### 修改 `commands.rs` — 异步生命周期管理

使用 `AppState` 存储 `JoinHandle` + `CancellationToken`：

```rust
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub client_relay_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    pub client_relay_cancel: Arc<Mutex<Option<CancellationToken>>>,
}

#[tauri::command]
async fn start_client_relay(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if !has_ipv6_connectivity() {
        return Ok(()); // 没有 IPv6 就不启动
    }
    let cancel = CancellationToken::new();
    let handle = tokio::spawn(async move {
        let relay = ClientRelay::new(...);
        relay.run(cancel.clone()).await;
    });
    *state.client_relay_handle.lock().unwrap() = Some(handle);
    *state.client_relay_cancel.lock().unwrap() = Some(cancel);
    Ok(())
}

#[tauri::command]
async fn stop_client_relay(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(cancel) = state.client_relay_cancel.lock().unwrap().take() {
        cancel.cancel();
    }
    if let Some(handle) = state.client_relay_handle.lock().unwrap().take() {
        handle.await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

前端在联机开始/结束时分别调用这两个命令。

---

### 第三阶段：联机流程集成

#### `central.rs` — 新增 `get_mixed_relays()`

```rust
pub fn get_mixed_relays() -> Option<Vec<RelayInfo>> {
    let response = send_request(0x17, &[])?;
    if !response.is_empty() && response[0] == 0x18 {
        serde_json::from_slice::<Vec<RelayInfo>>(&response[1..]).ok()
    } else {
        None
    }
}
```

#### `central.rs` — 创建/加入房间传递 IPv6 能力

```rust
pub fn create_room(..., has_ipv6: bool) -> ... {
    let mut req = serde_json::json!({..., "has_ipv6": has_ipv6});
    ...
}
```

#### FlashConnect.vue / GaojiConnect.vue

- 联机开始前调用 `start_client_relay` 命令
- 联机结束后调用 `stop_client_relay` 命令
- 在请求中传递当前客户端是否有 IPv6

#### 中央服务器房间处理器更新

`handle_create_room` 和 `handle_join_room` 接收 `has_ipv6` 字段，传给 `assign_room_path` 的 `use_mixed` 参数。

---

## 实施顺序

### Step 1: 中央服务器 — TopologyManager + Debounce
- 引入 `TopologyManager` 结构体 + RwLock
- 替换 `CentralState.topology` 为 `topology_manager`
- 修改 `handle_relay_register` 写入双视图
- 修改 `handle_client` 断开清理 + debounce 队列
- 添加 `0x17/0x18` 混合中继列表命令
- 修改 `assign_room_path` 支持 `use_mixed` 参数
- `RelayRegisterReq` 添加 `client_caps` 可选字段
- 修改 `cleanup_thread` 清理双视图
- 添加 `pending_reroute` 字段和批量处理

### Step 2: 客户端 — ClientRelay（全异步）
- 创建 `client_relay.rs`
  - 注册 + 心跳（独立 tokio task）
  - 异步监听 + 隧道帧零拷贝转发
  - 连接数上限 + 令牌桶带宽限速
  - CancellationToken 优雅退出
- `RelayRegisterReq` 解析 `client_caps`
- 集成到 `lib.rs`
- `AppState` 管理 ClientRelay 生命周期
- `commands.rs` 新增 `start_client_relay` / `stop_client_relay`

### Step 3: 前端集成
- `central.rs` 新增 `get_mixed_relays()`
- `create_room` / `join_room` 传递 `has_ipv6`
- FlashConnect.vue / GaojiConnect.vue 联机流程调用

---

## 注意事项

| 类别 | 要点 |
|------|------|
| 锁设计 | TopologyManager 使用 RwLock（读多写少），避免 Mutex 顺序死锁 |
| 防滥用 | Client-Relay 注册速率限制；`max_connections=8` 默认上限 |
| 带宽控制 | 令牌桶算法限速（默认 5 Mbps），超限帧直接丢弃 |
| 零拷贝 | 热路径分两次 write_all 避免拼接；复用缓冲区避免高频分配 |
| 信令风暴 | 断开 debounce 100ms 批量处理，去重后再重路由 |
| 生命周期 | CancellationToken + tokio::spawn，app 关闭自动清理 |
| 端口释放 | TcpListener 被 drop 时自动释放，无需手动 close |
| 兼容性 | 旧客户端/旧中继不受影响，中央服务器 `0x12` 接口不变 |