/**
 * 事件桥：**全应用唯一的订阅点**。
 *
 * 组件与 composable 一律不得自己 `listen`：订阅跟着组件生命周期走会漏事件
 * （旧实现里"房间期间没人监听 `stopped`，退出后状态卡住"就是这个原因）。
 *
 * # 挂载时序（顺序不能换）
 *
 * 1. 先 `listen`——堵住"订阅之前事件已经发出"的空窗；
 * 2. 再拉一次全量 `snapshot`——拿权威基线；
 * 3. 之后进入 live，事件按 `rev` / `seq` 去重。
 *
 * 事件在步骤 1 与 2 之间到达时，`apply` 的 `rev` 判断会让它先落地；
 * 步骤 2 的快照 `rev` 更旧，会被同一判断丢弃。两条路径幂等。
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { useIntentStore, type AppIntent } from "../stores/intent";
import { useRoomsStore } from "../stores/rooms";
import { useWindowChromeStore } from "../stores/windowChrome";
import { APP_INTENT, ROOM_PROGRESS, ROOM_STATE, WINDOW } from "./events";
import type { RoomProgressEvent, RoomStateEvent } from "./contracts/room";
import type { WindowIntent } from "./contracts/window";

let installed = false;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
/** 已经重试过几次（用于退避与上限判断）。 */
let retryCount = 0;
let unlisten: UnlistenFn[] = [];

/** 重试上限：超过它就只靠全量快照工作，不再无限重试。 */
const MAX_RETRIES = 5;
/** 首次重试的等待时间；之后按 2 的幂退避。 */
const BASE_RETRY_DELAY = 2000;
/** 退避封顶。 */
const MAX_RETRY_DELAY = 30000;

async function install(): Promise<void> {
  if (installed) return;
  const store = useRoomsStore();
  const registered: UnlistenFn[] = [];
  try {
    registered.push(
      await listen<RoomStateEvent>(ROOM_STATE, (e) => store.apply(e.payload.snapshot)),
    );
    registered.push(
      await listen<RoomProgressEvent>(ROOM_PROGRESS, (e) => store.applyProgress(e.payload)),
    );
    // 跨窗口意图：由消费方（App.vue / ConnectView）决定怎么落地，桥只负责收。
    registered.push(
      await listen<AppIntent>(APP_INTENT, (e) => useIntentStore().push(e.payload)),
    );
    // 窗口外壳的原生事件（贴靠覆盖层的 hover/click）：同样只收，落地在 store。
    registered.push(
      await listen<WindowIntent>(WINDOW, (e) => useWindowChromeStore().apply(e.payload)),
    );
    unlisten = registered;
    installed = true;
    // 装上了就把退避计数清零：下次失败从头开始退避。
    retryCount = 0;
    void store.refresh().catch((e) => console.warn("[ipc] 首次同步失败", e));
  } catch (e) {
    // 部分成功时先退订，避免重试后同一条事件被处理两次。
    for (const off of registered) off();
    console.warn("[ipc] 事件订阅失败，将重试", e);
    // **即使订阅失败也要拉一次全量**：订阅与快照是两条独立路径，
    // 事件不可用时界面至少还能显示"此刻有哪些房间"，而不是一个空列表。
    void store.refresh().catch(() => undefined);
    // 重试有上限：非 Tauri 环境下 `invoke` 永远失败，无限重试就是
    // 每 2 秒一次失败 + 一次重试，永久不停。
    if (retryCount >= MAX_RETRIES) {
      console.warn(
        `[ipc] 事件订阅重试 ${MAX_RETRIES} 次仍未成功，放弃重试（此后仅使用全量快照）`,
      );
      return;
    }
    // 指数退避：2s → 4s → 8s → 16s → 30s（封顶）。
    const delay = Math.min(BASE_RETRY_DELAY * 2 ** retryCount, MAX_RETRY_DELAY);
    retryCount += 1;
    if (retryTimer === null) {
      retryTimer = setTimeout(() => {
        retryTimer = null;
        void install();
      }, delay);
    }
  }
}

/** 安装事件桥。必须在 `app.mount()` 之前调用（见两个入口文件）。 */
export function installIpcBridge(): void {
  void install();
}

/** 卸载（仅测试与热重载需要）。 */
export function disposeIpcBridge(): void {
  // 必须清掉待重试的定时器：否则 `installed` 已复位而旧定时器还活着，
  // HMR 之后会再装一次桥，同一条事件被处理两遍。
  if (retryTimer) {
    clearTimeout(retryTimer);
    retryTimer = null;
  }
  retryCount = 0;
  for (const off of unlisten) off();
  unlisten = [];
  installed = false;
}
