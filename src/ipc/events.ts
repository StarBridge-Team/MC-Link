/**
 * 事件主题常量。
 *
 * 必须与 `src-tauri/src/core/events.rs` 的 `topic` 模块逐字一致。
 * 房间相关事件**一律带 `sessionId`**：没有它，多房间时前端无法判断
 * 这条事件属于哪个窗口（这正是旧 `connect-event` 做不到多房间的原因）。
 */
export const ROOM_STATE = "mclink://room/state";
export const ROOM_PROGRESS = "mclink://room/progress";
export const ROOM_LOG = "mclink://room/log";
export const APP_SETTINGS = "mclink://app/settings";
/** 跨窗口意图（托盘面板 → 主窗口），负载 `{ action, process? }`。 */
export const APP_INTENT = "mclink://app/intent";
export const WINDOW = "mclink://window";

/**
 * 上一次自动更新落地失败。
 *
 * 负载 `{ reason: string, target: string }`：`reason` 是人可读的中文原因，
 * `target` 是出问题的目标路径。
 *
 * **事件名与后端保持一致，后端改了就收不到**（后端定义见
 * `src-tauri/src/lib.rs` 的 `UPDATE_INSTALL_ERROR_EVENT`）。
 *
 * 它是**启动后延迟发出**的（约 3 秒后）：setup 阶段前端还没注册监听，
 * 立即发必被丢掉。
 */
export const UPDATE_INSTALL_ERROR = "update-install-error";

/**
 * 数据目录没能加入资源（asset）协议白名单。
 *
 * 负载 `{ reason: string, directory: string }`：`reason` 是人可读的中文原因，
 * `directory` 是受影响的数据目录。
 *
 * **事件名与后端保持一致，后端改了就收不到**（后端定义见
 * `src-tauri/src/lib.rs` 的 `ASSET_SCOPE_ERROR_EVENT`）。
 *
 * 它也是**启动后延迟发出**的（约 3 秒后），理由同上。
 */
export const APP_ASSET_SCOPE_ERROR = "mclink://app/asset-scope-error";
