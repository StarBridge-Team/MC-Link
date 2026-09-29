# Checklist

## Phase 1: Tauri 命令分层重构
- [x] `commands/` 目录创建，包含 mod.rs 和所有子模块
- [x] game.rs 包含 start_online、stop_online 等联机命令
- [x] window.rs 包含全部窗口管理命令
- [x] relay.rs 包含中继相关命令
- [x] revamp.rs 包含全部 revamp 命令
- [x] settings.rs 包含全部设置命令
- [x] app.rs 包含应用级命令
- [x] adapter.rs 包含适配器命令
- [x] account.rs 包含账号命令
- [x] lib.rs 正确引用新命令模块，`cargo build` 编译通过
- [x] 旧的 commands.rs 已删除

## Phase 2: 统一协议类型定义
- [x] mc-link-common 包含统一的 RelayInfo、RoomInfo、PathHop 类型
- [x] mc-link-common 包含统一的 TunnelFrameHeader、PathAssignment、RoomRoute
- [x] central.rs 引用 mc-link-common 类型，无本地重复定义
- [x] client_relay.rs 引用 mc-link-common 类型，无本地重复定义
- [x] mc-link-relay/protocol.rs 引用 mc-link-common 类型，无本地重复定义
- [x] central-server/types.rs 引用 mc-link-common 类型，无本地重复定义
- [x] pack_packet / try_decrypt_response 在 mc-link-common 中
- [x] hex_to_path_id 在 mc-link-common 中

## Phase 3: 前端 API 抽象层
- [x] src/lib/api/types.ts 包含完整共享类型
- [x] src/lib/api/game.ts 封装所有联机 invoke 调用
- [x] src/lib/api/relay.ts 封装所有中继 invoke 调用
- [x] src/lib/api/revamp.ts 封装所有 revamp invoke 调用
- [x] src/lib/api/settings.ts 封装所有设置 invoke 调用
- [x] src/lib/api/account.ts 封装所有账号 invoke 调用
- [x] src/lib/api/adapter.ts 封装所有适配器 invoke 调用
- [x] src/lib/api/index.ts 统一导出所有 API
- [x] 所有前端组件已替换为 API 层调用，无直接 invoke("xxx")
- [x] 旧 src/lib/revamp.ts 已删除

## Phase 4: 配置加载统一（选做）
- [x] ClientRelayConfig 结构体使用 serde_yaml 反序列化
- [x] client_relay_setting_* 手动解析函数已移除
