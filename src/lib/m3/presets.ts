// M3 配色方案的固定选项。
//
// 变体取值必须与后端 `src-tauri/src/m3/mod.rs` 接受的一致（接受
// `fruitsalad` / `fruit_salad` 两种拼写，序列化回传统一为 `fruit_salad`）。
// 显示名不放这里：文案属于 i18n，见 `src/i18n/locales/*.ts` 的 `m3.variant.*`。

import type { M3Variant } from "./types";

/** 全部可用变体，顺序即界面展示顺序。 */
export const M3_VARIANTS: M3Variant[] = [
  "tonal_spot",
  "monochrome",
  "neutral",
  "vibrant",
  "expressive",
  "fidelity",
  "content",
  "rainbow",
  "fruit_salad",
];

/** 常用种子色（默认取 M3 基准紫）。 */
export const M3_PRESET_SEEDS: string[] = [
  "#6750A4",
  "#0061A4",
  "#006D3B",
  "#BA1A1A",
  "#7D5260",
  "#386A20",
  "#FF8F00",
  "#006874",
];

/** 对比度可调范围（后端会 clamp 到 [-1, 1]）。 */
export const M3_CONTRAST = { min: -1, max: 1, step: 0.25 } as const;

/** 变体的 i18n key 后缀（`m3.variant.<key>`）。 */
export const M3_VARIANT_LABEL_KEY: Record<M3Variant, string> = {
  tonal_spot: "tonalSpot",
  monochrome: "monochrome",
  neutral: "neutral",
  vibrant: "vibrant",
  expressive: "expressive",
  fidelity: "fidelity",
  content: "content",
  rainbow: "rainbow",
  fruit_salad: "fruitSalad",
};
