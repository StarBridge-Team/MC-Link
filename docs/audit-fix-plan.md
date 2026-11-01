# 缺陷修复计划（2026-11-01 审计批次）

来源：2026-11-01 前后端契约 / 内存泄漏 / 异步 / 状态同步 / 平台差异 八维审计。

图例：`- [ ]` 未开始 · `- [~]` 进行中 · `- [x]` 已完成（附提交号）

**提交约定**：一个条目一次提交，格式 `fix(scope): 说明`；只 `git add` 本条目的文件，
禁止 `git add -A`（仓库有 rustfmt-on-save，`src-tauri` 常是脏的）。

---

## 批次 1 · 前端 P0/P1（无决策项）

- [ ] **F1 · D6-1** `confirmAction()` 恒返回 false（弹窗看不到、"重置引导"永不执行）
  `src/composables/useToast.ts`
- [ ] **F2 · D2-5** `disposeIpcBridge()` 不清 `retryTimer`；订阅重试无上限无退避
  `src/ipc/bridge.ts`
- [ ] **F3 · D2-7** `onResized` / `onMoved` 的 UnlistenFn 被丢弃（监听器泄漏）
  `src/composables/useWindowControls.ts`
- [ ] **F4 · D2-8** `bindSystemTheme` 的 matchMedia 监听无法移除
  `src/composables/useSettings.ts`
- [ ] **F5 · D4-3** 设置保存失败零提示（`saveError` / `saving` / `loaded` 无出口）
  `src/views/settings/PersonalizationSection.vue` 等
- [ ] **F6 · D2-6** 3 处 `listen()` 的 await 竞态 → 监听器永久泄漏
  `src/views/HomeView.vue`、`src/views/ConnectView.vue`、`src/views/game/PluginsSection.vue`
- [ ] **F7 · D1-4** `resize_window` 的 `center` 前端可选、后端必填（契约漂移）
  `src/lib/api/window.ts`
- [ ] **F8 · D1-2 / D1-3** `get_terracotta_state` 返回类型说谎；`game_list.port` 可空性说谎
  `src/lib/api/adapter.ts`、`src/lib/api/typesPlugin.ts`、`src/views/game/GamesSection.vue`
- [ ] **F9 · D4-5 / D4-6** 无适配器时输入框打不进字、按钮静默；未 `await setup.loadOnce()` 用错游戏
  `src/composables/useConnect.ts`、`src/components/settings/GeneralCard.vue`

## 批次 2 · 房间会话生命周期（后端为主）

- [ ] **R1 · D4-1 / D2-2** 终态不可回退：轮询把已断开房间复活成"在线"；`run_link` 复活已断开会话
  `src-tauri/src/mgr/room.rs`
- [ ] **R2 · D2-1 / D2-3 / D4-7** 后端 `registry.remove()` 是死代码；前端 `refresh()` 只合并不裁剪（幽灵房间）
  `src-tauri/src/domain/room/registry.rs`、`src-tauri/src/mgr/room.rs`、`src/stores/rooms.ts`

## 批次 3 · 启动期安全与渠道闸门（后端）

- [ ] **S1 · D3-1** 托盘右键在主线程建窗口 → 主线程等主线程（死锁）
  `src-tauri/src/window/widget.rs`、`src-tauri/src/tray.rs`
- [ ] **S2 · D7-2** 托盘构建失败导致整个应用启动失败
  `src-tauri/src/lib.rs`、`src-tauri/src/tray.rs`
- [ ] **S3 · D5-2** `legal.rs` 远端清单文件名未做路径校验（路径遍历）
  `src-tauri/src/legal.rs`、`src-tauri/src/assets/pull/mod.rs`
- [ ] **S4 · D5-3** 数据目录解析失败静默回退 `"."`（密钥/设置落到工作目录）
  `src-tauri/src/datadir.rs`
- [ ] **S5 · D8-1** 官方 updater 插件路径绕过 `build_channel` 闸门
  `src-tauri/src/update/commands.rs`

## 批次 4 · 适配器平台化

