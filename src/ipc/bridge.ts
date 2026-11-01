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

import { useSettings, type AssetScopeError } from "../composables/useSettings";
import { useUpdater, type UpdateInstallError } from "../composables/useUpdater";
import { useIntentStore, type AppIntent } from "../stores/intent";
import { useRoomsStore } from "../stores/rooms";
import { useWindowChromeStore } from "../stores/windowChrome";
import type { PersonalizationSettings } from "../lib/api/types";
import {
  APP_ASSET_SCOPE_ERROR,
  APP_INTENT,
  APP_SETTINGS,
  ROOM_PROGRESS,
  ROOM_STATE,
  UPDATE_INSTALL_ERROR,
  WINDOW,
} from "./events";
import type { RoomProgressEvent, RoomStateEvent } from "./contracts/room";
import type { WindowIntent } from "./contracts/window";

/**
 * `mclink://app/settings` 的负载。
 *
 * 契约见 `src-tauri/src/core/events.rs` 的 `topic::APP_SETTINGS`：`{ rev, patch }`。
 *
 * - `rev` 单调递增：同一窗口内按它去重（重复投递不重复应用）。
 * - `patch` 只含**本次实际改动的字段**，不是整份设置——整份会让每个窗口
 *   把背景与材质重算一遍。
 *
 * `patch` 的 key 是设置项的字段名（snake_case）：那正是 `personalization.yml` 里的
 * key，也是前端 `PersonalizationSettings` 的字段名（**冻结契约**）。
 * camelCase 风格只适用于事件负载的**外层**字段名（`rev` / `patch`）。
 */
interface AppSettingsEvent {
  rev: number;
  patch: Partial<PersonalizationSettings>;
}

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
    // 个性化设置：别的窗口改了主题/背景/明暗/材质，这里立刻跟上。
    // **只应用、不回写**——回写会形成"收到事件 → 写盘 → 发事件"的回环。
    registered.push(
      await listen<AppSettingsEvent>(APP_SETTINGS, (e) =>
        useSettings().applyRemotePatch(e.payload.rev, e.payload.patch),
      ),
    );
    // 上一次自动更新落地失败（便携版替换 exe 失败等）：交给更新流程负责提示与状态。
    // 后端在启动约 3 秒后延迟发出，setup 阶段前端还没注册监听，立即发必被丢掉。
    registered.push(
      await listen<UpdateInstallError>(UPDATE_INSTALL_ERROR, (e) =>
        useUpdater().reportInstallError(e.payload),
      ),
    );
    // 资源（asset）协议白名单没加上：交给设置（外观/背景负责人）常驻提示。
    registered.push(
      await listen<AssetScopeError>(APP_ASSET_SCOPE_ERROR, (e) =>
        useSettings().reportAssetScopeError(e.payload),
      ),
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
