# 接口统一化重构规范

## Why

当前 MC Link 项目存在 4 大接口体系（42 个 Tauri 命令、TCP 二进制协议、HTTP JSON API、自定义加密协议），分布在多个模块中互相调用，导致：

- 同一数据类型（RelayInfo、RoomInfo、PathHop）在 3+ 个文件中重复定义
- 42 个 Tauri 命令全部塞在 `commands.rs`（1387+ 行），无分层设计
- 前后端接口无类型抽象层，前端直接 `invoke("xxx")` 散落在各处
- 两套并行的中继系统（MC Link 原始中继 + Revamp 中继）用不同协议做同一件事
- 两套账号系统（客户端 `account.rs` + 中央服务器 `account_api.rs`）对接不同后端
- 配置加载方式不统一（serde_yaml vs 手动逐行解析）

目标是建立**清晰、可管理、可复用的接口体系**。

## What Changes

### Phase 1: Tauri 命令分层重构
- 将 `commands.rs` 按业务领域拆分为多个命令模块
- 建立 `commands/` 目录，按领域组织命令
- 为每个命令模块定义清晰的输入/输出类型

### Phase 2: 统一协议类型定义
- 将散布在各模块中的共享类型（RelayInfo、RoomInfo、PathHop 等）归并到 `mc-link-common`
- 为 Tauri 命令层、TCP 协议层、HTTP API 层定义统一的接口类型
- 消除重复的加密/协议工具函数

### Phase 3: 前端接口抽象层
- 建立 `src/lib/api/` 目录，为每个后端领域提供类型化的 API 封装
- 前端所有 `invoke()` 调用统一通过 API 层进行
- 废除散落的直接 `invoke("xxx")` 调用

### Phase 4: 服务端接口治理（选做）
- 统一配置加载方式
- 消除两套中继系统的协议重复
- 统一账号 API 对接方式

## Impact

### Affected specs
- Tauri 后端命令系统
- TCP 协议层（mc-link-common / relay / central-server）
- 前端 Vue 组件
- 配置加载机制

### Affected code
| 模块 | 影响范围 | 改动量 |
|------|---------|--------|
| `src-tauri/src/commands.rs` | 拆分为 6+ 文件 | ~1387 行重构 |
| `src-tauri/src/protocol.rs` | 归并到 mc-link-common | ~57 行迁移 |
| `src-tauri/src/central.rs` | 统一类型定义 | ~200 行调整 |
| `src-tauri/src/client_relay.rs` | 移除重复的 TunnelFrameHeader/PathHop | ~300 行精简 |
| `mc-link-relay/src/protocol.rs` | 归并到 mc-link-common | ~100 行迁移 |
| `central-server/src/types.rs` | 统一类型定义 | ~200 行调整 |
| `src/lib/revamp.ts` | 迁移到 API 层 | ~100 行 |
| `src/App.vue` | 改用 API 层调用 | ~50 行 |
| 其他 Vue 组件 | 改用 API 层调用 | ~100 行 |

## ADDED Requirements

### Requirement: Tauri 命令分层架构

系统 SHALL 将 `commands.rs` 按业务领域拆分为独立的模块文件。

#### Scenario: 命令模块划分
- **WHEN** 开发者需要查找/修改某个命令
- **THEN** 命令按以下领域分组：
  - `commands/mod.rs` — 模块声明 + re-exports
  - `commands/game.rs` — 联机相关（start_online, stop_online, ping_relay, get_latency, check_room_full, check_room_exists, get_players）
  - `commands/window.rs` — 窗口管理（minimize, maximize, close, drag, resize, show, exit, tray）
  - `commands/relay.rs` — 中继相关（get_relays, scan_lan_servers, start_client_relay, stop_client_relay）
  - `commands/revamp.rs` — Revamp 中继（revamp_ping, revamp_get_nodes, revamp_get_rooms 等全部 9 个命令）
  - `commands/settings.rs` — 设置相关（get_setting, save_setting, get_personalization, save_personalization, get_default_effect, set_window_effect, get_background_files, get_background_file_url）
  - `commands/app.rs` — 应用级（get_app_version, get_tauri_version, init_app, prepare_app, get_ip_info）
  - `commands/adapter.rs` — 适配器（download_adapter, adapter_startup_init, get_adapter_status, get_terracotta_state, start_terracotta_host, start_terracotta_guest）
  - `commands/account.rs` — 账号（account_login, account_verify, account_ping）

