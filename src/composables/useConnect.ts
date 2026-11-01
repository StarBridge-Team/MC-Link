import { computed, ref } from "vue";

import { errorMessage } from "../ipc/client";
import { scanLocalGames as scanLocalGamesApi } from "../lib/api/connect";
import { listGames } from "../lib/api/plugin";
import { roomPrepare } from "../lib/api/room";
import type { ConnectAdapter, GameInfo, JoinField, LocalGame } from "../lib/api/types";
import { extractInviteCode } from "../lib/shareLink";
import { useAccountStore } from "../stores/account";
import { useRoomsStore } from "../stores/rooms";
import { useSetup } from "./useSetup";
import { showError } from "./useToast";

/**
 * 联机页的**表单与选择**状态。
 *
 * # 与 0.5.x 的差别
 *
 * 会话本身不再由这里持有：`mode` / `roomCode` / `role` / `status` 全部删除，
 * 权威状态在 Rust，房间窗口读 `useRoomsStore`。这里只剩"建房/加入之前"的输入，
 * 属于纯 UI 状态，丢了也只是让用户重填，不会造成状态分裂。
 *
 * 模块级单例：`ConnectView` 与它的两个面板、`StartDialog` 共用一份，
 * 切 Tab / 开关弹窗不会丢已填的值。
 */

const adapters = ref<ConnectAdapter[]>([]);
const selectedId = ref("");
const joinFields = ref<JoinField[]>([]);
const form = ref<Record<string, string>>({});
const busy = ref(false);
const games = ref<GameInfo[]>([]);
const localGames = ref<LocalGame[]>([]);
/** 房主模式的搜索词（筛本机游戏进程）。 */
const gameQuery = ref("");

const setup = useSetup();
const rooms = useRoomsStore();
const account = useAccountStore();
const gameId = computed(() => setup.state.value?.game || "minecraft-java");

const canJoin = computed(() => {
  // 必须先有可用适配器：此前 `every` 对空数组恒为 true，"开始联机"按钮永远可点，
  // 但 `join()` 会因 `!adapterId` 静默 return，用户什么都看不到。
  if (!selectedId.value || adapters.value.length === 0) return false;
  return joinFields.value.every(
    (f) => !f.required || (form.value[f.key] ?? "").trim().length > 0,
  );
});

/**
 * 按当前 `joinFields` 重建表单：**保留已填的值**（按 key 对齐）。
 *
 * 不能整体清空：成员页每次切 Tab 都会重新挂载并刷新适配器，清空会把用户刚敲进去的
 * 邀请码抹掉——而邀请码输入框常驻在工具栏上，"值凭空消失"尤其突兀。
 */
function syncFormFields() {
  const next: Record<string, string> = {};
  for (const f of joinFields.value) next[f.key] = form.value[f.key] ?? "";
  form.value = next;
}

/** 展示名来自后端账号（`Setting/account.yml`），不再读 localStorage。 */
function playerName(): string {
  return account.displayName();
}

async function loadAdapters() {
  try {
    // 一次拿齐适配器（含字段声明）与扫描缓存，不再由前端拼两个命令。
    const res = await roomPrepare(gameId.value);
    adapters.value = res.adapters;
    const first = res.adapters.find((a) => a.ready) || res.adapters[0];
    if (first) {
      selectedId.value = first.pluginId;
      joinFields.value = first.joinFields || [];
    } else {
      selectedId.value = "";
      joinFields.value = [];
    }
    syncFormFields();
    // 之前没有适配器时输入的内容留在暂存里：适配器就绪后回填。
    if (inviteDraft.value) applyInvite(inviteDraft.value);
  } catch (e) {
    adapters.value = [];
    console.warn("[connect] 加载适配器失败", e);
  }
}

async function loadGames() {
  try {
    const res = await listGames(undefined);
    games.value = res.games;
  } catch {
    /* 非关键 */
  }
}

/** 扫描是否在途：房主模式每 5 秒轮询一次，扫描可能比间隔还慢，必须挡住叠加调用。 */
let scanning = false;

/** 房主模式：扫描本机游戏实例（检测器插件）。 */
async function scanLocalGames() {
  if (scanning) return;
  scanning = true;
  try {
    localGames.value = await scanLocalGamesApi(gameId.value);
  } catch (e) {
    // 保留上一次的结果：一次失败不代表游戏已退出，清空会让卡片闪烁。
    console.warn("[connect] 扫描本地游戏失败", e);
  } finally {
    scanning = false;
  }
}

const filteredLocalGames = computed(() => {
  const q = gameQuery.value.trim().toLowerCase();
  if (!q) return localGames.value;
  return localGames.value.filter((g) =>
    `${g.name} ${g.process} ${g.port}`.toLowerCase().includes(q),
  );
});

/** 扫到了游戏但被搜索词筛空——与"一台都没扫到"必须区分。 */
const noMatch = computed(
  () => localGames.value.length > 0 && filteredLocalGames.value.length === 0,
);

// ---------------------------------------------------------------- 跨页进入请求

const requestedTab = ref<"host" | "member" | null>(null);

/** 带有效期：一直匹配不上（游戏已退出）就作废，否则几分钟后会弹出旧请求。 */
const START_REQUEST_TTL = 20_000;
const pendingStart = ref<{ process: string; until: number } | null>(null);

