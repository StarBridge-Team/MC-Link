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
import { APP_INTENT, ROOM_PROGRESS, ROOM_STATE } from "./events";
import type { RoomProgressEvent, RoomStateEvent } from "./contracts/room";

let installed = false;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
let unlisten: UnlistenFn[] = [];

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
    unlisten = registered;
    installed = true;
    void store.refresh().catch((e) => console.warn("[ipc] 首次同步失败", e));
  } catch (e) {
    // 部分成功时先退订，避免重试后同一条事件被处理两次。
    for (const off of registered) off();
    console.warn("[ipc] 事件订阅失败，2 秒后重试", e);
    if (retryTimer === null) {
      retryTimer = setTimeout(() => {
        retryTimer = null;
        void install();
      }, 2000);
    }
  }
}

/** 安装事件桥。必须在 `app.mount()` 之前调用（见两个入口文件）。 */
export function installIpcBridge(): void {
  void install();
}

/** 卸载（仅测试与热重载需要）。 */
export function disposeIpcBridge(): void {
  for (const off of unlisten) off();
  unlisten = [];
  installed = false;
}
