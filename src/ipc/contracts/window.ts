/**
 * `mclink://window` 的负载：后端发给**指定窗口**的定向意图（`emit_to`，不广播）。
 *
 * 与 `mclink://app/intent` 的区别：那个是"跨窗口请求"（托盘面板 → 主窗口），
 * 这个是"窗口自身的外壳事件"，接收方就是被指向的那个窗口。
 */

/**
 * 贴靠布局覆盖层回传的意图（Windows 专用，见 `src-tauri/src/window/snap`）。
 *
 * - `snap-hover`：鼠标进入/离开最大化按钮。按钮位置被一个原生子窗口盖住了
 *   （系统只认那个窗口的命中测试），浏览器因此收不到 `:hover`，只能由原生侧通知。
 * - `snap-click`：在按钮上完成了一次点击。最大化/还原**仍由前端执行**，
 *   不在原生侧再实现一份，否则同一个动作会有两套状态机。
 */
export type WindowIntent = (
  | { action: "snap-hover"; hovering: boolean }
  | { action: "snap-click" }
) & {
  /**
   * 目标窗口 label。后端 `emit_to` 已经定向，这里再带一份用于前端二次过滤：
   * 多窗口共用同一个事件主题，任何一处退化成广播都会让悬停态跨窗口串。
   */
  window?: string;
};