- [ ] **T1 · D5-1** 陶瓦可执行文件名硬编码 `terracotta-0.4.2-windows-x86_64.exe` → 非 Windows 永远起不来
  `src-tauri/src/adapter.rs`、`src-tauri/src/assets/adapter.rs`
- [ ] **T2** 发布脚本只发 `windows-x86_64` 一个平台
  `scripts/sync-assets.mjs`

## 批次 5 · 锁与异步阻塞

- [ ] **L1 · D3-2** `AdapterManager` 的 std Mutex 被持锁跨越 12s sleep / 3s 阻塞 HTTP
  `src-tauri/src/adapter.rs`、`src-tauri/src/plugin/builtin/terracotta.rs`
- [ ] **L2 · D3-3** `launch_lock` 跨 await 持有 20s → 所有插件调用被串行化
  `src-tauri/src/plugin/manager/mod.rs`、`src-tauri/src/plugin/manager/provision.rs`
- [ ] **L3 · D3-6** 退出路径主线程 `blocking_lock()` + 阻塞 HTTP
  `src-tauri/src/plugin/manager/mod.rs`
- [ ] **L4 · D3-5** async 上下文里 `Child::wait()` 阻塞 worker
  `src-tauri/src/plugin/manager/provision.rs`、`src-tauri/src/plugin/launcher.rs`

## 批次 6 · 跨窗口状态与事件路由

- [ ] **E1 · D4-2** 跨窗口个性化设置永不同步（`APP_SETTINGS` 事件两端都定义了却从未 emit/listen）
  `src-tauri/src/core/events.rs`、`src-tauri/src/config/write.rs`、`src/ipc/bridge.ts`、`src/composables/useSettings.ts`
- [ ] **E2 · D2-4** 房间事件是广播而非按 sessionId 路由
  `src-tauri/src/domain/room/registry.rs`、`src-tauri/src/bridge/events.rs`
- [ ] **E3 · D2-11** 房间窗口关闭后不断开会话（链路/端口/耦合器不清理）
  `src-tauri/src/window/factory.rs`、`src-tauri/src/commands/window.rs`

## 批次 7 · 便携版 / 安装版

- [ ] **P1 · D8-2** 便携版替换 exe 非原子写；失败不回滚、不提示；`.bak` 被下次启动删
  `src-tauri/src/update/install.rs`
- [ ] **P2 · D8-3** 便携版 asset 白名单运行时补丁失败 → 全站 404，仅一行警告
  `src-tauri/src/lib.rs`
- [ ] **P3 · D8-5** 安装版：检查读 `latest.json`、落地读 `tauri.json`，两份清单可能不同步
  `src-tauri/src/update/commands.rs`、`scripts/make-update.mjs`

## 批次 8 · Linux 平台差异（非决策项）

- [ ] **N1 · D7-3** 托盘面板 Linux 直角且圆角外仍可命中
  `src-tauri/src/window/region.rs`
- [ ] **N2 · D7-4** Wayland 下 `set_position` 失效 → 面板不贴托盘
  `src-tauri/src/window/widget.rs`
- [ ] **N3 · D7-6** Linux 无材质兜底 → 透明失败即白底矩形
  `src-tauri/src/window/widget.rs`、`src-tauri/src/effect.rs`

---

## 待用户决策（完成上述条目后统一询问）

- [ ] **Q1 · D7-1** 托盘只有 `Click` 一条路径且无菜单兜底 → Linux 上托盘面板不可达。修法形态？
- [ ] **Q2 · D4-8** 房间窗口 / 托盘面板不渲染背景图、视频、背景音乐（只有主窗口有载体）。是否统一？
- [ ] **Q3 · D8-4** 便携版被单实例插件顶掉（"复制一份到别处"这一便携核心用法不可用）。是否放行多开？
- [ ] **Q4 · D1-5** 错误码契约是半成品（前端 0 处按 `code` 分支；仅 6/78 命令返回 `{code,message}`）。补齐还是删字段？
- [ ] **Q5** 适配器发布平台范围（桌面 6 个 / 含更多）
