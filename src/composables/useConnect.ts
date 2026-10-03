import { computed, ref } from "vue";
import {
  listConnectAdapters,
  scanLocalGames as scanLocalGamesApi,
  startConnectHost,
  joinConnect,
  stopConnect,
  getConnectStatus,
  onConnectEvent,
} from "../lib/api/connect";
import type { ConnectAdapter, ConnectEvent, ConnectStatus, JoinField, LocalGame } from "../lib/api/types";
import { listGames } from "../lib/api/plugin";
import type { GameInfo } from "../lib/api/types";
import { local, KEYS } from "../lib/persist";
import { useSetup } from "./useSetup";
import { showError } from "./useToast";

// 模块级单例：联机状态同时被 ConnectView 与深链处理读取，避免多处副本不一致。
const adapters = ref<ConnectAdapter[]>([]);
const selectedId = ref("");
const joinFields = ref<JoinField[]>([]);
const form = ref<Record<string, string>>({});
const mode = ref<"idle" | "connecting" | "connected">("idle");
const role = ref<"host" | "guest" | null>(null);
const roomCode = ref("");
const status = ref<ConnectStatus | null>(null);
const errorMsg = ref("");
const busy = ref(false);
const games = ref<GameInfo[]>([]);
const localGames = ref<LocalGame[]>([]);
/** 房主模式的搜索词（筛本机游戏进程）。 */
const gameQuery = ref("");
const logs = ref<string[]>([]);
let unlisten: (() => void) | null = null;

const setup = useSetup();
const gameId = computed(() => setup.state.value?.game || "minecraft-java");
const currentGameName = computed(
  () => games.value.find((g) => g.id === gameId.value)?.name || gameId.value,
);

const canJoin = computed(() =>
  joinFields.value.every((f) => !f.required || (form.value[f.key] ?? "").trim().length > 0),
);

/**
 * 按当前 `joinFields` 重建表单：**保留已填的值**（按 key 对齐），只丢弃适配器不再声明的字段。
 *
 * 这里不能整体清空。成员页每次切 Tab 都会重新挂载并刷新适配器，清空会把用户刚敲进去的
 * 邀请码抹掉——而邀请码输入框常驻在工具栏上，"值凭空消失"尤其突兀。
 */
function syncFormFields() {
  const next: Record<string, string> = {};
  for (const f of joinFields.value) next[f.key] = form.value[f.key] ?? "";
  form.value = next;
}

function playerName(): string {
  return local.getString(KEYS.playerName) || "Player";
}

function errMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message?: unknown }).message;
    return typeof m === "string" ? m : String(e);
  }
  return String(e);
}

async function loadAdapters() {
  try {
    const res = await listConnectAdapters(gameId.value);
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
    // 保留上一次的结果，不清空。
    //
    // 这个函数现在每 5 秒跑一次，一次失败就把列表清空的话，界面会以"卡片消失又回来"
    // 的频率闪烁；而检测器偶发失败（例如多播端口被占）并不代表游戏已经退出。
    console.warn("[connect] 扫描本地游戏失败", e);
  } finally {
    scanning = false;
  }
}

/** 按搜索词筛已扫到的游戏：名称 / 进程 / 端口任一命中即保留。 */
const filteredLocalGames = computed(() => {
  const q = gameQuery.value.trim().toLowerCase();
  if (!q) return localGames.value;
  return localGames.value.filter((g) =>
    `${g.name} ${g.process} ${g.port}`.toLowerCase().includes(q),
  );
});

/**
 * 扫到了游戏、但被搜索词筛空。
 *
 * 与"一台都没扫到"要分开：后者无论搜索词是什么都该提示去点「重新扫描」，
 * 说成"没有匹配"会把人引到错的方向。
 */
const noMatch = computed(
  () => localGames.value.length > 0 && filteredLocalGames.value.length === 0,
);

// ---------------------------------------------------------------- 跨页进入请求

/**
 * 跨页请求：从首页点「开始联机」进来时该落在哪个 Tab。
 * `ConnectView` 挂载时消费一次并清空，之后 Tab 由用户自己控制。
 */
const requestedTab = ref<"host" | "member" | null>(null);

/**
 * 跨页请求：进来后替用户按下某个进程的「开始联机」。
 *
 * 只带进程名——首页那个事件负载里没有端口（`LocalGameFound`），而联机页的扫描结果
 * 带 `process`，两者来自同一个检测器，按它匹配是可靠的。
 *
 * 带有效期：一直匹配不上（游戏已退出、检测器失灵）就作废，
 * 否则用户几分钟后再进联机页会突然弹出一个属于旧请求的弹窗。
 */
const START_REQUEST_TTL = 20_000;
const pendingStart = ref<{ process: string; until: number } | null>(null);

/** 首页「开始联机」调用：请求落在某个 Tab；给了进程名则同时请求为它开弹窗。 */
function requestEntry(tab: "host" | "member", process?: string) {
  requestedTab.value = tab;
  pendingStart.value = process
    ? { process, until: Date.now() + START_REQUEST_TTL }
    : null;
}

/** 取出仍未过期的待启动进程；过期即清空。 */
function activeStartProcess(): string | null {
  const pending = pendingStart.value;
  if (!pending) return null;
  if (Date.now() > pending.until) {
    pendingStart.value = null;
    return null;
  }
  return pending.process;
}

