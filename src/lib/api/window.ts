import { invoke } from "@tauri-apps/api/core";

/**
 * 窗口与进程相关的 IPC 封装。
 *
 * 最小化 / 最大化 / 移动用的是 `@tauri-apps/api/window` 的窗口方法，
 * 不属于自定义命令，因此不在这里再包一层。
 *
 * # `closeWindow` 与 `exitApp` 不是一回事
 *
 * - `closeWindow`：只关窗口，托盘仍在，可唤回；
 * - `exitApp`：`app.exit(0)`，真正结束进程（插件关停由后端 `RunEvent::Exit` 执行）。
 */

/** 调整窗口尺寸；`center` 为 true 时居中。 */
export async function resizeWindow(options: {
  width: number;
  height: number;
  minWidth?: number;
  minHeight?: number;
  center?: boolean;
}) {
  return invoke<void>("resize_window", options);
}

/** 开始拖动窗口（无边框窗口的自定义标题栏用）。 */
export async function dragWindow() {
  return invoke<void>("drag_window");
}

/** 关闭窗口（保留托盘）。 */
export async function closeWindow() {
  return invoke<void>("close_window");
}

/** 完全退出应用。 */
export async function exitApp() {
  return invoke<string>("exit_app");
}

/** 显示并聚焦主窗口（托盘菜单与深链接都会用到）。 */
export async function showMainWindow() {
  return invoke<void>("show_main_window");
}

// `setTraySize` 已删除：托盘面板的尺寸由后端按行数计算（`window::widget::panel_height`），
// 前端不再参与，`tray-resize` 事件也随之取消。
