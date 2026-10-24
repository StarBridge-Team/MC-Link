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

const canJoin = computed(() =>
  joinFields.value.every((f) => !f.required || (form.value[f.key] ?? "").trim().length > 0),
);

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

const inviteValue = computed({
  get: () => (inviteField.value ? form.value[inviteField.value.key] ?? "" : ""),
  set: (value: string) => applyInvite(value),
});

const formFields = computed<JoinField[]>(() => {
  const key = inviteField.value?.key;
  return joinFields.value.filter((f) => f.key !== key);
});

// ---------------------------------------------------------------- 动作

async function mount() {
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
  if (!adapterId) return;
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
  if (!adapterId) return;
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
  const extracted = extractInviteCode(code);
  if (!extracted) return;
  const field = inviteField.value;
  if (field) form.value = { ...form.value, [field.key]: extracted };
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