/** 消费掉待启动请求（已经在扫描结果里匹配到目标时调用）。 */
function clearStartRequest() {
  pendingStart.value = null;
}

/** 当前所选适配器在房主侧需要的字段。 */
const hostFields = computed<JoinField[]>(
  () => adapters.value.find((a) => a.pluginId === selectedId.value)?.hostFields || [],
);

/**
 * 适配器声明的「可由邀请码填入」的字段（未声明时退化为第一个字段）。
 *
 * 该字段由联机页的搜索框负责录入，所以**不再作为表单字段渲染**，
 * 否则同一个值会同时出现两个输入框。
 */
const inviteField = computed<JoinField | undefined>(
  () => joinFields.value.find((f) => f.autofillFromInvite) ?? joinFields.value[0],
);

/**
 * 邀请码输入框的值。读写的都是 `form` 里那一个字段，因此与深层链接共用同一条路径：
 * 粘贴整条邀请链接时，`applyInvite` 会先抽码再写回，输入框随即只显示码。
 */
const inviteValue = computed({
  get: () => (inviteField.value ? form.value[inviteField.value.key] ?? "" : ""),
  set: (value: string) => applyInvite(value),
});

/** 需要以表单字段呈现的字段（邀请码字段由搜索框负责）。 */
const formFields = computed<JoinField[]>(() => {
  const key = inviteField.value?.key;
  return joinFields.value.filter((f) => f.key !== key);
});

async function refreshStatus() {
  if (!selectedId.value) return;
  try {
    status.value = await getConnectStatus(selectedId.value, gameId.value);
  } catch {
    /* ignore */
  }
}

function onEvent(e: ConnectEvent) {
  logs.value = [e.message, ...logs.value].slice(0, 20);
  if (e.stage === "progress") {
    busy.value = true;
    mode.value = "connecting";
    errorMsg.value = "";
  } else if (e.stage === "connected") {
    busy.value = false;
    mode.value = "connected";
    role.value = (e.role as "host" | "guest") || role.value;
    if (e.room_code) roomCode.value = e.room_code;
    void refreshStatus();
  } else if (e.stage === "error") {
    busy.value = false;
    mode.value = "idle";
    errorMsg.value = e.message;
    // 必须弹吐司：连接状态卡只在 `mode !== "idle"` 时渲染，而错误一到就回到 idle，
    // 于是 errorMsg 从来没机会显示——用户看到的是"点了开始联机，什么都没发生"。
    showError(e.message);
  } else if (e.stage === "stopped") {
    busy.value = false;
    mode.value = "idle";
    role.value = null;
    roomCode.value = "";
    status.value = null;
  }
}

async function mount() {
  await loadGames();
  await loadAdapters();
  await refreshStatus();
  if (!unlisten) unlisten = await onConnectEvent(onEvent);
}

function unmount() {
  unlisten?.();
  unlisten = null;
}

function selectAdapter(id: string) {
  selectedId.value = id;
  const a = adapters.value.find((x) => x.pluginId === id);
  joinFields.value = a?.joinFields || [];
  syncFormFields();
}

async function startHost(adapterId: string, fields: Record<string, unknown>) {
  if (!adapterId) return;
  busy.value = true;
  errorMsg.value = "";
  try {
    await startConnectHost(adapterId, gameId.value, fields, playerName());
  } catch (e) {
    busy.value = false;
    errorMsg.value = errMessage(e);
  }
}

async function join(adapterId: string, fields: Record<string, unknown>) {
  if (!adapterId) return;
  busy.value = true;
  errorMsg.value = "";
  try {
    await joinConnect(adapterId, gameId.value, fields, playerName());
  } catch (e) {
    busy.value = false;
    errorMsg.value = errMessage(e);
  }
}

async function stop() {
  if (!selectedId.value) return;
  try {
    await stopConnect(selectedId.value, gameId.value);
  } catch (e) {
    errorMsg.value = errMessage(e);
  }
}

/**
 * 从深链 / 粘贴文本里抽取房间码并填入对应字段。
 * 支持 `mclink://join/<code>`、`?code=<code>` 与纯房间码三种形态。
 */
function applyInvite(code: string) {
  const extracted = extractCode(code);
  if (!extracted) return;
  const field = inviteField.value;
  if (field) form.value = { ...form.value, [field.key]: extracted };
}

function extractCode(text: string): string {
  const t = text.trim();
  const m1 = t.match(/mclink:\/\/join\/([^/?#\s]+)/i);
  if (m1) return decodeURIComponent(m1[1]);
  const m2 = t.match(/[?&]code=([^&?\s]+)/i);
  if (m2) return decodeURIComponent(m2[1]);
  return t;
}

function buildShareLink(): string {
  return `mclink://join/${roomCode.value}`;
}

export function useConnect() {
  return {
    adapters,
    selectedId,
    joinFields,
    form,
    mode,
    role,
    roomCode,
    status,
    errorMsg,
    busy,
    logs,
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
    currentGameName,
    canJoin,
    loadAdapters,
    loadGames,
    scanLocalGames,
    mount,
    unmount,
    selectAdapter,
    startHost,
    join,
    stop,
    applyInvite,
    buildShareLink,
    refreshStatus,
  };
}
