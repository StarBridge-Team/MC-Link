# 项目记忆 - MC Link

> 此文件记录项目的重要架构决策和用户明确要求，供后续开发和维护参考。

## 联机核心（2026-09-30 更新）

### 现状：与 WGP Core 的连接已暂时断开

- 已移除 `wgp-core` 的跨目录 path 依赖及其 Tauri 命令。**项目现在可独立构建，不再需要仓库外的 `mc-link-core`。**
- 过渡期客户端**不含可用的联机功能**：联机页（`components/connect/`）与 `lib/api/connect.ts` 保留，但调用会失败。
- **用户明确要求：上述前端文件保持现状——不要修改、不要删除、不要"修复"。**
  它们将作为 UI 重构与插件系统的素材；联机能力在那一阶段之后再重新接入。
  过渡期点击"开始联机"报 `command not found` 属于**预期现象**，不是 bug，无需处理。
- **重新接入参考**：`legacy-wgp-p2p-v0.4.0` tag，包含：
  - `src-tauri/src/commands/connect.rs`（3 个命令 + `p2p-event` 事件）
  - `src-tauri/src/mgr/connection.rs`（ConnectionManager 状态机）
  - `src-tauri/Cargo.toml` 中的 `wgp-core` 依赖行
- **重接要点**：`wgp-core` 必须放在本仓库上一级的 `Rust/mc-link-core/`；命令为
  `start_p2p_connection` / `stop_p2p_connection` / `get_p2p_status`；事件名 `p2p-event`；
  还需恢复 `mgr/mod.rs`、`commands/mod.rs`、`lib.rs` 中的模块声明、`app.manage(...)` 与命令注册。

### 另一条独立链路：陶瓦适配器（已并入插件系统）

- 陶瓦联机封装**与 WGP 无关**，不要因为"联机断开"而删除。
- 2026-09-30 起已规范化为**内置适配器插件**：`builtin/terracotta.rs` 是唯一入口，
  `adapter.rs` 退居为该插件的运行载体，`terracotta_client.rs` 已删除。
  `commands/adapter.rs` 的 6 个命令签名不变，内部改为经插件管理器派发。
- 设计说明见仓库根目录 `插件系统设计.md`。

### 进行中的方向

- **插件系统后端已落地**（`src-tauri/src/plugin/`）：三类能力（适配/检测/耦合）、
  回环 WebSocket 控制面、权限与加密。检测类与耦合类目前只有接口与路由，尚无内置实现。
- 下一步：前端 UI 大幅重构，含插件管理界面；联机能力在新流程下经插件系统接入。

### 适配器安装的完整性校验（2026-09-30）

- 适配器包是**第三方可执行文件**，安装前必须通过 SHA256 校验。校验清单托管在我们自己的资源服务器：
  `<assets_server>/adapter/manifest.json`。客户端 **fail-closed**——拉不到清单即拒绝安装，
  不会退回无校验下载。
- 清单由 `scripts/sync-assets.mjs` 的 `prepareAdapters()` 生成（下载上游发布包 → 计算 SHA256 →
  写入 `assets-server/Assets/adapter/`），随 `pnpm sync:assets` 上传；资源服务器新增了 `/adapter/*` 路由。
- `adapter/` 目录**不参与** `Assets/manifest.json` 的版本聚合，否则每次适配器升级都会让客户端重下全部字体/图标。
- 升级适配器：改 `sync-assets.mjs` 顶部的 `ADAPTER_VERSION` / `ADAPTER_URL`，再执行 `pnpm sync:assets`。
- 若将来适配器改为随应用内嵌（`tauri.conf.json` 的 `bundle.resources`），应在
  `src-tauri/src/assets/adapter.rs` 的 `fetch_manifest` 中增加"本地常量清单"分支。

## UI 重构期间的约定（2026-09-30 起，进行中）

**当前阶段**：用户正在做前端大面积 UI 重构；这轮**前端不接后端逻辑**，UI 完成后由 AI 负责把
后端能力接上去（含插件系统接线与联机能力恢复）。

### 重构期间保持"跨层契约"不变（改了我需要同步改后端，改动前先告知）

| 契约 | 位置 | 为什么 |
|---|---|---|
| `PersonalizationSettings` 字段名 | 后端 `config/mod.rs` ↔ 前端 `lib/api/types.ts` | 字段名即 yml 的 key，改名等于让老用户配置失效 |
| `lib/persist.ts` 的 `KEYS` 字符串 | `src/lib/persist.ts` | 改名会让老用户 localStorage 变孤儿（表现为设置丢失） |
| 窗口效果取值集合 | `mica` / `acrylic` / `hud_window` / `none` | 后端 `effect::apply_effect_by_name` 按名称匹配 |
| 事件名与负载结构 | `app-log`、`download-progress`、`update-progress`、`p2p-event`、`tray-resize`、`deep-link` | 后端 emit 侧未变，前端改名会监听不到 |
| 更新清单格式与命令名 | `update/latest.json` ↔ `src/lib/api/update.ts` | 见下文"应用更新"章节 |

