# 缺陷修复计划（2026-11-01 审计批次）

来源：2026-11-01 前后端契约 / 内存泄漏 / 异步 / 状态同步 / 平台差异 八维审计。

图例：`- [ ]` 未开始 · `- [~]` 进行中 · `- [x]` 已完成

**提交约定**：一个条目一次提交，格式 `fix(scope): 说明`；只 `git add` 本条目的文件，
禁止 `git add -A`（仓库有 rustfmt-on-save，`src-tauri` 常是脏的）。

---

## 批次 1 · 前端 P0/P1（无决策项）—— 全部完成

- [x] **F1 · D6-1** `confirmAction()` 改为等 `closed` 事件 + 自动 `hide`
- [x] **F2 · D2-5** `disposeIpcBridge()` 清 `retryTimer`；订阅退避重试（上限 5 次）
- [x] **F3 · D2-7** `onResized` / `onMoved` 存 `UnlistenFn`，卸载时移除
- [x] **F4 · D2-8** `bindSystemTheme` 改为模块级单一监听
- [x] **F5 · D4-3** 设置保存/加载失败转错误提示（`InfoBar`）
- [x] **F6 · D2-6** HomeView / ConnectView / PluginsSection 的 `listen` 加 `disposed` 守卫
- [x] **F7 · D1-4** `resize_window` 的 `center` 改必填
- [x] **F8 · D1-2 / D1-3** 陶瓦状态返回整值；`game_list.port` 可空
- [x] **F9 · D4-5 / D4-6** 无适配器不可建房；`mount()` 先 `await setup.loadOnce()`

## 批次 2 · 房间会话生命周期 —— 完成

- [x] **R1 · D4-1 / D2-2** 终态不可回退：`is_terminal` 守卫轮询与建链写回
- [x] **R2 · D2-1 / D2-3 / D4-7** `registry.remove()` 接通；前端 `refresh()` 全量对账裁幽灵房间

## 批次 3 · 启动期安全与渠道闸门 —— 完成

- [x] **S1 · D3-1** 托盘右键建窗移出主线程（`async_runtime::spawn`）
- [x] **S2 · D7-2** 托盘构建失败不再阻断启动（只打日志）
- [x] **S3 · D5-2** 法务条款清单文件名 path 校验，非法即整份作废
- [x] **S4 · D5-3** 数据目录解析失败即报错（不再回退 `"."`）
- [x] **S5 · D8-1** 官方 updater 插件路径受 `build_channel` 闸门约束

## 批次 4 · 适配器平台化 —— 完成

- [x] **T1 · D5-1** 适配器按平台发现 exe（不再硬编码 windows 产物名）
- [x] **T2** 资源多平台分发（含 arm64 / armv7 归一化）

## 批次 5 · 锁与异步阻塞 —— 完成

- [x] **L1 · D3-2** `AdapterManager` 外层锁改为 `Arc<AdapterManager>`；12s 轮询移出持锁
- [x] **L2 · D3-3** `launch_lock` 只护 spawn，等待握手在锁外
- [x] **L3 · D3-6** 退出路径不再持锁做阻塞 HTTP
- [x] **L4 · D3-5** 子进程 `wait` 改后台线程，不阻塞 worker

## 批次 6 · 跨窗口状态与事件路由 —— 完成

- [x] **E1 · D4-2** 补 `APP_SETTINGS` 事件（后端 emit + 前端 `applyRemotePatch` 不回写）
- [x] **E2 · D2-4** 房间事件按 sessionId 定向投递（main + 该会话房间窗口）
- [x] **E3 · D2-11** 房间窗口关闭即断链（用户拍板：**保持现状，关窗不断链**，未改）

## 批次 7 · 便携版 / 安装版 —— 完成

- [x] **P1 · D8-2** 便携版落地原子化（临时文件 + 回滚 .bak + 成功标记才清 .bak）
- [x] **P2 · D8-3** asset 白名单失败发 `asset-scope-error` 事件（前端 AboutSection 常驻提示）
- [x] **P3 · D8-5** `latest.json` / `tauri.json` 同步生成；平台标签归一化

## 批次 8 · Linux 平台差异 —— 完成

- [x] **N1 · D7-3** 托盘面板 Linux 直角 + 角上误命中注释纠偏
- [x] **N2 · D7-4** Wayland 下定位降级到主窗口下方
- [x] **N3 · D7-6** 材质兜底注释更正（Linux 只靠 transparent）
- [x] 行数回退：`room.rs` 451 / `install.rs` 349（+`install/tests.rs`）/ `useSettings` 拆 `useSettingsPersist`
- [x] **D1-5 错误码映射**：`src/lib/api/errorCodes.ts` + `errorMessage`/`messageOf` 按 code 取中文

---

## 已完成决策（用户拍板）

- [x] **Q1 · D7-1** 托盘可达性：仅文档/注释说明，不改 UI（Wayland 定位降级已就位）
- [x] **Q4 · D1-5** 错误码：补全 `code → 中文` 映射表（覆盖 4 个已带 code 的错误）
- [x] **Q5** 适配器发布平台：桌面 6 平台分发（windows/macos × x64/arm64 + linux × x64/arm64）

## 仍待用户决策（产品方向，需拍板）

- [ ] **Q2 · D4-8** 房间窗口 / 托盘小面板要不要也显示你设置的背景图、背景视频、背景音乐
  （目前只有主窗口显示这些个性化内容）
- [ ] **Q3 · D8-4** 便携版（U 盘 / 复制文件夹那种）要不要允许"多开"
  （当前被单实例限制，只能开一个；便携的核心用法是"复制一份到别处"，多开可能冲突）
