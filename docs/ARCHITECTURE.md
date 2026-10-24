# MC Link 架构（0.6.x 目标态）

> 本文件描述**目标态**，不是现状。现状与目标不一致之处按文末批次推进。
> 硬约束见 `CLAUDE.md`；历史决策见 `project_memory.md`；插件系统见 `插件系统设计.md`。

## 1. 职责边界

MC Link **不启动游戏**。它只做三件事：

1. 扫描本地游戏（经检测类插件）；
2. 管理联机链路，**可同时运行多个房间**（经适配类 / 耦合类插件）；
3. 提供周边工具（更新、设置、插件市场、账号、公告）。

## 2. 分层与依赖方向

```
commands/   薄适配层：反序列化 → 调 mgr → 返回快照（不写业务、不做 IO）
    ↓
mgr/        唯一编排门面：跨域协调、事务边界
    ↓
domain/     业务状态机：room / account / settings / update / proxy / asset
    ↓
core/       基础设施：events / ids / error / http / lock / cache / persist
```

- `window/` 依赖 `domain/`，**不反向**。
- `domain/` **不得** `use tauri::*`。它只持有 `Arc<dyn EventSink>`，由 `bridge/events.rs` 提供
  Tauri 实现。这条是房间状态机可单测的前提。
- `mgr` 是唯一门面。不新开 `service/` 层与它并存。

## 3. 状态权威与事件契约

**权威状态在 Rust。** 每个 WebviewWindow 有独立 JS 上下文，因此"全局状态" = Rust 全局 +
每窗口一份投影。窗口关闭不影响链路。

| 事件 | 负载 | 丢失容忍 | 说明 |
|---|---|---|---|
| `mclink://room/state` | `{ sessionId, rev, snapshot }` | 不容忍 | 全量快照，不做 diff |
| `mclink://room/progress` | `{ sessionId, seq, stage, message }` | 容忍 | Rust 侧 100–200ms 节流 |
| `mclink://room/log` | `{ sessionId, seq, line }` | 容忍 | `seq` 断档即清空重拉 |
| `mclink://app/settings` | `{ rev, patch }` | 容忍 | 拉到全量即纠偏 |
| `mclink://window` | `{ action }` | 不容忍 | 定向 `emit_to(label)`，不广播 |

规则：

- **`rev` 与状态在同一把锁内推进，emit 也在锁内**（Tauri emit 非阻塞），保证同一会话事件有序。
- 前端应用前判 `if (evt.rev <= local.rev) return;`，于是"命令返回的快照"与"随后到达的事件"
  谁先到都幂等。
- **锁粒度按会话**。全局 `write_lock` 只服务配置文件；房间锁在 `RoomRegistry` 内部，
  否则一次 20MB 更新包下载会把建房卡住。
- **禁止新增没有 `sessionId` 的全局事件**。现有 `connect-event` 就是反例，多房间必须废掉。

### 挂载时序（每个窗口都必须遵守）

```
1. listen(全部 topic)      ← 先订阅，堵住空窗
2. 事件进 buffer
3. invoke snapshot()       ← 拿权威基线
4. 应用快照 → 重放 buffer 中 rev > 基线 的事件
5. live 模式
```

只在 `src/ipc/bridge.ts` 实现一次，`app.mount()` 之前调用。**任何组件/composable 不得自己
`listen`**。低频纠偏：窗口 `focus` 且距上次同步超过 N 秒时静默 `snapshot()` 一次。

## 4. 命令契约（用例级）

一个命令 = 一次用户可感知的意图。前端 Pinia Action 与它一一对应，中间不做组装。

| 命令 | 语义 | 返回 |
|---|---|---|
| `room_prepare { gameId }` | 一次拿齐适配器 + 本机游戏 + 账号 | `{ adapters, games, account }` |
| `room_start_host { adapterId, gameId, fields }` | 建房（后台任务，立即返回） | `RoomSnapshot`（含 `sessionId`） |
| `room_join { adapterId, gameId, fields }` | 加入 | `RoomSnapshot` |
| `room_leave { sessionId }` | 断开并关闭会话 | `RoomSnapshot` |
| `room_snapshot { sessionId }` | 拉单个会话 | `RoomSnapshot` |
| `room_list` | 拉全部会话 | `RoomSnapshot[]` |

