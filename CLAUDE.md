# MC Link — 开发约束

> 本文件是**硬约束**，不是介绍文档。项目背景、决策记录与"为什么这么做"见 `project_memory.md`。
> 参考资料在 `docs/reference/`：`element-plus-manual.md` 是**第三方组件库手册**，不是本项目约定。
>
> 历史教训：本文件此前放的是那份 2485 行的 Element Plus 手册，于是每次 AI 辅助都把它当项目约束读取，
> 在错误前提上工作。**不要把参考资料塞进这里。**

## 1. 这是什么

Tauri 2 + Vue 3 + TypeScript 桌面客户端（`src/` + `src-tauri/`），外加一个自建资源服务器
（`assets-server/`，分发字体、图标、适配器包与更新包）。

- 本仓库**不含任何联机服务端**（`central-server`、`mc-link-relay`、`mc-link-common` 已移除，见归档 tag）
- 联机核心（WGP）**按用户要求暂时断开**：`src-tauri` 不再依赖仓库外的 `mc-link-core`，
  因此**本仓库可独立构建**。重接步骤见 `project_memory.md` 的"联机核心"章节
- 前端 UI 正在**大幅重构**：结构随时在变，不要把当前目录形状当成约定

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

1. **IPC 只写在 `src/lib/api/**`**。其它任何位置出现 `invoke(` 都会被守卫拦下——
   命令改名时散落的调用不会有编译错误，只在运行时炸。
2. **前端本地存储只经 `src/lib/persist.ts`**。不得直接 `localStorage.*`；
   `KEYS` 里的 key 字符串**禁止改名**（会让老用户的设置变成孤儿，表现为"设置丢了"）。
3. **Rust 侧用户数据只经 `crate::persist`**（原子写入 + 损坏隔离）。
   不得 `std::fs::write` 直接写设置类文件；**禁止**"解析失败就用默认值覆盖原文件"。
4. **`commands/*` 只做参数校验与转交**，业务逻辑放功能模块（`plugin/`、`update/`、`downloader/`…）。
5. **单文件上限：前端 400 行 / 后端 500 行**（守卫会 warn）。超了就拆，不要"下次再说"。
6. **一次提交只做一件事**，并且**只 `git add` 自己改的文件**：
   本仓库可能被多方/多个 AI 同时编辑，`git add -A` 会把别人正在写的文件卷进你的提交。
   提交后用 `git show --name-only HEAD` 核对清单。

## 4. 命名规范

- **Tauri 命令**：`<域>_<动词>`，snake_case。现状仍带 `_command` 后缀（历史遗留），
  改名时**只改 `src/lib/api` 一处**即可（这正是约束 1 存在的意义）。全量统一，不要搞个别例外。
- **Tauri 事件**：现状拼写不统一（`app-log` / `download-progress` / `p2p-event` / `tray-resize`）；
  新增事件用 `<域>:<动作>`（如 `app:log`），整体重命名属待办。
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

## 6. 不要做的事

- 不要用 `git add -A` / `git commit -a`（见约束 6）
- 不要把私钥、token 提交进仓库（历史上有过：`.tauri/updater.key`，已移出索引但**必须轮换**）
- 不要在 `commands/*` 里写网络/文件/线程逻辑
- 不要新增第二个下载实现、第二个 IPC 出口、第二套样式 token 来源
- 不要把临时产物（归档包、类型检查输出）留在仓库里
- 不要在 `tauri.conf.json` 把 `assetProtocol.scope` 放成全盘 `**`

## 7. 现在的状态（避免重复劳动或做错假设）

- **OOBE**：后端与契约**已就绪**（`src-tauri/src/setup.rs` + 4 个命令），**引导界面未实现**
- **i18n**：基础设施已就绪（`src/i18n/` + `useSetup`），切换语言会立即生效
- **插件系统**：后端完整（8 个命令 + 回环网关 + 权限），**插件管理界面未实现**
- **自动更新**：链路完整，但 `plugins.updater.pubkey` 仍为空 → 官方插件路径未激活
- **联机**：按用户要求断开，重接参考 `legacy-wgp-p2p-v0.4.0`
- **未接线的后端能力**：检查更新接界面、玩家名写入、分区设置 —— 详见 `project_memory.md`
