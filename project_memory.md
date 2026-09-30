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

### 另一条独立链路：陶瓦适配器（保留）

- `adapter.rs` + `terracotta_client.rs` + `commands/adapter.rs` 封装第三方"陶瓦联机" exe
  （本地 HTTP + 端口轮询），**与 WGP 无关**，不要因为"联机断开"而删除。

### 进行中的方向

- 接下来将对 UI 做大幅重构，并引入**插件系统**。联机能力将在该阶段之后重新接入。

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
