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
import { fetchRemoteBackground } from "../lib/api/datadir";
import { setWindowDarkMode, setWindowEffect } from "../lib/api/effect";
import type { PersonalizationSettings } from "../lib/api/types";
import { applyBackground, isRemoteUrl, resolveMediaUrl } from "../lib/appearance/background";
import { createSettingsPersist, defaultSettings } from "./useSettingsPersist";

/** 保持对外的 `defaultSettings` 导出（落盘模块是唯一拥有者）。 */
export { defaultSettings };

/**
 * `mclink://app/asset-scope-error` 的负载。
 *
 * 事件名与后端保持一致（后端见 `src-tauri/src/lib.rs` 的 `ASSET_SCOPE_ERROR_EVENT`），
 * 后端改了就收不到。
 *
 * - `reason`：人可读的中文原因。
 * - `directory`：受影响的数据目录。
 */
export interface AssetScopeError {
  reason: string;
  directory: string;
}

// ---- 模块级单例状态 ----
const state = reactive<PersonalizationSettings>(defaultSettings());

/**
 * 数据目录没能加入资源（asset）协议白名单（后端启动约 3 秒后延迟发出）。
 *
 * 后果：背景图 / 图标 / 字体可能加载不出来（403/404）。
 * 它是**持续状态**：失败已经发生过了，重启前一直有效，所以放在这里而不只是一次提示。
 * 对应「关于」页的常驻 InfoBar。
 */
const assetScopeError = ref<AssetScopeError | null>(null);

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

/**
 * 落盘（读盘 / 写盘）委托给 `useSettingsPersist`。
 *
 * 状态本体（`state`）仍在本模块，两边共享同一个对象，不会出现"两处各存一份、
 * 互相覆盖"。依赖方向是单向的：`useSettings.ts` → `useSettingsPersist.ts`，
 * 后者需要的"状态"与"生效动作"由这里注入（`PersistHost`），避免循环依赖。
 */
const persist = createSettingsPersist({
  getState: () => state,
  applyAll: () => applyAll(),
  setPlatformDefaultEffect: (value: string) => {
    platformDefaultEffect.value = value;
  },
});

function syncWindowDark(dark: boolean) {
  if (windowDark === dark) return;
  windowDark = dark;
  void setWindowDarkMode(dark).catch(() => undefined);
}

/**
 * 顶栏透明、动画速度、明暗钩子类、窗口材质与 DWM 暗色。
 *
 * 组件库配色由 `<m3e-theme>` 负责（`scheme` 绑定在 `App.vue`）；这里给 `<html>`
 * 切 `.dark` 是为了 `tokens.css` 字面量兜底段与非 m3e 组件。
 *
 * 窗口材质交给原生窗口，**只在材质相关字段真的变化时调用**（一次 IPC 往返，
 * 挂在每次配色变更上纯属浪费）。染色浓度与明暗一并传：材质是"半透明纯色 + 染色层"。
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
 * 解析背景/音乐地址并应用到 DOM。**网络图地址必须由后端给出**（前端直连拿不到字节无法取色，
 * 且随机图片 API 每次图不同会"取色用 A、显示用 B"）。后端缓存后两边读同一文件。
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
 * 这些字段变化才重算背景/背景音乐（每次都可能是一次 IPC）。
 * `transparent_effect` 决定窗口材质，切材质后不重算 `--app-opacity` 不会下发；
 * 新增「分深浅两档」字段必须加进这里，否则切换明暗后停在原值（像"深色档没保存"）。
 */
