import { invoke } from "@tauri-apps/api/core";

/**
 * 窗口与进程相关的 IPC 封装。
 *
 * 最小化 / 最大化 / 移动用的是 `@tauri-apps/api/window` 的窗口方法，
 * 不属于自定义命令，因此不在这里再包一层。
 *
 * # 「关窗口」与「退出」不是一回事
 *
 * - 主窗口的关闭键 = `hideSelfWindow`：**隐藏到托盘**，进程与房间链路继续；
 *   主窗口本身由后端的 `CloseRequested` 拦截，永远不会被销毁（否则托盘叫不回来）。
 * - `exitApp`：`app.exit(0)`，真正结束进程（适配器与插件关停由后端执行）。
 *
 * 曾经的 `closeWindow`（`close_window` 命令）会先停掉所有适配器再关窗口，
 * 语义是"关主窗口=结束联机"——与"关窗口≠断链路"的设计相反，已删除。
 */

/**
 * 调整窗口尺寸；`center` 为 true 时居中。
 *
 * `center` **必填**：后端 `resize_window` 的签名是 `center: bool`（没有默认值），
 * 写成可选会让"漏传"在类型上合法、在运行时却按 `false` 处理。
 */
export async function resizeWindow(options: {
  width: number;
  height: number;
  minWidth?: number;
  minHeight?: number;
  center: boolean;
}) {
  return invoke<void>("resize_window", options);
}

/** 开始拖动窗口（无边框窗口的自定义标题栏用）。 */
export async function dragWindow() {
  return invoke<void>("drag_window");
}

/** 完全退出应用。 */
export async function exitApp() {
  return invoke<string>("exit_app");
}

/**
 * 关闭**当前窗口**（房间窗口的自绘关闭按钮）。
 *
 * 不走 `@tauri-apps/api/window` 的 `close()`：那需要给每个窗口配 capability，
 * 漏配时按钮会**静默失效**（点了没反应，控制台也没明显报错）。
 */
export async function closeSelfWindow() {
  return invoke<void>("close_self_window");
}

/** 隐藏**当前窗口**（小组件面板收起）。 */
export async function hideSelfWindow() {
  return invoke<void>("hide_self_window");
}

/** 显示并聚焦主窗口（托盘菜单与深链接都会用到）。 */
export async function showMainWindow() {
  return invoke<void>("show_main_window");
}

/**
 * 打开（或唤出）**设置独立窗口**（B7）。
 *
 * 设置已从主窗口的 Tab 抽成独立窗口：主窗口导航点「设置」走这里，不再 `router.push`。
 * 后端是单例窗口（label = `settings`），重复调用只做 show + focus。
 */
export async function openSettingsWindow() {
  return invoke<void>("settings_window_open");
}

/** 窗口边缘方向，与 `commands/window.rs::resize_drag` 的字符串一一对应。 */
export type ResizeEdge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

/**
 * 从窗口边缘发起缩放拖动。
 *
 * 只有 Linux 需要：Windows 的缩放热区由 tao 的命中测试给出。见
 * `ui/WindowResizeEdges.vue`（那里解释了为什么是自定义命令而不是 JS 的
 * `startResizeDragging`）。
 */
export async function resizeDrag(direction: ResizeEdge) {
  return invoke<void>("resize_drag", { direction });
}

/**
 * 上报最大化按钮矩形（CSS 逻辑像素），供 Windows 贴靠布局覆盖层跟随。
 *
 * 传 `null` 表示按钮不在了，覆盖层隐藏。贴靠布局是**增强**功能：调用失败只影响
 * "最大化键不弹贴靠菜单"，不该抛出到 UI，调用方自行吞掉错误。
 */
export async function snapSetBounds(
  bounds: { x: number; y: number; width: number; height: number } | null,
) {
  return invoke<void>("snap_set_bounds", { bounds });
}

// `setTraySize` 已删除：托盘面板的尺寸由后端按行数计算（`window::widget::panel_height`），
// 前端不再参与，`tray-resize` 事件也随之取消。
