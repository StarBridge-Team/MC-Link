import { computed, ref } from "vue";
import {
  listConnectAdapters,
  startConnectHost,
  joinConnect,
  stopConnect,
  getConnectStatus,
  onConnectEvent,
} from "../lib/api/connect";
import type { ConnectAdapter, ConnectEvent, ConnectStatus, JoinField } from "../lib/api/types";
import { listGames } from "../lib/api/plugin";
import type { GameInfo } from "../lib/api/types";
import { local, KEYS } from "../lib/persist";
import { useSetup } from "./useSetup";

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
const showJoin = ref(false);
const games = ref<GameInfo[]>([]);
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

function resetForm() {
  const next: Record<string, string> = {};
  for (const f of joinFields.value) next[f.key] = "";
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
    resetForm();
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
    showJoin.value = false;
    void refreshStatus();
  } else if (e.stage === "error") {
    busy.value = false;
    mode.value = "idle";
    errorMsg.value = e.message;
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
  resetForm();
  showJoin.value = false;
}

async function startHost() {
  if (!selectedId.value) return;
  busy.value = true;
  errorMsg.value = "";
  try {
    // 房主不需要 join 字段；适配器若需要可经 join_fields 声明，这里仅传空。
    await startConnectHost(selectedId.value, gameId.value, {}, playerName());
  } catch (e) {
    busy.value = false;
    errorMsg.value = errMessage(e);
  }
}

async function join() {
  if (!selectedId.value || !canJoin.value) return;
  busy.value = true;
  errorMsg.value = "";
  try {
    await joinConnect(selectedId.value, gameId.value, { ...form.value }, playerName());
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
  const field =
    joinFields.value.find((f) => f.autofillFromInvite) || joinFields.value[0];
  if (field) form.value = { ...form.value, [field.key]: extracted };
  showJoin.value = true;
  mode.value = "idle";
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
    showJoin,
    logs,
    gameId,
    currentGameName,
    canJoin,
    loadAdapters,
    loadGames,
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
