import { ref, onUnmounted, onDeactivated } from "vue";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

const WINDOW_STATE_KEY = "window_state";

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
      localStorage.setItem(
        WINDOW_STATE_KEY,
        JSON.stringify({ x: pos.x, y: pos.y, width: size.width, height: size.height } satisfies WindowState)
      );
    } catch {
      /* ignore */
    }
  }

  function debouncedSaveState() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveWindowState, 500);
  }

  async function restoreWindowState() {
    const saved = localStorage.getItem(WINDOW_STATE_KEY);
    if (!saved) return false;
    try {
      const s: WindowState = JSON.parse(saved);
      if (s.width < 100 || s.height < 100) return false;
      await invoke("resize_window", { width: s.width, height: s.height, minWidth: 700, minHeight: 500, center: false });
      const window = getCurrentWindow();
      await window.setPosition(new LogicalPosition(s.x, s.y));
      return true;
    } catch {
      return false;
    }
  }

  async function setDefaultWindowSize() {
    await invoke("resize_window", { width: 900, height: 650, minWidth: 700, minHeight: 500, center: true });
    await saveWindowState();
  }

  async function setupWindowStateListeners() {
    const window = getCurrentWindow();
    await window.onResized(debouncedSaveState);
    await window.onMoved(debouncedSaveState);
  }

  function startDrag() {
    invoke("drag_window");
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
    await invoke("close_window");
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
