# MC Link — 开发约束

> 本文件是**硬约束**，不是介绍文档。**目标态架构见 `docs/ARCHITECTURE.md`**，
> 历史决策与"为什么这么做"见 `project_memory.md`，插件系统见 `插件系统设计.md`。
>
> 历史教训：本文件此前放的是那份 2485 行的 Element Plus 手册，于是每次 AI 辅助都把它当项目约束读取，
> 在错误前提上工作。**不要把参考资料塞进这里。**（该手册已随 Element Plus 一起删除。）

## 1. 这是什么

Tauri 2 + Vue 3 + TypeScript 桌面客户端（`src/` + `src-tauri/`），外加一个自建资源服务器
（`assets-server/`，分发字体、图标、适配器包与更新包）。

- 本仓库**不含任何联机服务端**（`central-server`、`mc-link-relay`、`mc-link-common` 已移除，见归档 tag）
- 联机能力**经插件系统实现**：内置陶瓦适配器 + 检测/耦合插件。`wgp-core` **不是**本仓库依赖，
  也不要为了"恢复联机"把它加回来
- **MC Link 不负责启动游戏**：只扫描、只建链路、只做周边工具
- 0.6.x 是史诗级重构（多房间 / 多窗口 / 组件系统 / 插件市场），结构按批次演进，见
  `docs/ARCHITECTURE.md` 第 8 节。**不要把当前目录形状当成约定**

## 2. 常用命令

| 命令 | 用途 |
|---|---|
| `pnpm dev:fe` | 本地开发（vite + 资源服务器并行）；`pnpm tauri dev` 也走它 |
| `pnpm build` | 类型检查（vue-tsc）+ 前端构建 —— **提交前跑它** |
| `pnpm tauri dev` | 启动完整桌面应用 |
| `pnpm build:release` | 发布：自增版本 → 构建 → 重命名 → 生成更新包与清单 → 上传资源 |
| `pnpm sync:assets` | 只同步/上传资源与更新包 |
| `node scripts/check-arch.mjs` | 架构守卫（CI 也跑） |

## 3. 硬约束（提交前必须满足）

1. **IPC 只写在 `src/lib/api/**`**（0.6.x 起迁移到 `src/ipc/client.ts`，迁移期两者都算合规出口）。
   其它任何位置出现 `invoke(` 都会被守卫拦下——命令改名时散落的调用不会有编译错误，只在运行时炸。
2. **前端本地存储只经 `src/lib/persist.ts`**。不得直接 `localStorage.*`；
   `KEYS` 里的 key 字符串**禁止改名**（会让老用户的设置变成孤儿，表现为"设置丢了"）。
3. **Rust 侧用户数据只经 `crate::persist`**（原子写入 + 损坏隔离）。
   不得 `std::fs::write` 直接写设置类文件；**禁止**"解析失败就用默认值覆盖原文件"。
4. **`commands/*` 只做参数校验与转交**，业务逻辑放 `mgr/` 与 `domain/`。
5. **单文件上限：前端 400 行 / 后端 500 行**（守卫会 warn）。超了就拆，不要"下次再说"。
6. **一次提交只做一件事**，并且**只 `git add` 自己改的文件**：
   本仓库可能被多方/多个 AI 同时编辑，`git add -A` 会把别人正在写的文件卷进你的提交。
   提交后用 `git show --name-only HEAD` 核对清单。
7. **事件必须带标识**：房间相关事件一律带 `sessionId` + 单调 `rev`（进度/日志用 `seq`）。
   新增无标识的全局广播事件会被拒；`rev` 与状态在同一把锁内推进。
8. **权威状态在 Rust**。前端（Pinia）只是投影，每个窗口一份。不得让两个窗口各自持有
   同一份"权威"状态；跨窗口同步只走事件 + 快照。
9. **窗口级显隐动画只能由系统渲染**（`window/animation.rs` 的 `SystemAnimator`），
   不得写 CSS 过渡；**DOM 内部**过渡（页面切换、悬浮组件进出场）仍用 CSS。
   不得为动画改变窗口形态（无边框 + 透明 + 现有材质保持不变）。
10. **一房间一窗口**，label = `room-<sessionId>`。label 是窗口身份唯一权威，路由参数只是镜像。
    不做"主窗口内房间 Tab"。

## 4. 命名规范

