// 个性化设置的**唯一数据源**。
//
// 后端 `PersonalizationSettings` 的 14 个字段全部在这里持有，界面只通过
// `patch()` 改动。这样做的原因：此前的实现里同一个字段被多个组件各自读一份、
// 各自写一份（`App.vue` 读进 ref、设置页读进另一个对象），于是"改了没反应"
// 或者"退出后没保存"这类问题无法定位。**任何需要读写这些字段的界面都来这里取。**
//
// 配色由 `<m3e-theme>` 的动态配色生成（`scheme` 绑定在 App.vue）；
// 这里只负责明暗状态与 `.dark` 钩子类。

import { computed, reactive, ref } from "vue";
import { getPersonalization, savePersonalization } from "../lib/api/config";
import { getDefaultEffect, setWindowDarkMode, setWindowEffect } from "../lib/api/effect";
import type { PersonalizationSettings } from "../lib/api/types";
import { applyBackground, resolveMediaUrl } from "../lib/appearance/background";

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
    background_type: "default",
    background_value: "",
    background_fit: "scale-to-fill",
    background_overlay: false,
    background_overlay_opacity: 30,
    music_mode: "none",
    music_value: "",
    homepage_mode: "default",
    homepage_value: "",
  };
}

// ---- 模块级单例状态 ----
const state = reactive<PersonalizationSettings>(defaultSettings());
const loaded = ref(false);
const saving = ref(false);
const saveError = ref<string | null>(null);

/** 后端返回的"本平台默认窗口材质"，仅用于解释空值。 */
const platformDefaultEffect = ref("none");

const systemPrefersDark =
  typeof window !== "undefined" && window.matchMedia
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

const isDark = computed(() => {
  if (state.theme_mode === "dark") return true;
  if (state.theme_mode === "light") return false;
  return systemPrefersDark?.matches ?? false;
});

/** 存进来的材质名归一化：`transparent` 与 `none` 对后端是同一件事。 */
const effect = computed(() =>
  state.transparent_effect === "transparent" || !state.transparent_effect
    ? "none"
    : state.transparent_effect,
);

let saveTimer: ReturnType<typeof setTimeout> | null = null;
let windowDark: boolean | null = null;
let appliedDark: boolean | null = null;
let themeTransitionTimer: ReturnType<typeof setTimeout> | undefined;

function syncWindowDark(dark: boolean) {
  if (windowDark === dark) return;
  windowDark = dark;
  void setWindowDarkMode(dark).catch(() => undefined);
}

/**
 * 顶栏透明、动画速度、明暗钩子类、窗口材质与 DWM 暗色。
 *
 * 组件库配色由 `<m3e-theme>` 的动态配色负责（`scheme` 绑定在 `App.vue`）；
 * 这里给 `<html>` 切 `.dark` 是为了 `tokens.css` 里字面量兜底段与非 m3e 组件。
 */
function applyChrome() {
  const root = document.documentElement;
  root.setAttribute("data-theme-mode", state.theme_mode);
  root.classList.toggle("dark", isDark.value);
  root.classList.toggle("no-animations", !state.animation_enabled);
  root.style.setProperty(
    "--anim-speed",
    String(state.animation_enabled ? state.animation_speed : 1),
  );
  syncWindowDark(isDark.value);
  void setWindowEffect(state.transparent_effect || platformDefaultEffect.value).catch(
    () => undefined,
  );

  // 主题切换淡入淡出：仅当明暗真的翻转、且动画开启时，临时给 <html> 挂
  // `.theme-transition` 让全站配色平滑过渡，动画结束（跟随「动画速度」）后移除，
  // 避免长期用 `*` 过渡覆盖组件自身 transition。
  if (appliedDark !== null && appliedDark !== isDark.value && state.animation_enabled) {
    const dur = 350 * (state.animation_speed || 1);
    root.classList.add("theme-transition");
    window.clearTimeout(themeTransitionTimer);
    themeTransitionTimer = window.setTimeout(
      () => root.classList.remove("theme-transition"),
      dur + 60,
    );
  }
  appliedDark = isDark.value;
}

let bgToken = 0;

/** 解析背景/音乐地址并应用到 DOM。 */
async function refreshBackground() {
  const token = ++bgToken;
  const usesMedia = state.background_type === "image" || state.background_type === "video";
  const bgUrl = usesMedia ? await resolveMediaUrl(state.background_value) : "";
  // `music_mode === "url"` 时 music_value 本身就是 URL；只有 "file" 才需要换成本地地址。
  const musicUrl =
    state.music_mode === "file" || state.music_mode === "url"
      ? state.music_mode === "url"
        ? state.music_value
        : await resolveMediaUrl(state.music_value)
      : "";

  // 期间用户又改了设置，丢弃这次结果，避免旧值覆盖新值。
  if (token !== bgToken) return;

  applyBackground(state, bgUrl, musicUrl);
}

/** 把当前状态整体落到 DOM（外观 + 背景）。 */
async function applyAll() {
  applyChrome();
  await refreshBackground();
}

/** 合并默认值，兼容老配置缺失字段。 */
function withDefaults(data: Partial<PersonalizationSettings>): PersonalizationSettings {
  return { ...defaultSettings(), ...data };
}

/** 从后端读取设置并应用。读失败时保持当前（默认）值，只记录错误。 */
async function load(): Promise<PersonalizationSettings> {
  try {
    platformDefaultEffect.value = await getDefaultEffect();
  } catch {
    // 取不到就退回 "none"：不应用材质好过应用一个未知材质。
  }
  try {
    const data = await getPersonalization();
    Object.assign(state, withDefaults(data));
    loaded.value = true;
  } catch (e) {
    saveError.value = messageOf(e);
    console.warn("[settings] 读取个性化设置失败，使用默认值", e);
  }
  await applyAll();
  return { ...state };
}

/**
 * 修改设置并立即生效。
 *
 * `options.save === false` 用于"连续拖动滑块"这类高频改动：先只改观感，
 * 松开后再调 `saveNow()`。
 */
function patch(partial: Partial<PersonalizationSettings>, options?: { silent?: boolean }) {
  Object.assign(state, partial);
  void applyAll();
  if (!options?.silent) scheduleSave();
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
  saving.value = true;
  saveError.value = null;
  try {
    await savePersonalization({ ...state });
    return true;
  } catch (e) {
    saveError.value = messageOf(e);
    return false;
  } finally {
    saving.value = false;
  }
}

/** 系统主题变化时，仅在"跟随系统"模式下重算。 */
function bindSystemTheme(onChange: () => void) {
  systemPrefersDark?.addEventListener("change", () => {
    if (state.theme_mode === "system") onChange();
  });
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function useSettings() {
  return {
    // 状态
    state,
    loaded,
    saving,
    saveError,
    platformDefaultEffect,
    // 派生
    isDark,
    effect,
    // 动作
    load,
    patch,
    saveNow,
    applyAll,
    bindSystemTheme,
  };
}
