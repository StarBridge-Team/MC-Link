import { ref, onUnmounted, onDeactivated } from "vue";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { closeWindow, dragWindow, resizeWindow } from "../lib/api/window";
import { local, KEYS } from "../lib/persist";

interface WindowState {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function useWindowControls() {
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  const isDragging = ref(false);

  async function saveWindowState() {
    try {
      const window = getCurrentWindow();
      const pos = await window.outerPosition();
      const size = await window.outerSize();
      if (size.width < 100 || size.height < 100) return;
      local.set(KEYS.windowState, {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
      } satisfies WindowState);
    } catch (e) {
      console.warn("[窗口] 保存窗口位置失败:", e);
    }
  }

  function debouncedSaveState() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveWindowState, 500);
  }

  async function restoreWindowState() {
    const s = local.get<WindowState | null>(KEYS.windowState, null);
    if (!s) return false;
    try {
      if (s.width < 100 || s.height < 100) return false;
      await resizeWindow({
        width: s.width,
        height: s.height,
        minWidth: 700,
        minHeight: 500,
        center: false,
      });
      const window = getCurrentWindow();
      await window.setPosition(new LogicalPosition(s.x, s.y));
      return true;
    } catch (e) {
      console.warn("[窗口] 恢复窗口位置失败:", e);
      return false;
    }
  }

  async function setDefaultWindowSize() {
    await resizeWindow({
      width: 900,
      height: 650,
      minWidth: 700,
      minHeight: 500,
      center: true,
    });
    await saveWindowState();
  }

  async function setupWindowStateListeners() {
    const window = getCurrentWindow();
    await window.onResized(debouncedSaveState);
    await window.onMoved(debouncedSaveState);
  }

  function startDrag() {
    // 拖动失败不影响后续交互，只记录
    void dragWindow().catch((e) => console.warn("[窗口] 拖动失败:", e));
    isDragging.value = true;
    setTimeout(() => (isDragging.value = false), 200);
  }

  async function handleMinimize() {
    try {
      const window = await getCurrentWindow();
      await window.minimize();
    } catch (error) {
      console.error("Minimize error:", error);
    }
  }

  async function handleMaximize() {
    try {
      const window = await getCurrentWindow();
      if (await window.isMaximized()) {
        await window.unmaximize();
      } else {
        await window.maximize();
      }
    } catch (error) {
      console.error("Maximize error:", error);
    }
  }

  async function handleClose() {
    await saveWindowState();
    await closeWindow();
  }

  function clearTimers() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = null;
  }

  onUnmounted(clearTimers);
  onDeactivated(clearTimers);

  return {
    isDragging,
    saveWindowState,
    restoreWindowState,
    setDefaultWindowSize,
    setupWindowStateListeners,
    startDrag,
    handleMinimize,
    handleMaximize,
    handleClose,
  };
}
