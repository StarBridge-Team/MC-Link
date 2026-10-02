// 应用主题**交还给组件库自己生成**——不要再写 `--hsl-*` 去覆盖 Varlet 的组件配色。
//
// 做法：明暗只切换 Varlet 自带的 `Themes.md3Light` / `md3Dark`，它们各自是一套
// 完整且协调的 MD3 调色板（固定的紫色系）。此前我们用种子色自己算一套 M3 再覆盖
// 到 `--hsl-*`，结果配色很难看、还和组件库打架，那个覆盖层已整体撤掉。
//
// 本项目自己的组件（卡片、芯片等）的颜色见 `styles/tokens.css`：按明暗给两套 MD3
// 基线值，只服务于我们自己的 DOM，**不碰 Varlet 的组件变量**。

import { StyleProvider } from "@varlet/ui";
import { Themes } from "@varlet/ui";

/** 切换整体明暗：亮色走 Varlet 的 md3Light，暗色走 md3Dark。 */
export function applyTheme(dark: boolean): void {
  StyleProvider(dark ? Themes.md3Dark : Themes.md3Light);
}

/** 清空主题变量，退回 Varlet 默认主题。 */
export function clearTheme(): void {
  StyleProvider(null);
}