- 变更类命令**返回变更后的快照**：发起方不依赖自己的事件。
- 去重：同一 `(adapterId, gameId)` 在非 `closed` 阶段已存在会话时返回既有会话，防双击开两个房间。

## 5. 窗口模型

| 窗口 | label | 持有 | 不持有 |
|---|---|---|---|
| 主窗口 | `main` | 房间列表、设置、插件、账号 | 任何单个房间的细节 |
| 房间窗口 | `room-<sessionId>` | 该会话快照 | 其他会话、房间列表 |
| 托盘面板 | `widget-tray` | 房间列表摘要（只读） | 写操作 |
| 外部小组件 | `widget-<kind>-<id>` | 自身位置/尺寸 | 业务状态 |

- **label 是窗口身份的唯一权威**，路由参数只是镜像。
- 房间窗口形态**与主窗口一致**：`decorations(false)` + `transparent(true)` + 现有材质。
  关闭房间窗口 = 只关窗口，不断链路；主窗口隐藏时房间窗口保持可见。
- 小组件窗口**预建常驻**（`visible(false)`），只 `show()/hide()`，绝不每次新建。
  冷建 WebView 在 Windows 上是 200–600ms，必然闪。托盘面板即按此实现
  （`window::widget`：位置由托盘图标的 `rect` 推算并夹进工作区，尺寸按行数计算）。
- **非主窗口不铺背景**：房间窗口与小组件只用「材质 + 纯色」，不加载背景图/视频。
  这是刻意的取舍——多窗口同步背景需要每个窗口各跑一遍取色与缓存，收益不抵成本。

### 组件系统的两半

| 形态 | 谁协调 | 状态 |
|---|---|---|
| 外部小组件（独立透明窗口） | Rust：创建/尺寸/位置/显隐动画 | 已落地（托盘面板为第一个实例） |
| 悬浮组件（窗口内 DOM 浮层） | 前端：拖拽、层级、焦点窗口过滤 | **未做**：等有真实组件要挂时再做，避免先造空壳 |

### 窗口级动画：**不做**（`AnimateWindow` 已被证伪）

`window/animation.rs` 只保留 `show` / `hide` 两个瞬时入口，不带动画。

曾经的方案是 Windows `AnimateWindow(AW_BLEND)`，实测被移除：

- 它是**同步阻塞**的，而 Tauri 的同步命令跑在主线程（窗口消息泵所在线程）。
  在消息处理中调用会把消息泵卡死：主窗口无法最小化/关闭、托盘点击无响应。
- 透明窗口在 Windows 上是 layered window，`AW_BLEND` 对它本就不保证成功；
  实测房间窗口与托盘面板变成一片透明/空白（只剩材质）。

结论：**没有既"系统渲染"又不阻塞消息泵、还能作用于透明 WebView 窗口的公开 API**。
要恢复动画，只有三条路，都需要重新评估：窗口内 CSS 过渡（用户明确不要）、
DWM 过渡（只能关不能定制）、自绘分层窗口（成本远超收益）。

- **不得改变窗口形态**（不加装饰、不改透明、不改材质）。
- **DOM 内部**过渡（页面切换、悬浮组件进出场）仍用 CSS。

### 小组件位置持久化

存**相对锚点**而非绝对坐标，否则换屏/改缩放就跑到屏幕外：

```
WidgetPlacement { monitor, dx, dy, w, h, edge, margin }   // dx/dy/w/h 均为逻辑单位
```

恢复时按同名显示器 → 主屏 → 最后一律 clamp 进 `work_area`。

## 6. 目录结构

### Rust

