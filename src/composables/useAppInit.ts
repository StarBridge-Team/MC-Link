import { ref } from "vue";
import { getAppVersion, initApp } from "../lib/api/app";
import { setWindowEffect } from "../lib/api/effect";
import { getPersonalization } from "../lib/api/config";
import { adapterStartupInit } from "../lib/api/adapter";
import { applyPersStyle, resolveBgFile } from "./usePersonalization";
import type { PersonalizationSettings } from "../lib/api/types";

export function useAppInit() {
  const appVersion = ref("");
  const resolvedBgUrl = ref("");

  async function initialize(
    onPersonalization: (settings: PersonalizationSettings) => void
  ) {
    try {
      appVersion.value = await getAppVersion();
    } catch {
      /* ignore */
    }

    try {
      const earlyPers = await getPersonalization();
      applyPersStyle(earlyPers);
    } catch {
      /* ignore */
    }

    // 注意：远程资源（字体/图标）的同步与注入已统一挪到 main.ts 挂载之前
    // （`bootstrapAssets`），这里不要再触发一次，否则会重复请求资源清单。

    let data: Awaited<ReturnType<typeof initApp>> | null = null;
    try {
      data = await initApp();
    } catch {
      /* ignore */
    }

    if (data) {
      const settings = data.personalization;
      const defaultEffect = data.default_effect;

      if (!settings.transparent_effect || settings.transparent_effect === "") {
        settings.transparent_effect = defaultEffect;
      }
      if (settings.background_value && !settings.background_value.startsWith("http")) {
        resolvedBgUrl.value = await resolveBgFile(settings.background_value);
      }
      onPersonalization(settings);
      // 始终同步窗口效果：setup_window_effects 启动时会预应用 mica，
      // 若用户设置为 none/transparent 需在此显式卸载
      setWindowEffect(settings.transparent_effect).catch(() => {});
    }

    try {
      await adapterStartupInit();
    } catch {
      /* ignore */
    }
  }

  return { appVersion, resolvedBgUrl, initialize };
}
