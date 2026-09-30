import { computed, ref } from "vue";
import {
  completeSetup,
  getSetupState,
  resetSetup,
  updateSetup,
} from "../lib/api/setup";
import type { SetupState } from "../lib/api/types";
import { setLocale } from "../i18n";

/**
 * 首次启动引导（OOBE）与语言/地区。
 *
 * # 界面怎么用
 *
 * ```ts
 * const setup = useSetup();
 * await setup.load();
 * if (setup.needsOobe.value) {
 *   // 选项用 setup.supportedLanguages / setup.supportedRegions 渲染，
 *   // 预选用 setup.detectedLanguage / setup.detectedRegion
 *   await setup.finish("zh-CN", "CN");
 * }
 * ```
 *
 * # 拿不到状态时不会误弹引导
 *
 * `completed` 初值为 `true`：读取失败（后端异常等）时宁可进正常界面，
 * 也不要把用户困在一个"存不下去"的引导页里——那会让应用完全不可用。
 *
 * # 语言切换会立刻生效
 *
 * `finish()` / `update()` 成功后直接调用 i18n 的 `setLocale`，
 * 用户选完语言就能看到界面即时切换。
 */
export function useSetup() {
  const loading = ref(false);
  const saving = ref(false);

  /** 是否已完成引导。初值 true：读取失败时按"已完成"处理，避免误弹引导。 */
  const completed = ref(true);
  /** 当前生效的界面语言。 */
  const language = ref("");
  /** 当前生效的地区。 */
  const region = ref("");
  /** 系统检测到的语言（引导预选用）。 */
  const detectedLanguage = ref("");
  /** 系统检测到的地区（引导预选用）。 */
  const detectedRegion = ref("");
  /** 可选语言，由后端给出。 */
  const supportedLanguages = ref<string[]>([]);
  /** 可选地区，由后端给出。 */
  const supportedRegions = ref<string[]>([]);

  const error = ref<string | null>(null);

  /** 是否应当展示首次引导。 */
  const needsOobe = computed(() => !completed.value);

  function applyState(state: SetupState) {
    completed.value = state.completed;
    language.value = state.language;
    region.value = state.region;
    detectedLanguage.value = state.detected_language;
    detectedRegion.value = state.detected_region;
    supportedLanguages.value = state.supported_languages;
    supportedRegions.value = state.supported_regions;
  }

  /** 读取引导状态。失败时保持"已完成"，只记录错误。 */
  async function load() {
    loading.value = true;
    error.value = null;
    try {
      const state = await getSetupState();
      applyState(state);
      return state;
    } catch (e) {
      error.value = messageOf(e);
      console.warn("[setup] 读取引导状态失败，按已完成处理", e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /** 完成首次引导：保存选择并立即切换界面语言。 */
  async function finish(nextLanguage: string, nextRegion: string) {
    saving.value = true;
    error.value = null;
    try {
      const state = await completeSetup(nextLanguage, nextRegion);
      applyState(state);
      setLocale(state.language);
      return state;
    } catch (e) {
      error.value = messageOf(e);
      throw e;
    } finally {
      saving.value = false;
    }
  }

  /** 修改语言/地区（引导之后）：同样会立即切换界面语言。 */
  async function update(nextLanguage: string, nextRegion: string) {
    saving.value = true;
    error.value = null;
    try {
      const state = await updateSetup(nextLanguage, nextRegion);
      applyState(state);
      setLocale(state.language);
      return state;
    } catch (e) {
      error.value = messageOf(e);
      throw e;
    } finally {
      saving.value = false;
    }
  }

  /** 重置引导（下次启动重新走 OOBE），调试引导界面时用。 */
  async function reset() {
    const state = await resetSetup();
    applyState(state);
    return state;
  }

  return {
    // 状态
    loading,
    saving,
    completed,
    language,
    region,
    detectedLanguage,
    detectedRegion,
    supportedLanguages,
    supportedRegions,
    error,
    // 派生
    needsOobe,
    // 动作
    load,
    finish,
    update,
    reset,
  };
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
