// 首次启动引导（OOBE）与语言/地区。
//
// # 模块级单例
//
// 状态定义在模块作用域，`useSetup()` 只是取同一份引用。这不是风格偏好：
// 引导状态同时被 `App.vue`（决定要不要盖浮层）、引导浮层本身、通用设置页与关于页
// 读取，如果每次调用都新建一份 ref，各处看到的就是**各自独立的副本**——
// 表现为"浮层里语言/地区是空的、下一步按钮永远点不动"。同一个数据只有一个来源。
//
// # 步骤由状态推导，不存"当前第几步"
//
// 后端返回 `steps: { language, eula, game }`，界面据此决定从哪一步继续。
// 每一步都**独立落盘**，所以中途关掉应用，下次启动会接着上次的进度。
//
// # 三步之间是"闸门"关系
//
// `complete_setup_command` 要求：语言已选 + EULA 已同意 + 首个游戏已选，
// 缺一即报错且**不写 `completed`**。因此界面不能跳过中间步骤直接"完成"。
//
// # 拿不到状态时不误弹引导
//
// `completed` 初值为 true：读取失败（后端异常等）时宁可进正常界面，
// 也不要把用户困在一个"存不下去"的引导页里。
//
// # 语言切换立刻生效
//
// 用户选完语言马上调 i18n 的 `setLocale`，不必等走完引导。

import { computed, ref } from "vue";
import { listGames } from "../lib/api/plugin";
import { completeSetup, getSetupState, resetSetup, updateSetup } from "../lib/api/setup";
import { acceptEula, fetchLegal, setFirstGame } from "../lib/api/legal";
import type { GameInfo, LegalBundle, SetupState } from "../lib/api/types";
import { setLocale } from "../i18n";
import { messageOf } from "./useToast";

export type OobeStep = "language" | "eula" | "game";

// ---- 模块级单例状态 ----
const loading = ref(false);
const saving = ref(false);
const error = ref<string | null>(null);

const state = ref<SetupState | null>(null);
const legal = ref<LegalBundle | null>(null);
const legalLoading = ref(false);
const games = ref<GameInfo[]>([]);
const gamesLoading = ref(false);

/** 是否已完成引导。初值 true：读不到状态时按"已完成"处理。 */
const completed = ref(true);
const devSkip = ref(false);
let loadedOnce = false;

/** 是否应当展示引导界面。 */
const needsOobe = computed(() => !completed.value && !devSkip.value);

/** 第一个未满足的步骤；全满足时返回 null。 */
const pendingStep = computed<OobeStep | null>(() => {
  const steps = state.value?.steps;
  if (!steps) return "language";
  if (!steps.language) return "language";
  if (!steps.eula) return "eula";
  if (!steps.game) return "game";
  return null;
});

function applyState(next: SetupState) {
  state.value = next;
  completed.value = next.completed;
  devSkip.value = next.dev_skip;
  setLocale(next.language || next.detected_language);
}

/** 读取引导状态。失败时保持"已完成"，只记录错误。 */
async function load(): Promise<SetupState | null> {
  loading.value = true;
  error.value = null;
  try {
    const next = await getSetupState();
    applyState(next);
    loadedOnce = true;
    return next;
  } catch (e) {
    error.value = messageOf(e);
    console.warn("[setup] 读取引导状态失败，按已完成处理", e);
    return null;
  } finally {
    loading.value = false;
  }
}

/** 只在还没读到过状态时加载，避免各页面重复请求。 */
async function loadOnce() {
  if (loadedOnce) return state.value;
  return load();
}

/**
 * 第一步：选择语言与地区。
 *
 * 用 `update_setup_command` 而不是 `complete`：前者只写语言/地区，不动
 * `completed`，所以这一步单独保存不会让引导被"提前完成"。
 */
async function chooseLanguage(language: string, region: string) {
  saving.value = true;
  error.value = null;
  try {
    const next = await updateSetup(language, region);
    applyState(next);
    return next;
  } catch (e) {
    error.value = messageOf(e);
    throw e;
  } finally {
    saving.value = false;
  }
}

/** 进入 EULA 步骤时拉取条款正文（拉不到不算失败，见 `lib/api/legal.ts`）。 */
async function loadLegal(language: string) {
  legalLoading.value = true;
  try {
    legal.value = await fetchLegal(language);
  } catch (e) {
    error.value = messageOf(e);
    legal.value = null;
  } finally {
    legalLoading.value = false;
  }
}

/** 第二步：同意 EULA（版本与哈希由后端裁决）。 */
async function agreeEula(language: string) {
  saving.value = true;
  error.value = null;
  try {
    const next = await acceptEula(language);
    applyState(next);
    return next;
  } catch (e) {
    error.value = messageOf(e);
    throw e;
  } finally {
    saving.value = false;
  }
}

/** 第三步：可选游戏清单（由插件声明派生）。 */
async function loadGames() {
  if (games.value.length > 0) return games.value;
  gamesLoading.value = true;
  try {
    const result = await listGames(undefined);
    games.value = result.games;
    return result.games;
  } catch (e) {
    error.value = messageOf(e);
    return [];
  } finally {
    gamesLoading.value = false;
  }
}

/** 第三步：选定首个游戏。 */
async function chooseGame(gameId: string) {
  saving.value = true;
  error.value = null;
  try {
    const next = await setFirstGame(gameId);
    applyState(next);
    return next;
  } catch (e) {
    error.value = messageOf(e);
    throw e;
  } finally {
    saving.value = false;
  }
}

/** 收尾：三步齐备后置 `completed`。缺步时后端会拒绝，错误可直接展示。 */
async function finish(language: string, region: string) {
  saving.value = true;
  error.value = null;
  try {
    const next = await completeSetup(language, region);
    applyState(next);
    return next;
  } catch (e) {
    error.value = messageOf(e);
    throw e;
  } finally {
    saving.value = false;
  }
}

/** 重置引导（连同意记录与游戏选择一起清），调试引导界面时用。 */
async function reset() {
  const next = await resetSetup();
  applyState(next);
  legal.value = null;
  loadedOnce = true;
  return next;
}

export function useSetup() {
  return {
    loading,
    saving,
    error,
    state,
    legal,
    legalLoading,
    games,
    gamesLoading,
    completed,
    needsOobe,
    pendingStep,
    load,
    loadOnce,
    chooseLanguage,
    loadLegal,
    agreeEula,
    loadGames,
    chooseGame,
    finish,
    reset,
  };
}
