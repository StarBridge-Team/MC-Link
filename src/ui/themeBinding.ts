/**
 * 把 `<m3e-theme>` 与后端配色接起来的公共绑定。
 *
 * 主窗口与房间窗口共用这一份：两个窗口各写一遍，配色迟早会不一致。
 *
 * # 为什么把 m3e 的入参"钉死"
 *
 * 动态配色走后端（`useColorScheme`），颜色由后端算出的 `--md-sys-color-*` 决定。
 * 只要 m3e 的 `color/variant/contrast` 还跟着用户设置走，它每次变化都会自己重算
 * 整张调色板并强制同步回流——那正是"改配色很卡"的来源。钉死之后它算什么都不影响，
 * 后端不可用时（浏览器预览）再把真实设置还给它。
 */
import { computed, ref } from "vue";

import { useColorScheme } from "../composables/useColorScheme";
import { useSettings } from "../composables/useSettings";

const PINNED_THEME = { color: "#0066cc", variant: "tonal-spot", contrast: "standard" } as const;

export function useThemeBinding() {
  const settings = useSettings();
  const colorScheme = useColorScheme();
  const themeEl = ref<HTMLElement | null>(null);

  const scheme = computed<"light" | "dark" | "auto">(() => {
    const mode = settings.state.theme_mode;
    return mode === "system" ? "auto" : (mode as "light" | "dark");
  });

  const color = computed(() =>
    colorScheme.active.value ? PINNED_THEME.color : settings.state.theme_color,
  );
  const variant = computed(() =>
    colorScheme.active.value ? PINNED_THEME.variant : settings.state.theme_variant,
  );
  const contrast = computed(() =>
    colorScheme.active.value ? PINNED_THEME.contrast : settings.state.theme_contrast,
  );

  return { settings, colorScheme, themeEl, scheme, color, variant, contrast };
}