### 重构期间必须继续遵守的两条硬规则（与本次重构无关，属长期规范）

- `invoke(...)` 只允许出现在 `src/lib/api/**`，页面与组件必须经 api 层。
- 本地存储只经 `src/lib/persist.ts`，不得直接写 `localStorage` / `sessionStorage`。

### 不要删除的东西（用户已明确要求保留）

- 联机页 4 个组件（`components/connect/`）+ `lib/api/connect.ts`：作为 UI 重构素材保留，
  调用会失败属预期（见上文"联机核心"章节）。

### UI 完成后需要 AI 接线的后端能力（后端已实现，前端目前零调用）

| 能力 | 后端状态 | 前端缺口 |
|---|---|---|
| 插件系统 | `src-tauri/src/plugin/**` 完整（8 个命令 + 回环网关 + 权限/加密） | 无 `lib/api/plugin.ts`，无插件管理界面 |
| 联机（P2P） | 已按用户要求**暂时断开** | 联机页保留但调用失败；重接见 `legacy-wgp-p2p-v0.4.0` |
| 检查更新 | ✅ 已完整实现（两种安装形态，见下节） | api + composable 已就绪，**只缺界面** |
| 玩家名 | `Setting/account.yml` 的 `player_name` | 只读不写，无写入入口 |
| 分区设置 | `save_setting` 命令存在 | `connector.yml` / `account.yml` 无设置界面 |

## 应用更新（2026-09-30 实现）

### 两种安装形态（这是本次更新的核心）

| 形态 | 判定 | 更新方式 |
|---|---|---|
| **便携版** | exe 同目录存在 `data/`、`portable.txt` 或 `portable` | 下载单个 exe，**直接替换自身**，随后重启 |
| **安装版** | 其余情况（数据在系统 app_data_dir） | 下载 NSIS 安装器并运行，装完由我们重新拉起应用 |

判定逻辑在 `src-tauri/src/datadir.rs::install_mode()`（与数据目录判定共用同一套约定）。
**不要把安装版包塞给便携版**：`install.rs` 会拒绝"形态与包类型不匹配"的组合，这是刻意的。

### 落地为什么必须由外部进程做

覆盖正在运行的 exe、或运行安装器替换本目录文件，都必须发生在**本进程退出之后**。
实现方式：生成一段 PowerShell，以 `-EncodedCommand`（UTF-16LE + Base64）交给隐藏窗口子进程，
它按 PID 等待本进程退出 → 执行替换/安装 → 重新拉起应用。用 `-EncodedCommand` 而非临时脚本文件，
是为了绕开"路径含中文时的代码页"问题。日志写在 `<data_dir>/Cache/Updates/install.log`。

### 更新清单（`<assets_server>/update/latest.json`）

多资产格式，**同一份清单同时服务两种发行方式**，客户端按 `platform` + `kind` 自选：

| 字段 | 说明 |
|---|---|
| `manifest_version` | 客户端只接受 ≤ 自身支持的版本（当前 1），更高即拒绝并提示手动下载 |
| `assets[].platform` | 形如 `windows-x86_64`，与 `assets::adapter::current_platform()` 一致 |
| `assets[].kind` | `installer` / `portable` / `portable-zip`（仅手动下载） |
| `assets[].sha256` | **必填**：更新包会被直接执行，缺失即拒绝自动安装 |
| `assets[].urls` | 留空时客户端按自身资源服务器地址推导 `update/<file>`，因此清单里不必写死域名 |

### 发布流程

```
pnpm build:release            # 自增版本 + tauri build + rename-build + make-update + sync-assets
pnpm tauri build --mandatory  # 同上，并把清单标记为强制更新
node scripts/make-update.mjs --platform windows-aarch64   # 其他架构，增量合并进同一份清单
```

- 清单与更新包由 `scripts/make-update.mjs` 生成到 `assets-server/Assets/update/`（不进 git），
  更新说明取仓库根目录可选的 `release-notes.md`。
- 资源服务器 `/update/*` 从 `Assets/update/` 读取（与 `POST /upload` 布局一致），并回退旧的 `Updates/` 目录。
- `update/` 与 `adapter/` 一样**不参与** `Assets/manifest.json` 的版本聚合，
  否则每次发版都会让所有客户端重下字体与图标。

### 前端契约

- `src/lib/api/update.ts`：`checkUpdate` / `downloadUpdate` / `installUpdate` / `clearUpdateCache` / `getInstallMode`
- `src/composables/useUpdater.ts`：`check()` → `download()`（可选，带进度）→ `install()`；
  `canAutoInstall` 为 false 时只能引导用户走 `manualUrl`
- 进度事件 `update-progress`，负载 `{ downloaded, total }`（`total` 为 0 表示长度未知）
- `install()` 成功后应用会在约 0.6 秒内退出：**先给用户提示再调用**

### 已知取舍与限制

