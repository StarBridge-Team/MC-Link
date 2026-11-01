/**
 * 跟随最大化按钮的矩形，并消费原生覆盖层回传的点击。
 *
 * # 为什么需要它
 *
 * Windows 上贴靠布局只在命中测试返回 `HTMAXBUTTON` 时弹出，而 WebView2 会把
 * `WM_NCHITTEST` 吃在自己手里，主窗口收不到。后端的做法是在按钮矩形上盖一个
 * 不绘制的原生子窗口（`src-tauri/src/window/snap`）。代价是那个矩形里的鼠标
 * 被原生窗口接管：浏览器收不到 `:hover`，也收不到 `@click`。于是：
 *
 * - 位置：这里把按钮矩形上报给后端，覆盖层才能盖准；
 * - 交互：原生侧把 hover / click 回传（`mclink://window`），由 store 记录。
 *
 * # 位置为什么不能只量一次
 *
 * 导航栏收起、窗口缩放、跨屏拖动（DPI 变化）都会让按钮平移。覆盖层一旦对不上，
 * 轻则按钮没有悬停反馈，重则盖住旁边的关闭键 —— 那是最危险的失败方式
 * （用户点关闭却只触发最大化）。所以用 `ResizeObserver` + resize/scroll 跟随，
 * 每次都用 `requestAnimationFrame` 合并，避免一帧内多次 IPC。
 */
import { onMounted, onUnmounted, watch } from "vue";

import { snapSetBounds } from "../lib/api/window";
import { useWindowChromeStore } from "../stores/windowChrome";

/** 小于这个尺寸视为"元素还没布局完"（与后端 `SnapBounds::is_usable` 的下界一致）。 */
const MIN_SIZE = 8;

type Bounds = { x: number; y: number; width: number; height: number };

/**
 * `elementId`：最大化按钮的 DOM id；`onMaximizeToggle`：点击时执行的动作
 * （沿用调用方已有的最大化路径，不在这里另写一份 `maximize()`）。
 */
export function useSnapTracking(elementId: string, onMaximizeToggle: () => void) {
  const chrome = useWindowChromeStore();

  let frame = 0;
  let lastKey = "none";
  let disposed = false;
  const observers: ResizeObserver[] = [];

  function push(bounds: Bounds | null) {
    // `devicePixelRatio` 必须进 key：拖到另一块不同缩放的显示器上时，CSS 矩形
    // 一点没变，只有它会变。不带上就会被这里判成"没变化"，覆盖层留在旧坐标。
    const key = bounds
      ? `${bounds.x},${bounds.y},${bounds.width},${bounds.height}@${window.devicePixelRatio}`
      : "none";
    if (key === lastKey) return;
    lastKey = key;
    // 失败静默：贴靠布局不可用时，最大化键仍然能点（走前端路径）。
    void snapSetBounds(bounds).catch(() => undefined);
  }

  function report() {
    frame = 0;
    if (disposed) return;

    const el = document.getElementById(elementId);
    if (!el) {
      push(null);
      return;
    }

    const rect = el.getBoundingClientRect();
    if (rect.width < MIN_SIZE || rect.height < MIN_SIZE) {
      push(null);
      return;
    }

    push({
      x: Math.round(rect.left),
      y: Math.round(rect.top),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    });
  }

  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(report);
  }

  onMounted(() => {
    report();

    // 元素自身尺寸变化（文字换行、图标字体加载完）+ 根元素尺寸变化（窗口缩放）。
    const observer = new ResizeObserver(schedule);
    const el = document.getElementById(elementId);
    if (el) observer.observe(el);
    observer.observe(document.documentElement);
    observers.push(observer);

    // 位置变化但尺寸不变的两种情况：窗口缩放（内容重排）、页面内滚动容器。
    window.addEventListener("resize", schedule);
    document.addEventListener("scroll", schedule, true);
    window.visualViewport?.addEventListener("resize", schedule);
  });

  onUnmounted(() => {
    disposed = true;
    if (frame) cancelAnimationFrame(frame);
    for (const observer of observers) observer.disconnect();
    observers.length = 0;
    window.removeEventListener("resize", schedule);
    document.removeEventListener("scroll", schedule, true);
    window.visualViewport?.removeEventListener("resize", schedule);
    // 按钮没了就别继续盖着 —— 否则原生窗口会一直吃掉那片区域的鼠标。
    push(null);
  });

  // 原生侧只回传"点了"，具体做什么仍由调用方决定（见 `AppTitleBar`）。
  watch(() => chrome.snapClicks, onMaximizeToggle);
}
