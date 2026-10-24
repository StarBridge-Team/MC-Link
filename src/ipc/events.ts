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
