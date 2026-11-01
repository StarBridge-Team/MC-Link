// 无边框窗口的控制：拖动、最小化、最大化、关闭、退出，以及窗口位置记忆。
//
// # 没有退场动画
//
// 最小化/关闭前**不再**先播一段 CSS 淡出再等 200ms 才调系统 API。理由：
//
// - 最小化本身就有系统动画（DWM 直接对窗口抓帧缩放），前置一段内容淡出等于把一个
//   动作拆成两段，还白白多出 200ms 的迟滞——用户点了没反应的那 200ms 就是这个；
// - 关闭 = 隐藏到托盘，窗口是原地消失，没有"退场过程"可看，淡出期间窗口还停在原处；
// - 房间窗口（`RoomApp.vue`）一直是即时关闭，去掉后两个窗口的行为才一致。
//
// 窗口内部的 CSS 过渡仍然保留在 `App.vue`（`.win-enter-*`）：那是在窗口已经显示
// 之后播放的内容淡入，不延迟任何系统动作。
//
// 位置记忆的存储键是 `KEYS.windowState`（冻结），因为它是纯前端数据，
// 丢了只是窗口回到默认位置，不会造成功能缺失。

import { onDeactivated, onUnmounted } from "vue";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { dragWindow, exitApp, hideSelfWindow, resizeWindow } from "../lib/api/window";
import { KEYS, local } from "../lib/persist";

interface WindowState {
  x: number;
  y: number;
  width: number;
  height: number;
}

const DEFAULT_SIZE = { width: 960, height: 680, minWidth: 720, minHeight: 520 };

export function useWindowControls() {
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * 原生窗口事件（resize / move）的退订句柄。
   *
   * `onResized` / `onMoved` 返回的退订函数**必须留着**：丢掉就等于永久注册，
   * 组件每次重挂载都多一份监听，一次拖动触发 N 次 `scheduleSave`。
   */
  const windowStateUnlisteners: UnlistenFn[] = [];
  /** 组件是否已卸载：`await` 之后要复查，防止卸载后还注册监听。 */
  let disposed = false;

  async function saveWindowState() {
    try {
      const win = getCurrentWindow();
      const pos = await win.outerPosition();
      const size = await win.outerSize();
      if (size.width < 100 || size.height < 100) return;
      local.set(KEYS.windowState, {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
      } satisfies WindowState);
    } catch (e) {
      console.warn("[window] 保存窗口位置失败:", e);
    }
  }

  function scheduleSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void saveWindowState(), 500);
  }

  /** 恢复上次的尺寸与位置；没有记录（或记录已失效）时返回 false。 */
  async function restoreWindowState(): Promise<boolean> {
    const saved = local.get<WindowState | null>(KEYS.windowState, null);
    if (!saved) return false;
    if (saved.width < 100 || saved.height < 100) return false;
    try {
      await resizeWindow({
        width: saved.width,
        height: saved.height,
        minWidth: DEFAULT_SIZE.minWidth,
        minHeight: DEFAULT_SIZE.minHeight,
        center: false,
      });
      await getCurrentWindow().setPosition(new LogicalPosition(saved.x, saved.y));
      return true;
    } catch (e) {
      console.warn("[window] 恢复窗口位置失败:", e);
      return false;
    }
  }

  async function setDefaultWindowSize() {
    await resizeWindow({
      width: DEFAULT_SIZE.width,
      height: DEFAULT_SIZE.height,
      minWidth: DEFAULT_SIZE.minWidth,
      minHeight: DEFAULT_SIZE.minHeight,
      center: true,
    });
    await saveWindowState();
  }

  async function setupWindowStateListeners() {
    const win = getCurrentWindow();
    const offResized = await win.onResized(scheduleSave);
    // `await` 期间组件可能已卸载：此时句柄还没进数组，`onUnmounted` 清不到它，
    // 监听器会永远留着。所以先复查再决定收不收。
    if (disposed) {
      offResized();
      return;
    }
    windowStateUnlisteners.push(offResized);

    const offMoved = await win.onMoved(scheduleSave);
    if (disposed) {
      offMoved();
      return;
    }
    windowStateUnlisteners.push(offMoved);
  }

  function startDrag() {
    void dragWindow().catch((e) => console.warn("[window] 拖动失败:", e));
  }

  /** 直接最小化：系统自带动画，见文件头的「没有退场动画」。 */
  async function handleMinimize() {
    try {
      await getCurrentWindow().minimize();
    } catch (e) {
      console.warn("[window] 最小化失败:", e);
    }
  }

  async function handleMaximize() {
    try {
      const win = getCurrentWindow();
      if (await win.isMaximized()) await win.unmaximize();
      else await win.maximize();
    } catch (e) {
      console.warn("[window] 最大化失败:", e);
    }
  }

  /**
   * 关闭窗口 = **隐藏到托盘**。
   *
   * 不销毁窗口、也不停适配器：房间链路要继续跑，托盘左键还要能把它叫回来。
   * 真正的退出只走 `handleExit`（托盘面板的「退出主程序」）。
   */
  async function handleClose() {
    await saveWindowState();
    try {
      await hideSelfWindow();
    } catch (e) {
      console.warn("[window] 隐藏窗口失败:", e);
    }
  }

  /** 完全退出应用（与 `handleClose` 的区别见 `lib/api/window.ts`）。 */
  async function handleExit() {
    await saveWindowState();
    try {
      await exitApp();
    } catch (e) {
      console.warn("[window] 退出应用失败:", e);
    }
  }

  function clearTimers() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = null;
  }

  /** 摘掉原生窗口监听：句柄在 `setupWindowStateListeners` 里收集。 */
  function removeWindowStateListeners() {
    while (windowStateUnlisteners.length > 0) {
      const off = windowStateUnlisteners.pop();
      try {
        off?.();
      } catch (e) {
        console.warn("[window] 移除窗口事件监听失败:", e);
      }
    }
  }

  function dispose() {
    disposed = true;
    clearTimers();
    removeWindowStateListeners();
  }

  // 只在卸载时摘监听：失活（KeepAlive）后重新激活时仍要能记住窗口位置。
  onUnmounted(dispose);
  onDeactivated(clearTimers);

  return {
    saveWindowState,
    restoreWindowState,
    setDefaultWindowSize,
    setupWindowStateListeners,
    startDrag,
    handleMinimize,
    handleMaximize,
    handleClose,
    handleExit,
  };
}