const BACKGROUND_KEYS = new Set<string>([
  "transparent_effect",
  "background_type",
  "background_value",
  // 种子来源不改变背景的渲染，但它是配色的输入之一：
  // 放进这里，切换后 `applyAll` 那条链路会把状态落一遍 DOM。
  "theme_seed_source",
  // 旧布尔保留：外部若仍直接改它，行为要与来源枚举一致地触发重算。
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

/** 材质相关字段：改动要走一次 `set_window_effect` IPC（`effect_tint` 改染色，必须真调才生效）。 */
const EFFECT_KEYS = new Set<string>(["transparent_effect", "effect_tint"]);

/**
 * 需要重算背景/模糊的字段（不含材质 IPC）。`theme_mode` 翻转也在内：
 * 模糊/遮罩各有深浅两档，不重算会停在原值；系统主题变化走 `applyAll()`，不经过这里。
 */
const NEEDS_BACKGROUND = new Set<string>([...BACKGROUND_KEYS, "theme_mode"]);

/** 只应用 `partial` 里真正相关的部分：配色走 `applyChrome`，材质/背景各是一次 IPC，不拖色事件顺带触发。 */
function applyChanged(partial: Partial<PersonalizationSettings>) {
  applyChrome();
  const keys = Object.keys(partial);
  if (keys.some((key) => EFFECT_KEYS.has(key))) syncWindowEffect();
  if (keys.some((key) => NEEDS_BACKGROUND.has(key))) void refreshBackground();
}

let windowDark: boolean | null = null;
let appliedDark: boolean | null = null;
let themeTransitionTimer: ReturnType<typeof setTimeout> | undefined;

/**
 * 最近一次应用的远端设置版本号：同一条事件可能重复投递，按 `rev` 去重
 * （这是远端版本号，与本地 `patch()` 无关）。
 */
let lastRemoteRev = 0;

/**
 * 应用别的窗口推过来的设置变更（`mclink://app/settings`）。
 *
 * **绝不能**回写：`patch()` 会触发保存，回写会形成"事件→写盘→事件"回环，
 * 权威状态在 Rust，前端只是投影。合并 `patch`（`Partial`，只含本次改动字段）后
 * 照常走外观/背景/材质链路；值已一致则跳过，避免重复重算。按 `rev` 去重。
 */
function applyRemotePatch(rev: number, patch: Partial<PersonalizationSettings>): void {
  if (rev <= lastRemoteRev) return;
  lastRemoteRev = rev;

  const effective: Partial<PersonalizationSettings> = {};
  let changed = false;
  for (const [key, value] of Object.entries(patch)) {
    if (state[key as keyof PersonalizationSettings] === value) continue;
    Object.assign(effective, { [key]: value });
    changed = true;
  }
  if (!changed) return;

  // 合并进状态后照常走一遍外观/背景/材质的生效链路（与 `patch()` 同一套）。
  Object.assign(state, effective);
  applyChanged(effective);
}

/**
 * 修改设置并立即生效。
 *
 * `options.save === false` 用于"连续拖动滑块"这类高频改动：先只改观感，
 * 松开后再调 `saveNow()`。
 */
function patch(partial: Partial<PersonalizationSettings>, options?: { silent?: boolean }) {
  Object.assign(state, partial);
  // 来源改了就把旧的布尔一起维护上：降级回旧版本时，它仍能正确判断"是否跟随背景图"。
  // 反过来不自动推导（旧布尔变了不改来源）——升级后的读写只认来源。
  if (partial.theme_seed_source !== undefined) {
    state.theme_from_background = partial.theme_seed_source === "background";
  }
  applyChanged(partial);
  if (!options?.silent) persist.scheduleSave();
}

/**
 * 系统主题变化的回调（模块级，至多一个监听）：`useSettings` 是单例，整个进程只注册一次，
 * 反复调用只换回调不叠加监听。旧实现用内联箭头，每次重挂载多一个监听。
 */
let systemThemeOnChange: (() => void) | null = null;
let systemThemeBound = false;

/** 系统主题变化时，仅在"跟随系统"模式下重算。 */
function bindSystemTheme(onChange: () => void) {
  systemThemeOnChange = onChange;
  if (systemThemeBound || !systemPrefersDark) return;
  systemThemeBound = true;
  systemPrefersDark.addEventListener("change", () => {
    if (state.theme_mode === "system") systemThemeOnChange?.();
  });
}

/**
 * 记录"数据目录没能加入资源协议白名单"，并**常驻**（不弹一次就丢）。
 * 由事件桥转交：后端启动约 3 秒后延迟发出 `mclink://app/asset-scope-error`；
 * 它是持续状态（重启前一直有效），界面据此显示常驻 InfoBar。
 */
function reportAssetScopeError(payload: AssetScopeError): void {
  assetScopeError.value = payload;
}

export function useSettings() {
  return {
    // 状态
    state,
    loaded: persist.loaded,
    saving: persist.saving,
    saveError: persist.saveError,
    assetScopeError,
    platformDefaultEffect,
    // 派生
    isDark,
    effect,
    // 动作
    load: persist.load,
    patch,
    applyRemotePatch,
    saveNow: persist.saveNow,
    applyAll,
    bindSystemTheme,
    /** 记录"资源目录未能加入白名单"（由事件桥转交，见 `reportAssetScopeError`） */
    reportAssetScopeError,
  };
}
