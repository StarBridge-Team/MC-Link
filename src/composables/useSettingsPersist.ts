/**
 * 个性化设置的**落盘**（读盘 / 写盘）部分。
 *
 * # 为什么单独一个文件
 *
 * `useSettings.ts` 同时管着两件事：状态怎么**生效**（配色 / 材质 / 背景）与状态怎么**存**。
 * 两者挤在一个文件里会超过仓库的行数上限（`scripts/check-arch.mjs` 的前端上限 400 行）。
 * 这里只管存取：读配置、防抖写、失败提示。
 *
 * # 状态仍然只有一份
 *
 * 设置本体（`state`）仍在 `useSettings.ts`，两边共享同一个对象，
 * 不会出现"两处各存一份、互相覆盖"。
 *
 * # 依赖方向是单向的
 *
 * 本模块**不 import** `useSettings.ts`（否则两个文件互相 import，模块初始化顺序会互相踩）。
 * 它需要的"状态"与"生效动作"由调用方注入（见 `PersistHost`），
 * 因此依赖只有 `useSettings.ts` → 本模块这一条。
 */
import { ref, type Ref } from "vue";

import { getPersonalization, savePersonalization } from "../lib/api/config";
import { getDefaultEffect } from "../lib/api/effect";
import type { PersonalizationSettings } from "../lib/api/types";
import { showError } from "./useToast";

/** 后端 `config/mod.rs` 的默认值镜像（读不到配置时的兜底）。 */
export function defaultSettings(): PersonalizationSettings {
  return {
    theme_color: "#0066cc",
    theme_mode: "system",
    theme_variant: "tonal-spot",
    theme_contrast: "standard",
    animation_enabled: true,
    animation_speed: 1,
    transparent_effect: "",
    effect_tint: 0,
    background_type: "default",
    background_value: "",
    background_fit: "scale-to-fill",
    background_overlay: false,
    background_overlay_opacity: 30,
    background_opacity: 100,
    background_opacity_dark: 100,
    background_image_blur_light: 0,
    background_image_blur_dark: 0,
    background_video_blur_light: 0,
    background_video_blur_dark: 0,
    seed_blur_light: 0,
    seed_blur_dark: 0,
    background_overlay_opacity_dark: 30,
    music_mode: "none",
    music_value: "",
    homepage_mode: "default",
    homepage_value: "",
    // 与后端默认一致：默认跟随手选的 theme_color。
    theme_from_background: false,
    theme_seed_source: "manual",
    ui_scale: 100,
    music_volume: 60,
  };
}

/** 合并默认值，兼容老配置缺失字段。 */
export function withDefaults(data: Partial<PersonalizationSettings>): PersonalizationSettings {
  const merged = { ...defaultSettings(), ...data };
  // 老配置没有 `theme_seed_source`（后端存的是空串）：按原来的布尔迁移一次。
  //
  // 迁移**只在这里做**：后端存空串表示"这个字段本来不存在"，它要是自己填成
  // `manual`，就再也分不清"用户选过手选"和"字段是空的"了。
  if (!merged.theme_seed_source) {
    merged.theme_seed_source = merged.theme_from_background ? "background" : "manual";
  }
  return merged;
}

/**
 * 落盘需要的外部依赖（由 `useSettings.ts` 注入）。
 *
 * 注入而不是直接 import，是为了避免 `useSettings.ts` ↔ 本模块的循环依赖。
 */
export interface PersistHost {
  /** 唯一状态源：读到的配置写进这里，写盘也从这里取。 */
  getState: () => PersonalizationSettings;
  /** 读盘成功后把状态落到界面（配色 / 材质 / 背景）。 */
  applyAll: () => Promise<void>;
  /** 后端返回的"本平台默认窗口材质"，仅用于解释空值。 */
  setPlatformDefaultEffect: (effect: string) => void;
}

