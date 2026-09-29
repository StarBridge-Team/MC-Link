# Tasks

## Task 1: 创建 Tauri 命令分层目录结构
将 `commands.rs` 按业务领域拆分为 `commands/` 目录下的多个模块文件。
- [x] SubTask 1.1: 创建 `src-tauri/src/commands/mod.rs`，声明各子模块并重新导出所有命令
- [x] SubTask 1.2: 创建 `commands/game.rs` — 迁移联机相关命令及其辅助函数
- [x] SubTask 1.3: 创建 `commands/window.rs` — 迁移窗口管理命令
- [x] SubTask 1.4: 创建 `commands/relay.rs` — 迁移中继命令
- [x] SubTask 1.5: 创建 `commands/revamp.rs` — 迁移所有 revamp_* 命令
- [x] SubTask 1.6: 创建 `commands/settings.rs` — 迁移设置命令
- [x] SubTask 1.7: 创建 `commands/app.rs` — 迁移应用级命令
- [x] SubTask 1.8: 创建 `commands/adapter.rs` — 迁移适配器命令
- [x] SubTask 1.9: 创建 `commands/account.rs` — 迁移账号命令
- [x] SubTask 1.10: 更新 `src-tauri/src/lib.rs` 中的模块声明和 `generate_handler![]` 引用
- [x] SubTask 1.11: 删除旧的 `commands.rs`，验证编译通过

## Task 2: 统一协议类型定义
将重复的协议类型归并到 `mc-link-common`，消除跨模块重复定义。
- [x] SubTask 2.1: 在 `mc-link-common` 中创建 `types.rs` 模块，定义统一类型（RelayInfo, RoomInfo, PathHop）
- [x] SubTask 2.2: 在 `mc-link-common` 中创建 `tunnel.rs` 模块，定义隧道协议类型（TunnelFrameHeader, PathAssignment, RoomRoute）
- [x] SubTask 2.3: 更新 `src-tauri/src/central.rs`，移除本地 RelayInfo/RoomInfo 定义，引用 mc-link-common 类型
- [x] SubTask 2.4: 更新 `src-tauri/src/client_relay.rs`，移除本地 TunnelFrameHeader/PathHop/PathAssignment/RoomRoute 定义，引用 mc-link-common 类型
- [x] SubTask 2.5: 更新 `mc-link-relay/src/protocol.rs`，移除本地 tunnel 类型定义，引用 mc-link-common 类型
- [x] SubTask 2.6: 更新 `central-server/src/types.rs`，移除与 mc-link-common 重复的类型，引用统一类型
- [x] SubTask 2.7: 将 `pack_packet` / `try_decrypt_response` 从 `src-tauri/protocol.rs` 归并到 mc-link-common
- [x] SubTask 2.8: 将 `hex_to_path_id` 从 `mc-link-relay/protocol.rs` 归并到 mc-link-common

## Task 3: 前端 API 抽象层
建立类型化的前端 API 封装层，所有后端调用通过该层进行。
- [x] SubTask 3.1: 创建 `src/lib/api/types.ts` — 共享类型定义
- [x] SubTask 3.2: 创建 `src/lib/api/game.ts` — 联机 API 封装
- [x] SubTask 3.3: 创建 `src/lib/api/relay.ts` — 中继 API 封装
- [x] SubTask 3.4: 创建 `src/lib/api/revamp.ts` — Revamp API 封装
- [x] SubTask 3.5: 创建 `src/lib/api/settings.ts` — 设置 API 封装
- [x] SubTask 3.6: 创建 `src/lib/api/account.ts` — 账号 API 封装
- [x] SubTask 3.7: 创建 `src/lib/api/adapter.ts` — 适配器 API 封装
- [x] SubTask 3.8: 创建 `src/lib/api/index.ts` — 统一导出
- [x] SubTask 3.9: 更新前端组件，将散落的 `invoke("xxx")` 替换为 API 层调用
- [x] SubTask 3.10: 删除旧的 `src/lib/revamp.ts`

## Task 4: 配置加载统一（选做）
- [x] SubTask 4.1: 创建 `ClientRelayConfig` 结构体在 `src-tauri` 中，使用 `serde_yaml` 反序列化 `client_relay.yml`
- [x] SubTask 4.2: 替换 `commands.rs` 中的 `client_relay_setting_*` 手动解析函数

# Task Dependencies
- [Task 3] 依赖 [Task 1] — 前端 API 层需要后端命令签名稳定
- [Task 4] 依赖 [Task 2] — 配置统一需要协议类型稳定
