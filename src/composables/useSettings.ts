// 个性化设置的**唯一数据源**。
//
// 后端 `PersonalizationSettings` 的全部字段都在这里持有，界面只通过
// `patch()` 改动。这样做的原因：此前的实现里同一个字段被多个组件各自读一份、
// 各自写一份（`App.vue` 读进 ref、设置页读进另一个对象），于是"改了没反应"
// 或者"退出后没保存"这类问题无法定位。**任何需要读写这些字段的界面都来这里取。**
//
// 字段数量刻意不写死在这里 —— 它随功能增长（模糊/遮罩各拆了深浅两档），
// 写一个数字只会隔三差五就过期。
//
// 配色由 `<m3e-theme>` 的动态配色生成（`scheme` 绑定在 App.vue）；
// 这里只负责明暗状态与 `.dark` 钩子类。

import { computed, reactive, ref } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getPersonalization, savePersonalization } from "../lib/api/config";
import { fetchRemoteBackground } from "../lib/api/datadir";
import { getDefaultEffect, setWindowDarkMode, setWindowEffect } from "../lib/api/effect";
import type { PersonalizationSettings } from "../lib/api/types";
import { applyBackground, isRemoteUrl, resolveMediaUrl } from "../lib/appearance/background";

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
  };
}

// ---- 模块级单例状态 ----
const state = reactive<PersonalizationSettings>(defaultSettings());
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
/**
 * 窗口材质交给原生窗口。**只在材质相关字段真的变化时调用** —— 它是一次 IPC 往返，
 * 挂在每次配色变更上纯属浪费（拖动取色器时每秒几十次）。
 *
 * 染色浓度与明暗一并传：材质是"半透明纯色 + 染色层"，只传效果名的话，
 * 调浓度或切明暗都不会反映到窗口上。
 */
function syncWindowEffect() {
  void setWindowEffect(
    state.transparent_effect || platformDefaultEffect.value,
    state.effect_tint,
    isDark.value,
  ).catch(() => undefined);
}