/** 落盘相关的能力（对外形状由 `useSettings()` 保持不变）。 */
export interface SettingsPersist {
  /** 是否已经尝试过读盘（无论成败）。 */
  loaded: Ref<boolean>;
  /** 是否正在写盘。 */
  saving: Ref<boolean>;
  /** 最近一次读写失败的原因。 */
  saveError: Ref<string | null>;
  /** 读盘并应用。 */
  load: () => Promise<PersonalizationSettings>;
  /** 立即写盘。 */
  saveNow: () => Promise<boolean>;
  /** 防抖写盘。 */
  scheduleSave: () => void;
}

/**
 * 创建落盘能力。
 *
 * 只在 `useSettings.ts` 的模块级创建一次，因此状态与行为全局唯一。
 */
export function createSettingsPersist(host: PersistHost): SettingsPersist {
  const loaded = ref(false);
  const saving = ref(false);
  const saveError = ref<string | null>(null);

  /**
   * 初始读取是否失败。
   *
   * 读失败时 `state` 停留在默认值，而 `savePersonalization` 是**全量覆盖**：
   * 此时任何一次 `patch()` 都会把一整套默认值写回后端，静默抹掉用户全部个性化配置。
   * 因此读失败后**禁止写入**，直到下一次 `load()` 成功。
   */
  let loadFailed = false;

  /**
   * "读失败导致拒绝保存"是否已经提示过。
   *
   * 它是**持续状态**而不是瞬时事件：读失败后每次 `patch()` 都会被拒绝，
   * 逐次弹 toast 会变成刷屏。只提示一次，界面另有常驻提示条。
   */
  let loadFailureNotified = false;

  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  /** 从后端读取设置并应用。读失败时保持当前（默认）值，只记录错误。 */
  async function load(): Promise<PersonalizationSettings> {
    try {
      host.setPlatformDefaultEffect(await getDefaultEffect());
    } catch {
      // 取不到就退回 "none"：不应用材质好过应用一个未知材质。
    }
    try {
      const data = await getPersonalization();
      Object.assign(host.getState(), withDefaults(data));
      loadFailed = false;
      loadFailureNotified = false;
      saveError.value = null;
    } catch (e) {
      // 读失败必须锁住写入：否则下一次 patch() 的防抖保存会把默认值整体覆盖用户配置。
      loadFailed = true;
      saveError.value = messageOf(e);
      console.warn("[settings] 读取个性化设置失败，使用默认值", e);
      // 读失败也要有人看得见：界面会一直停在默认值上，用户毫无察觉。
      showError(`设置读取失败：${saveError.value}`);
    } finally {
      // 无论成败都算"尝试过加载"：首页据此不再先渲染默认态再突变。
      loaded.value = true;
    }
    await host.applyAll();
    return { ...host.getState() };
  }

  /** 防抖保存（400ms），避免拖动滑块时把磁盘写满。 */
  function scheduleSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void saveNow(), 400);
  }

  /** 立即写盘。 */
  async function saveNow(): Promise<boolean> {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    // 读失败后写入等于用默认值覆盖用户配置：宁可报错也不落盘（见 loadFailed 的说明）。
    if (loadFailed) {
      saveError.value = "设置未能读取，已暂停保存以避免覆盖原有配置；请重启应用";
      // 这是持续状态，只提示一次，避免每次拖滑块都弹 toast。
      if (!loadFailureNotified) {
        loadFailureNotified = true;
        showError(saveError.value);
      }
      return false;
    }
    saving.value = true;
    saveError.value = null;
    try {
      await savePersonalization({ ...host.getState() });
      return true;
    } catch (e) {
      saveError.value = messageOf(e);
      // 落盘失败必须有人看得见：否则界面照样变（`patch()` 先改内存），
      // 用户以为存上了，重启后回退。
      showError(`设置保存失败：${saveError.value}`);
      return false;
    } finally {
      saving.value = false;
    }
  }

  return { loaded, saving, saveError, load, saveNow, scheduleSave };
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