function requestEntry(tab: "host" | "member", process?: string) {
  requestedTab.value = tab;
  pendingStart.value = process ? { process, until: Date.now() + START_REQUEST_TTL } : null;
}

function activeStartProcess(): string | null {
  const pending = pendingStart.value;
  if (!pending) return null;
  if (Date.now() > pending.until) {
    pendingStart.value = null;
    return null;
  }
  return pending.process;
}

function clearStartRequest() {
  pendingStart.value = null;
}

// ---------------------------------------------------------------- 字段派生

const hostFields = computed<JoinField[]>(
  () => adapters.value.find((a) => a.pluginId === selectedId.value)?.hostFields || [],
);

/**
 * 适配器声明的「可由邀请码填入」的字段（未声明时退化为第一个字段）。
 * 该字段由联机页的搜索框负责录入，因此不再作为表单字段渲染。
 */
const inviteField = computed<JoinField | undefined>(
  () => joinFields.value.find((f) => f.autofillFromInvite) ?? joinFields.value[0],
);

/**
 * 没有可用适配器时的输入暂存。
 *
 * 此前 `inviteField` 缺失时 getter 恒返回 `""`、setter 直接 return，
 * 输入框敲不进任何字符（用户完全不知道发生了什么）。先把内容留住，
 * 等适配器就绪后再回填。
 */
const inviteDraft = ref("");

/** 已经就"没有适配器"提示过一次：避免每敲一个字弹一次 toast。 */
let warnedNoAdapter = false;

/**
 * 没有可用适配器时的统一提示。
 *
 * 组件层没有 i18n 上下文（`useConnect` 是模块级单例），错误文案直接用中文——
 * 与后端返回的中文错误消息（`errorMessage(e)`）保持一致。
 */
const NO_ADAPTER_HINT = "没有可用的联机适配器：请先在「游戏」页启用适配器插件";

const inviteValue = computed({
  get: () =>
    inviteField.value ? form.value[inviteField.value.key] ?? "" : inviteDraft.value,
  set: (value: string) => applyInvite(value),
});

const formFields = computed<JoinField[]>(() => {
  const key = inviteField.value?.key;
  return joinFields.value.filter((f) => f.key !== key);
});

// ---------------------------------------------------------------- 动作

async function mount() {
  // 引导状态必须先就绪：`gameId` 来自 `setup.state`，
  // 拿不到时只会用默认游戏去拉适配器（拉错游戏的适配器清单）。
  // `App.vue` 的 `setup.load()` 与本页的 `onMounted` 是并发的，这里补一次等待。
  await setup.loadOnce();
  // 账号要在建房之前就绪：`player_name` 是适配器建链时就要用的参数，
  // 晚一步就会以 "Player" 的身份进房间。
  await account.load();
  await loadGames();
  await loadAdapters();
  await scanLocalGames();
}

function selectAdapter(id: string) {
  selectedId.value = id;
  const adapter = adapters.value.find((x) => x.pluginId === id);
  joinFields.value = adapter?.joinFields || [];
  syncFormFields();
}

/** 建房：命令返回即开房间窗口，主窗口立刻回到可继续操作的状态。 */
async function startHost(adapterId: string, fields: Record<string, unknown>) {
  if (!adapterId) {
    // 早退也要有反馈：此前静默 return，用户点了按钮什么都没有。
    showError(NO_ADAPTER_HINT);
    return;
  }
  busy.value = true;
  try {
    await rooms.startHost(adapterId, gameId.value, fields, playerName());
  } catch (e) {
    showError(errorMessage(e));
  } finally {
    busy.value = false;
  }
}

/** 加入房间：同上。 */
async function join(adapterId: string, fields: Record<string, unknown>) {
  if (!adapterId) {
    showError(NO_ADAPTER_HINT);
    return;
  }
  busy.value = true;
  try {
    await rooms.join(adapterId, gameId.value, fields, playerName());
  } catch (e) {
    showError(errorMessage(e));
  } finally {
    busy.value = false;
  }
}

/** 从深链 / 粘贴文本里抽出房间码并填入对应字段。 */
function applyInvite(code: string) {
  const field = inviteField.value;
  if (!field) {
    // 没有字段可填：内容先留住，并说明原因（不再静默吞字符）。
    inviteDraft.value = code;
    if (!warnedNoAdapter) {
      warnedNoAdapter = true;
      showError(NO_ADAPTER_HINT);
    }
    return;
  }
  warnedNoAdapter = false;
  inviteDraft.value = "";
  const extracted = extractInviteCode(code);
  if (!extracted) return;
  form.value = { ...form.value, [field.key]: extracted };
}

export function useConnect() {
  return {
    adapters,
    selectedId,
    joinFields,
    form,
    busy,
    games,
    localGames,
    gameQuery,
    filteredLocalGames,
    noMatch,
    requestedTab,
    pendingStart,
    requestEntry,
    activeStartProcess,
    clearStartRequest,
    hostFields,
    inviteField,
    inviteValue,
    formFields,
    gameId,
    canJoin,
    loadAdapters,
    loadGames,
    scanLocalGames,
    mount,
    selectAdapter,
    startHost,
    join,
    applyInvite,
  };
}