- **Tauri 命令**：`<域>_<动词>`，snake_case。0.6.x 新命令去掉 `_command` 后缀
  （如 `room_start_host`）；历史命令保留原名，改名**只改 `src/lib/api` 一处**。
- **Tauri 事件**：`mclink://<域>/<动作>`（如 `mclink://room/state`）。
  历史事件名（`app-log` / `download-progress` / `update-progress` / `tray-resize` / `deep-link`）
  是**现存契约**，改名必须前后端同改，不要单独动一侧。
- **窗口 label**：`main` / `room-<sessionId>` / `widget-tray` / `widget-<kind>-<id>`。
- **文件/组件**：组件 PascalCase；**禁止拼音命名**（历史上出现过 `GaojiConnect.vue`）；
  实现代码不放进 `pages/` 之类语义模糊的目录。
- **Rust 模块**：按职责命名，避免 `utils.rs` 这类万能文件。

## 5. 自动更新（动它之前必读）

两条落地路径，不是一套：

| 场景 | 谁落地 | 读哪份清单 |
|---|---|---|
| Windows **便携版** | 自研（`update/install.rs`，替换 exe） | `latest.json` |
| Windows **安装版** / Linux / macOS | 官方插件 `tauri-plugin-updater` | `tauri.json` |

- **能不能自更新，先看构建渠道**（`src-tauri/src/build_channel.rs`）：
  只有发布流程产出的 `official` 构建允许自动更新；`dev` 连检查都不发起；
  `self-built` 只给手动下载链接。**debug 优先于标记**。
- **发布必须走 `scripts/tauri-build.mjs`**（即 `pnpm build:release`）。
  裸跑 `pnpm tauri build` 得到的产物会被判为 `self-built`，**不会报错，只是永远更新不了**。
- 官方插件路径以 `plugins.updater.pubkey` 为开关；未配置时不注册插件，安装版回退自研路径。
- 下载走 `downloader/verified.rs`（唯一实现）：多镜像、体积上限、空闲超时、
  SHA256 强制校验、分片并行与断点续传（需服务端支持 Range，见 `assets-server/src/server.rs`）。
- **GitHub 代理池**是唯一代理来源：内置 3 个 + 资源服务器下发（按天缓存），
  下载前并发测速择优、失败降级。不要在别处再写一份代理列表。

## 6. 不要做的事

- 不要用 `git add -A` / `git commit -a`（见约束 6）
- 不要把私钥、token 提交进仓库（历史上有过：`.tauri/updater.key`，已移出索引但**必须轮换**）
- 不要在 `commands/*` 里写网络/文件/线程逻辑
- 不要新增第二个下载实现、第二个 IPC 出口、第二套样式 token 来源
- 不要为窗口显隐写 CSS 动画，也不要为了动画改窗口装饰/透明/材质
- 不要把临时产物（归档包、类型检查输出）留在仓库里
- 不要在 `tauri.conf.json` 把 `assetProtocol.scope` 放成全盘 `**`
- 不要保留与代码矛盾的文档：改代码的同一次提交里改掉或删掉它

## 7. 当前状态（2026-10-24）

0.6.x 按批次推进，**完成到哪一步以 `docs/ARCHITECTURE.md` 第 8 节为准**，不要按本节的记忆做假设。

- **B0–B4 已完成**：`core/` + `bridge/`、`domain/room` 多会话、房间窗口（`room.html`）、
  Pinia（`stores/rooms.ts`）+ 唯一事件桥（`ipc/bridge.ts`）
- 会话命令为 `room_prepare / room_start_host / room_join / room_leave / room_snapshot /
  room_refresh / room_list`；`connect_*` 只剩扫描与缓存查询
- **待做**：账号 UUID、代理池、OOBE 加固（B5）；组件系统与托盘、系统窗口动画（B6）；
  设置级联、HMCL 主题、插件市场 6 类（B7）
- 已就绪、可直接复用：插件系统后端（8 命令 + 回环网关 + 权限）、自动更新全链路、
  OOBE 后端与界面、i18n 基础设施（现有 `zh-CN` / `en-US`，0.6.x 要补 `zh-TW`）、
  `@m3e/web` 前端外壳
- 已知缺口：插件**安装器**仍无后端实现；检测类/耦合类插件只有接口与路由，无内置实现