```
src-tauri/src/
├─ lib.rs                  # 只做 wiring
├─ bootstrap/              # 启动期一次性动作（setup / deep_link / effects / single_instance）
├─ core/                   # error / events / ids / http / lock / data_dir / cache / persist
├─ domain/
│  ├─ account/             # model / service / uuid（离线 UUID，Java 同算法）
│  ├─ room/                # model / registry（状态机 + 多会话表）★ 多房间核心
│  ├─ game/                # 扫描结果缓存
│  ├─ settings/            # model / store / remote_meta
│  ├─ update/
│  ├─ proxy/               # pool / probe ★ 代理池（内置 3 + 远端按天缓存）
│  └─ asset/
├─ plugin/                 # 保留，PluginKind 扩到 6 类
├─ window/                 # factory / widget / animation  ★ 全局窗口管理器
├─ mgr/                    # room（编排：调插件）/ account / settings / update / window
├─ commands/               # 按域拆分，一文件一域
└─ bridge/events.rs        # EventSink 的 Tauri 实现（唯一 emit 出口）
```

### 前端

```
src/
├─ entries/                # main.ts / room.ts / widget.ts（三个 HTML 入口）
├─ app/                    # MainApp.vue / RoomApp.vue / WidgetApp.vue
├─ stores/                 # rooms / roomSession / settings / account / plugins / windows / ui
├─ ipc/                    # client.ts / events.ts / bridge.ts / contracts/
├─ features/               # 按域垂直切分（home / connect / rooms / room / games / plugins / settings / account / oobe）
├─ ui/                     # Icon.vue / WindowFrame.vue / floating/
├─ router/ i18n/ styles/
```

`features/` 是目标态，`views/` + `components/` + `composables/` 是过渡态：一个域搬完就删旧文件，
不允许两代并存超过一个批次。

## 7. 页面结构

主窗口导航：**首页 / 联机 / 房间 / 游戏 / 插件 / 设置**

| 页面 | 要点 |
|---|---|
| 首页 | 公告 Banner（远端）→ 进行中房间 → 最近游戏 → 快捷入口 |
| 联机 | 房主/成员两 Tab；提交后**开房间窗口**，主窗口留在原地可继续建第二个 |
| 房间 | 会话总览（游戏/角色/码/人数/时长/延迟），可聚焦窗口、复制邀请、断开；搜索+筛选 |
| 游戏 | 已发现游戏 + 详情；搜索/筛选 |
| 插件 | 市场 / 已安装 / 权限；搜索、依赖链拓扑、6 类筛选 |
| 设置 | 左树右内容 + 顶部搜索，由 `settings/registry.ts` 驱动（级联、去重、服务端分区同表注册） |

房间窗口：自绘标题栏（房间码 + 延迟 + 状态灯）+ 成员网格 + 分享区 + 链路信息 + 日志抽屉
+ 悬浮组件层。

## 8. 批次与验收

| 批次 | 内容 | 验收 | 状态 |
|---|---|---|---|
| B0 | 打 tag、清 `.gitignore`、提交现状 | `git status --porcelain` 为空 | 完成（`snapshot/pre-0.6-refactor`） |
| B1 | `core/` + `bridge/` | `cargo test --lib` 通过 | 完成 |
| B2 | `domain/room` + 用例命令（单房间跑通） | 旧 `connect_*` 会话命令全部删除 | 完成 |
| B3 | 房间窗口 + 多房间 | 同开 2 个房间，关主窗口链路不断 | 完成 |
| B4 | Pinia + `bridge.ts` | `mclink://room` 只出现在 `src/ipc/**` | 完成 |
| B5 | 账号 UUID / 代理池 / OOBE 加固 | 单测 + 断网演练 | 完成（UUID 有 Java 同值测试；代理池接进更新链路） |
| B6 | 托盘重构 + 系统窗口动画 | 动画降级路径可复现 | 完成（**窗口内悬浮组件未做**，见下） |
| B7 | 设置级联 + HMCL 主题 + 插件市场 6 类 | — | 待做 |

每批次独立可回滚，结束时 build 绿 + 该批次验收命令通过。
