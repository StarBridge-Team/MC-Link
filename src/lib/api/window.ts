import { invoke } from "@tauri-apps/api/core";

/**
 * 窗口相关的 IPC 封装。
 *
 * 这几个命令原先由 `composables/useWindowControls.ts` 直接 `invoke`，
 * 违反"IPC 只允许出现在 `src/lib/api/**`"的约定：命令一旦改名，
 * 散落在组件里的调用点必然漏改（而且不会有编译错误，只在运行时炸）。
 *
 * 注意：最小化 / 最大化 / 移动用的是 `@tauri-apps/api/window` 的窗口方法，
 * 不属于自定义命令，因此不在这里再包一层。
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

/** 关闭窗口。 */
export async function closeWindow() {
  return invoke<void>("close_window");
}
