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
