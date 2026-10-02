// 无边框窗口的控制：拖动、最小化、最大化、关闭、退出，以及窗口位置记忆。
//
// 位置记忆的存储键是 `KEYS.windowState`（冻结），因为它是纯前端数据，
// 丢了只是窗口回到默认位置，不会造成功能缺失。

import { onDeactivated, onUnmounted, ref } from "vue";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { closeWindow, dragWindow, exitApp, resizeWindow } from "../lib/api/window";
import { KEYS, local } from "../lib/persist";

interface WindowState {
  x: number;
  y: number;
  width: number;
  height: number;
}

const DEFAULT_SIZE = { width: 960, height: 680, minWidth: 720, minHeight: 520 };

/** 关闭/最小化前播放的退场动画时长，需与 CSS 中 --motion-medium 对齐（0.2s）。 */
const EXIT_MS = 200;

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export function useWindowControls() {
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const isDragging = ref(false);
  /** 退场动画进行中：App 根容器据此播放淡出/缩放，动画结束后再真正执行窗口操作。 */
  const isExiting = ref(false);

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
    await win.onResized(scheduleSave);
    await win.onMoved(scheduleSave);
  }

  function startDrag() {
    void dragWindow().catch((e) => console.warn("[window] 拖动失败:", e));
    isDragging.value = true;
    setTimeout(() => (isDragging.value = false), 200);
  }

  async function handleMinimize() {
    try {
      isExiting.value = true;
      await wait(EXIT_MS);
      await getCurrentWindow().minimize();
    } catch (e) {
      console.warn("[window] 最小化失败:", e);
    } finally {
      isExiting.value = false;
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

  /** 关闭窗口（保留托盘，应用仍在后台运行）。 */
  async function handleClose() {
    await saveWindowState();
    try {
      isExiting.value = true;
      await wait(EXIT_MS);
      await closeWindow();
    } catch (e) {
      console.warn("[window] 关闭窗口失败:", e);
    } finally {
      isExiting.value = false;
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

  onUnmounted(clearTimers);
  onDeactivated(clearTimers);

  return {
    isDragging,
    isExiting,
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
