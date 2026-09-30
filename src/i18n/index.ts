import { createI18n } from "vue-i18n";
import zhCN from "./locales/zh-CN";
import enUS from "./locales/en-US";

/**
 * 界面语言与 i18n 装配。
 *
 * # 「有哪些语言可选」的唯一来源在后端
 *
 * 可选清单由 `src-tauri/src/setup.rs` 的 `SUPPORTED_LANGUAGES` 决定，界面通过
 * `getSetupState()` 取值，**不要在前端硬编码选项**。本文件只提供文案与切换实例，
 * 因此这里的键必须与后端清单保持一致：新增语言要同时改两处并补一个 locale 文件。
 *
 * # 为什么在挂载前读语言
 *
 * 若先按浏览器语言渲染、拿到后端设置后再切换，用户会看到文案闪一下。
 * 因此启动流程是先 `initI18n()` 再 `mount()`；它自带超时与兜底，不会拖死启动。
 */

export const SUPPORTED_LOCALES = ["zh-CN", "en-US"] as const;
export type AppLocale = (typeof SUPPORTED_LOCALES)[number];

/** 没有对应翻译时的兜底语言。 */
export const FALLBACK_LOCALE: AppLocale = "en-US";

/** 读取语言设置的等待上限（毫秒）：IPC 万一无响应也不能让界面起不来。 */
const LOAD_TIMEOUT_MS = 1500;

/** 把任意语言标识归一化成受支持的一种：`zh`、`zh_CN`、`zh-Hans-CN` → `zh-CN`。 */
export function normalizeLocale(input: string | null | undefined): AppLocale {
  const value = (input ?? "").trim().toLowerCase();
  if (value.startsWith("zh")) return "zh-CN";
  if (value.startsWith("en")) return "en-US";
  return FALLBACK_LOCALE;
}

/** WebView 的语言，作为后端读不到时的兜底。 */
export function browserLocale(): AppLocale {
  if (typeof navigator === "undefined") return FALLBACK_LOCALE;
  return normalizeLocale(navigator.language);
}

export const i18n = createI18n({
  // Composition API 模式（配合 <script setup> 里的 useI18n()）
  legacy: false,
  // 允许模板里直接用 {{ $t("...") }}，省去每个组件都写 useI18n()
  globalInjection: true,
  locale: browserLocale(),
  fallbackLocale: FALLBACK_LOCALE,
  messages: {
    "zh-CN": zhCN,
    "en-US": enUS,
  },
});

/** 当前界面语言。 */
export function currentLocale(): AppLocale {
  return normalizeLocale(String(i18n.global.locale.value));
}

/** 切换界面语言（`legacy: false` 下 `locale` 是 ref）。 */
export function setLocale(locale: string): AppLocale {
  const next = normalizeLocale(locale);
  i18n.global.locale.value = next;
  return next;
}

/**
 * 读取用户保存的语言并应用。
 *
 * 读不到就保持浏览器语言：**语言设置失败不应该阻塞启动**。
 */
export async function applySavedLocale(): Promise<AppLocale> {
  try {
    const { getSetupState } = await import("../lib/api/setup");
    const state = await getSetupState();
    return setLocale(state.language);
  } catch (e) {
    console.warn("[i18n] 读取语言设置失败，沿用浏览器语言", e);
    return currentLocale();
  }
}

/**
 * 启动时初始化：应在 `app.mount()` **之前**调用。
 *
 * 与超时赛跑：后端没响应时按当前（浏览器）语言继续，不让界面卡在启动阶段。
 */
export async function initI18n(): Promise<AppLocale> {
  const timeout = new Promise<AppLocale>((resolve) => {
    setTimeout(() => resolve(currentLocale()), LOAD_TIMEOUT_MS);
  });
  return Promise.race([applySavedLocale(), timeout]);
}