- **仅 Windows 支持自动安装**。Linux（deb/appimage）与 macOS 返回 `asset: null`，只能手动下载。
  这不是遗漏：deb 需要包管理器与提权，macOS 需要签名校验，自行替换会破坏系统安装记录。
- 便携版更新包用的是**未压缩的 exe**（当前约 17MB），而便携整包 zip 只有约 7MB。
  换成 zip 需要引入解压依赖且要处理多文件覆盖，当前按"简单可靠"取舍；若带宽成为问题可再改。
- `Assets/update/` 下的旧版本包不会自动清理，会随版本累积，确认无客户端在用后可手动删除。
- `downloader/downloader.rs`（分片 + 断点续传下载器）目前**零调用方**：适配器与更新包都走
  `downloader/verified.rs`。保留是因为其续传能力对弱网仍有价值，但按整洁规则需要二选一：
  删除，或把续传并入 `verified`。**此项待用户决定**（见文件头的 `allow(dead_code)` 说明）。

## 持久化约定（2026-09-30 起强制执行）

### 唯一入口

| 层 | 唯一入口 | 谁负责 |
|---|---|---|
| Rust 用户数据 | `src-tauri/src/persist.rs` | 原子写入（tmp+rename）、损坏隔离（改名为 `.corrupt-<时间戳>` 而非覆盖）、失败打印原因 |
| 前端本地存储 | `src/lib/persist.ts` | `KEYS` 集中登记 key；`local` / `session` 命名空间；失败 `console.warn` |

### 禁止事项

- **禁止**在 Rust 侧用 `std::fs::write` 直接写用户数据（Setting/*.yml、Adapter/*.json、
  Plugins/registry.json、插件密钥都必须走 `crate::persist`）。`Assets/` 是可重新下载的
  缓存，仍走 `clear_cache` 语义，不受此约束。
- **禁止**在前端直接使用 `localStorage` / `sessionStorage`，一律经 `lib/persist.ts`。
- **禁止**"解析失败就用默认值覆盖原文件"——必须隔离备份后重建，并打印日志。
  这条是"软件回到全新状态"的已知成因之一。
- **禁止**改动 `KEYS` 里的 key 字符串。改名会让老用户的 localStorage 数据变孤儿
  （表现为设置丢失）；确需改名必须先写迁移。

### 为什么

用户报告过"退出时改动没保存、重启后回到全新状态"。排查确认：个性化保存链路本身正常，
真正的风险来自 ① 多处直接 `std::fs::write` + 静默覆盖（损坏即整体清空且无法取证）、
② 前端存储封装写好却没人用、失败被静默吞掉、`player_name` 甚至只读不写。

### 已知的"未接线"项（不是 bug，重构时一并处理）

- 玩家名（`KEYS.playerName`）目前**没有写入入口**，只有读取，所以改了也不会保存。
- `save_setting` 命令（`conector.yml` / `account.yml` 等分区）前端**无任何调用者**，
  这些分区的设置界面已随 `MetaSettingSection.vue` 一起移除。

## 认证方案（2026-09-30 更新）

### 现状：客户端不含任何登录功能

- **已移除全部认证实现**，包括：
  - 客户端：登录、Token 校验、桌面登录 init/poll、用户信息与头像代理、Token 解密
  - 服务端：`central-server/` 整体移除（该服务将另行重写，不在本仓库）
- **不要重新引入** Token 登录、OAuth 登录、账号会话或任何自建认证服务。
- 历史实现保留在 `legacy-auth-central-v0.4.0` tag，仅用于查阅，不再维护。

### 后续方向

- 账号鉴权将由 **OpenXigoID** 与 **Prism Auth** 实现。
- 现阶段**暂不考虑登录功能**，客户端所有功能均在未登录状态下可用。
- 集成时以上述两者为唯一鉴权入口，不再自建认证服务与账号存储。

## 易被误删的非认证内容

- `Setting/account.yml` 的字段是 `player_name`（玩家显示名），**与认证无关**。
- `Setting/connector.yml` 是 MC 局域网扫描配置（`default_port`、`lan_scan_enabled`），**与 WGP 无关**。
- `deep_link`（`mclink://` 协议）是通用 URL 分发机制，非认证专用，保留待用。
- `uapi-sdk-rust` 用于 `get_ip_info`（网络 IP 归属查询），非认证依赖，保留。

## 历史决策（已作废，仅作溯源）

_以下内容记录于 2026-06-11，其描述的 Token / OAuth 认证架构已随认证实现一并移除，不再适用。_

- 管理面板只保留 Token 登录、移除 OAuth 登录入口。
- 客户端通过独立页面获取 token，方式为：`@qq.com` 邮箱登录，或 OAuth 单点登录
  （GitHub、Microsoft、LittleSkin、MSL 用户中心）。
- 客户端将 token 交给中央服务器，由中央服务器调用账号 API 验证 token 有效性。
- 当时状态：`oauth.rs`、`session.rs` 已实现；管理面板同时支持 Token 与 OAuth，待重构。