#### Scenario: 命令输入输出类型
- **WHEN** 命令参数或返回值需要修改
- **THEN** 每个命令的输入/输出类型在对应模块文件中定义，禁止全局共享类型定义在 commands.rs 中

### Requirement: 统一协议类型定义

系统 SHALL 提供统一的协议类型定义层，消除重复定义。

#### Scenario: 共享类型归并
- **WHEN** 多个模块使用相同的数据结构
- **THEN** 该类型必须在 `mc-link-common` 中定义，其他模块引用该类型

#### Scenario: 受影响的类型
- `RelayInfo` — 当前在 `central.rs` 和 `central-server/src/types.rs` 中重复定义
- `RoomInfo` — 当前在 `central.rs` 和 `central-server/src/types.rs` 中重复定义
- `PathHop` — 当前在 `client_relay.rs`、`mc-link-relay/protocol.rs`、`central-server/types.rs` 中三处定义
- `TunnelFrameHeader` — 当前在 `client_relay.rs` 和 `mc-link-relay/protocol.rs` 中重复定义
- `PathAssignment` / `RoomRoute` — 当前在 `client_relay.rs` 和 `mc-link-relay/protocol.rs` 中重复定义

#### Scenario: 协议工具函数归并
- **WHEN** 相同的协议处理函数在多个模块中出现
- **THEN** 该函数归并到 `mc-link-common` 中
- 具体影响：`pack_packet` / `try_decrypt_response`（src-tauri 版）→ mc-link-common
- `hex_to_path_id`（mc-link-relay 版）→ mc-link-common

### Requirement: 前端类型化 API 层

系统 SHALL 提供类型化的前端 API 封装层，所有后端调用通过该层进行。

#### Scenario: API 层架构
- **WHEN** 前端组件需要调用后端
- **THEN** 必须通过 `src/lib/api/` 下的封装函数调用，不得直接 `invoke("xxx")`
- API 层目录结构：
  - `src/lib/api/index.ts` — 统一导出
  - `src/lib/api/game.ts` — 联机 API
  - `src/lib/api/relay.ts` — 中继 API
  - `src/lib/api/revamp.ts` — Revamp API（替代现有 revamp.ts）
  - `src/lib/api/settings.ts` — 设置 API
  - `src/lib/api/account.ts` — 账号 API
  - `src/lib/api/adapter.ts` — 适配器 API

#### Scenario: 返回值类型定义
- **WHEN** API 函数返回数据
- **THEN** 必须有完整的 TypeScript 类型定义，禁止使用 `any` 或未经类型化的返回值
- 类型定义在对应 API 模块中，或共享类型在 `src/lib/api/types.ts` 中

## MODIFIED Requirements

### Requirement: Revamp 接口整合
**当前**: 9 个独立 Tauri 命令 `revamp_ping`, `revamp_get_nodes`, `revamp_get_rooms`, `revamp_room_exists`, `revamp_create_room`, `revamp_join_room`, `revamp_leave_room`, `revamp_get_version`, `revamp_start_host`, `revamp_join_room_cmd`
**修改后**: 归入 `commands/revamp.rs` 模块，前端通过 `src/lib/api/revamp.ts` 调用

### Requirement: 配置加载统一
**当前**: 
- `commands.rs` 中手动解析 `client_relay.yml`（逐行扫描 `client_relay_setting_*` 函数）
- `mc-link-relay/src/config.rs` 使用 `serde_yaml::from_str` 完整解析
**修改后**: 所有配置加载统一使用 `serde_yaml` + 结构体反序列化，移除逐行手动解析

## REMOVED Requirements

### Requirement: 旧 account.rs 直接 HTTP 调用
**Reason**: `src-tauri/src/account.rs` 直接对接 `43.240.222.237:8879`，与 `central-server/src/account_api.rs` 功能重叠但 endpoint 不同
**Migration**: 客户端账号操作改为通过中央服务器代理，`account.rs` 简化为调用中央服务器 TCP 命令的客户端

### Requirement: direct invoke("xxx") 模式
**Reason**: 前端散落的直接 `invoke()` 调用难以维护和类型检查
**Migration**: 所有调用通过 `src/lib/api/` 层代理
