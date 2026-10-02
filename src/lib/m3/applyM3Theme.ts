// 把 M3 配色方案落到 DOM 上。
//
// # 为什么写到 documentElement 的 inline style
//
// Varlet 的弹层（Dialog / Snackbar / Popup）会 teleport 到 `body`，若把主题变量挂在
// 某个包裹元素上，弹层就取不到变量。因此统一写到 `:root`，与组件树结构无关。
//
// # 两套变量，一套来源
//
// 1. `--m3-<role>`：M3 颜色角色原文（hex），给本项目自己的 CSS 用；
// 2. `--hsl-*`：把同一批角色换算成 **HSL 三段式**（`220, 99%, 61%`）——
//    这是 Varlet 的书写形式（`--color-primary: hsla(var(--hsl-primary), 1)`），
//    只改三段式就能让所有 Varlet 组件跟着换色，包括带透明度的派生用法。
//
// 组件级变量（圆角、高度、按钮色引用等）来自 Varlet 自带的 MD3 主题对象
// （`Themes.md3Light` / `md3Dark`），它们同样引用 `--color-*`，因此叠加即可。

import { Themes } from "@varlet/ui";
import type { M3Roles, M3Scheme } from "./types";

/** `#rrggbb` / `#rgb` → `h, s%, l%`（Varlet 的三段式写法）。 */
export function hexToHslTriplet(hex: string): string {
  let value = hex.trim().replace(/^#/, "");
  if (value.length === 3) {
    value = value
      .split("")
      .map((c) => c + c)
      .join("");
  }
  // 8 位（#aarrggbb）与 4 位按"忽略 alpha"处理：Varlet 的三段式不带透明度。
  if (value.length === 8) value = value.slice(2);
  if (value.length === 4) {
    value = value
      .slice(1)
      .split("")
      .map((c) => c + c)
      .join("");
  }
  const num = Number.parseInt(value, 16);
  if (!Number.isFinite(num) || value.length !== 6) return "0, 0%, 0%";

  const r = ((num >> 16) & 0xff) / 255;
  const g = ((num >> 8) & 0xff) / 255;
  const b = (num & 0xff) / 255;

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;

  let h = 0;
  let s = 0;
  if (d !== 0) {
    s = d / (1 - Math.abs(2 * l - 1));
    switch (max) {
      case r:
        h = ((g - b) / d) % 6;
        break;
      case g:
        h = (b - r) / d + 2;
        break;
      default:
        h = (r - g) / d + 4;
    }
    h *= 60;
    if (h < 0) h += 360;
  }

  return `${Math.round(h)}, ${Math.round(s * 100)}%, ${Math.round(l * 100)}%`;
}

/** M3 角色 → Varlet 三段式变量名。 */
const HSL_BRIDGE: Record<string, keyof M3Roles & string> = {
  "--hsl-body": "surface",
  "--hsl-text": "on_surface",
  "--hsl-primary": "primary",
  "--hsl-on-primary": "on_primary",
  "--hsl-primary-container": "primary_container",
  "--hsl-on-primary-container": "on_primary_container",
  "--hsl-surface-container": "surface_container",
  "--hsl-surface-container-low": "surface_container_low",
  "--hsl-surface-container-high": "surface_container_high",
  "--hsl-surface-container-highest": "surface_container_highest",
  "--hsl-inverse-surface": "inverse_surface",
  "--hsl-outline": "outline",
  "--hsl-on-surface": "on_surface",
  "--hsl-on-surface-variant": "on_surface_variant",
  "--hsl-disabled": "surface_container_highest",
  "--hsl-text-disabled": "on_surface_variant",
};

/** M3 零散角色 → 显式 `--color-*`（Varlet 只定义了一部分，其余供本项目 CSS 使用）。 */
const COLOR_BRIDGE: Record<string, keyof M3Roles & string> = {
  "--color-outline-variant": "outline_variant",
  "--color-surface": "surface",
  "--color-surface-variant": "surface_variant",
  "--color-on-surface": "on_surface",
  "--color-secondary": "secondary",
  "--color-on-secondary": "on_secondary",
  "--color-secondary-container": "secondary_container",
  "--color-on-secondary-container": "on_secondary_container",
  "--color-tertiary": "tertiary",
  "--color-on-tertiary": "on_tertiary",
  "--color-tertiary-container": "tertiary_container",
  "--color-on-tertiary-container": "on_tertiary_container",
  "--color-scrim": "scrim",
  "--color-shadow": "shadow",
};

/** 语义色（success / warning / error / info）在 Varlet 侧的变量名。 */
const SEMANTIC_VARS = {
  success: "success",
  warning: "warning",
  error: "danger",
  info: "info",
} as const;

export type SemanticKey = keyof typeof SEMANTIC_VARS;

/** 生成一组完整的待写入 CSS 变量（不含 Varlet 组件级默认值）。 */
export function buildSchemeVars(scheme: M3Scheme, isDark: boolean): Record<string, string> {
  const roles = isDark ? scheme.dark : scheme.light;
  const vars: Record<string, string> = {};

  for (const [role, value] of Object.entries(roles)) {
    if (typeof value !== "string") continue;
    // 两种拼写都写：下划线与后端 serde 契约逐字对应，连字符是 CSS 侧的惯用形式。
    // **必须两种都给**：变量名拼错时 `var()` 不会报错，只会静默使用兜底色，
    // 表现为"主题只改了一半"（曾经因为这里只写了下划线、CSS 里用连字符，
    // 多词角色全部退回基线配色）。多写几十个属性的代价远低于这类排查成本。
    vars[`--m3-${role}`] = value;
    vars[`--m3-${role.replace(/_/g, "-")}`] = value;
  }

  for (const [name, role] of Object.entries(HSL_BRIDGE)) {
    const value = roles[role];
    if (value) vars[name] = hexToHslTriplet(value);
  }

  for (const [name, role] of Object.entries(COLOR_BRIDGE)) {
    const value = roles[role];
    if (value) vars[name] = value;
  }

  for (const [key, suffix] of Object.entries(SEMANTIC_VARS)) {
    const sem = scheme.semantic[key as SemanticKey];
    if (!sem) continue;
    const set = isDark ? sem.dark : sem.light;
    vars[`--hsl-${suffix}`] = hexToHslTriplet(set.primary);
    vars[`--hsl-${suffix}-container`] = hexToHslTriplet(set.primary_container);
    vars[`--hsl-on-${suffix}`] = hexToHslTriplet(set.on_primary);
    vars[`--hsl-on-${suffix}-container`] = hexToHslTriplet(set.on_primary_container);
    vars[`--m3-sem-${key}`] = set.primary;
    vars[`--m3-sem-${key}-container`] = set.primary_container;
    vars[`--m3-sem-${key}-on`] = set.on_primary;
    vars[`--m3-sem-${key}-on-container`] = set.on_primary_container;
  }

  // 正文字色与描边：Varlet 的基线写法带 alpha，这里显式覆盖为不透明的 M3 角色。
  vars["--color-outline"] = roles.outline;
  vars["--color-text"] = roles.on_surface;
  vars["--color-body"] = roles.surface;
  return vars;
}

/** 上一次写入的变量名，便于切换主题时清理不再出现的键。 */
let applied = new Set<string>();

/** 把方案写入 `:root`。`isDark` 决定取方案里的暗色还是亮色一组。 */
export function applyM3Scheme(scheme: M3Scheme, isDark: boolean): void {
  const root = document.documentElement;
  const md3 = (isDark ? Themes.md3Dark : Themes.md3Light) as Record<string, string>;
  const vars = { ...md3, ...buildSchemeVars(scheme, isDark) };

  for (const [name, value] of Object.entries(vars)) {
    root.style.setProperty(name, value);
  }

  // 清理上一轮存在、这一轮没有的键（例如语义色缺项时）。
  for (const name of applied) {
    if (!(name in vars)) root.style.removeProperty(name);
  }
  applied = new Set(Object.keys(vars));

  root.classList.toggle("var--dark", isDark);
  root.classList.toggle("dark", isDark);
}

/** 移除所有 M3 变量，回到 Varlet 基线主题。 */
export function clearM3Scheme(): void {
  const root = document.documentElement;
  for (const name of applied) root.style.removeProperty(name);
  applied = new Set();
}