function applyChrome() {
  const root = document.documentElement;
  root.setAttribute("data-theme-mode", state.theme_mode);
  root.classList.toggle("dark", isDark.value);
  root.classList.toggle("no-animations", !state.animation_enabled);
  root.style.setProperty(
    "--anim-speed",
    String(state.animation_enabled ? state.animation_speed : 1),
  );
  // 让 `color-scheme` 跟着**应用**主题走。
  //
  // 应用主题与系统主题是独立的（`theme_mode` 可以钉死 light/dark），而 `color-scheme`
  // 默认只跟系统。不同步的话，原生控件（滚动条、`<input>` 的默认样式、原生弹窗）
  // 会在"应用深色 + 系统浅色"时保持浅色。此前全站没有声明 `color-scheme`，
  // `base.css` 与这里分别是它的初值与运行时值。
  root.style.colorScheme = isDark.value ? "dark" : "light";
  syncWindowDark(isDark.value);

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

/**
 * 最近一次成功解析的**网络背景**结果。
 *
 * `useColorScheme` 要用其中的 `seed`（后端下载时顺手提取的），所以放模块级共享，
 * 避免为取色再下一次同一张图。
 */
const remoteBg = ref<{ url: string; path: string; seed: string | null } | null>(null);

/** 供 `useColorScheme` 读取：当前网络背景对应的本地文件与种子色。 */
export function remoteBackground() {
  return remoteBg;
}

/**
 * 丢弃已解析的网络背景结果，让下一次 `refreshBackground` 重新下载。
 *
 * 用于"用户点了应用 URL"——同一个 URL 也要能重拉（随机图片 API 的典型用法是
 * 点一次换一张）。刻意**不**通过往设置里加字段来实现：那样会被序列化丢掉，
 * 也会把一次性的界面动作混进持久化契约。
 */
export function forceReloadRemote() {
  remoteBg.value = null;
}

/**
 * 网络背景的"正在获取"状态。
 *
 * 界面（`BackgroundSourceCard` 的「应用」按钮）用它显示转圈。
 */
export const remoteBgLoading = ref(false);

/**
 * 解析背景/音乐地址并应用到 DOM。
 *
 * **网络图的地址必须由后端给出**：前端直接把它交给浏览器的话，后端拿不到字节就无法
 * 取色；而且随机图片 API 每次返回的图不同，会出现"取色用 A 图、显示用 B 图"。
 * 后端下载并缓存后，两边读的是同一个文件。详见 `lib/api/datadir.ts`。
 */
async function refreshBackground() {
  const token = ++bgToken;
  const usesMedia = state.background_type === "image" || state.background_type === "video";

  let bgUrl = "";
  if (usesMedia) {
    if (isRemoteUrl(state.background_value)) {
      // 已经解析过同一个 URL 就直接复用，不再下一次（明暗切换、模糊调整等都会走到这里）。
      if (remoteBg.value?.url === state.background_value) {
        bgUrl = convertFileSrc(remoteBg.value.path);
      } else {
        remoteBgLoading.value = true;
        try {
          const res = await fetchRemoteBackground(
            state.background_value,
            state.background_type === "video",
          );
          if (token !== bgToken) return;
          remoteBg.value = {
            url: state.background_value,
            path: res.path,
            seed: res.seed,
          };
          bgUrl = convertFileSrc(res.path);
        } catch (e) {
          // 下载彻底失败（且没有旧缓存）：不把界面卡在"加载中"，回落成无背景。
          console.warn("[background] 获取网络背景失败:", e);
          remoteBg.value = null;
        } finally {
          remoteBgLoading.value = false;
        }
      }
    } else {
      bgUrl = await resolveMediaUrl(state.background_value);
    }
  }

  // `music_mode === "url"` 时 music_value 本身就是 URL；只有 "file" 才需要换成本地地址。
  const musicUrl =
    state.music_mode === "file" || state.music_mode === "url"
      ? state.music_mode === "url"
        ? state.music_value
        : await resolveMediaUrl(state.music_value)
      : "";

  // 期间用户又改了设置，丢弃这次结果，避免旧值覆盖新值。
  if (token !== bgToken) return;

  applyBackground(state, bgUrl, musicUrl, isDark.value);
}

/** 把当前状态整体落到 DOM（外观 + 材质 + 背景）。启动与系统主题变化时用。 */
async function applyAll() {
  applyChrome();
  syncWindowEffect();
  await refreshBackground();
}

/**
 * 只有这些字段变化才需要重解析背景与背景音乐（每次都可能是一次 IPC）。
 *
 * `transparent_effect` 在这里是因为它决定了窗口材质，而材质与"背景不透明度"是**互斥的
 * 两种外观判断依据**（同一个 default 背景，底下垫的是系统材质还是桌面）。
 * 切材质后不重算，`--app-opacity` 就不会被重新下发。
 *
 * 新增「分深浅两档」的调节字段必须同时加进这里：否则切换明暗后不会重算，
 * 界面会停在换档前的值上，看起来像"深色档没保存"。
 */
const BACKGROUND_KEYS = new Set<string>([
  "transparent_effect",
  "background_type",
  "background_value",
  // 开关本身不改变背景的渲染，但它是"配色跟随背景图"的输入之一：
  // 放进这里，打开/关闭后 `applyAll` 那条链路会把状态落一遍 DOM。
  "theme_from_background",
  "background_fit",
  "background_overlay",
  "background_overlay_opacity",
  "background_overlay_opacity_dark",
  "background_opacity",
  "background_opacity_dark",
  "background_image_blur_light",
  "background_image_blur_dark",
  "background_video_blur_light",
  "background_video_blur_dark",
  "seed_blur_light",
  "seed_blur_dark",
  "music_mode",
  "music_value",
]);

/**
 * 材质相关的字段：改动它们要走一次 `set_window_effect` IPC。
 *
 * `effect_tint` 在这里而不是在 `BACKGROUND_KEYS`：它改的是原生窗口的材质染色，
 * 必须真的调一次官方 API 才生效——光刷新背景层会看到"滑块动了但窗口没变"。
 */
const EFFECT_KEYS = new Set<string>(["transparent_effect", "effect_tint"]);

/**
 * 需要重算背景/模糊的字段（不含材质 IPC）。
 *
 * 明暗翻转（`theme_mode`）也要算在内：模糊强度、遮罩各有深浅两档，
 * 不重算就会停在换档前的值上。系统主题变化走的是 `applyAll()`，不经过这里。
 */
const NEEDS_BACKGROUND = new Set<string>([...BACKGROUND_KEYS, "theme_mode"]);

/**
 * 只应用 `partial` 里真正相关的部分。
 *
 * 配色字段只影响 `applyChrome()`（写几个 class 与 CSS 变量，无 IPC）；窗口材质与背景
 * 各是一次 IPC，不该被拖色事件顺带触发 —— 那是纯粹白花的往返。
 */
function applyChanged(partial: Partial<PersonalizationSettings>) {
  applyChrome();
  const keys = Object.keys(partial);
  if (keys.some((key) => EFFECT_KEYS.has(key))) syncWindowEffect();
  if (keys.some((key) => NEEDS_BACKGROUND.has(key))) void refreshBackground();
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
    loadFailed = false;
    saveError.value = null;
  } catch (e) {
    // 读失败必须锁住写入：否则下一次 patch() 的防抖保存会把默认值整体覆盖用户配置。
    loadFailed = true;
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
  applyChanged(partial);
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
  // 读失败后写入等于用默认值覆盖用户配置：宁可报错也不落盘（见 loadFailed 的说明）。
  if (loadFailed) {
    saveError.value = "设置未能读取，已暂停保存以避免覆盖原有配置；请重启应用";
    return false;
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
