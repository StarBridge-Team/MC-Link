import { invoke } from "@tauri-apps/api/core";

/**
 * M3 动态配色：**调色板由后端生成**（`generate_m3_scheme`）。
 *
 * # 为什么从前端搬到后端
 *
 * 前端既算调色板、又重写整张 CSS 变量样式表，拖动取色器时每一次输入都要付一遍这个
 * 成本，实测很卡。改由后端算完之后，前端只做一件事：把 48 个角色写成 CSS 变量。
 * 顺带把 `<m3e-theme>` 的入参钉死，让它不再自行重算（见 `useColorScheme`）。
 *
 * # 为什么在这里翻译词汇
 *
 * 调用方用的是前端的词汇（变体 kebab-case、对比度三档枚举），后端用的是自己的
 * （变体 snake_case、对比度 -1..1）。翻译只在这一处发生，别处不需要知道后端的写法。
 */

/** 前端 kebab 变体 → 后端 snake 变体（后端未识别的值会回落到 tonal_spot）。 */
const VARIANTS: Record<string, string> = {
  "tonal-spot": "tonal_spot",
  "fruit-salad": "fruit_salad",
  vibrant: "vibrant",
  expressive: "expressive",
  neutral: "neutral",
  monochrome: "monochrome",
  fidelity: "fidelity",
  content: "content",
  rainbow: "rainbow",
};

/** 三档对比度 → 后端数值（与 `@m3e/web` 的 ContrastLevel 取值一致）。 */
const CONTRASTS: Record<string, number> = {
  standard: 0,
  medium: 0.5,
  high: 1,
};

/**
 * 单个主题（明或暗）下的颜色角色，键是后端字段名（snake_case），值是 `#rrggbb`。
 *
 * 用宽松的 `Record` 而不是逐字段声明：调用方是**按名字批量写 CSS 变量**，逐个字段
 * 声明只会多出 48 行需要同步维护的重复。字段集合与前端消费的变量集合已核对一致
 * （后端 48 个角色 ↔ m3e 组件引用的 48 个 `--md-sys-color-*`，一一对应）。
 */
export type M3Roles = Record<string, string>;

/** 后端 `M3Scheme` 里前端用得上的部分（调色板与语义色用不到，不声明）。 */
export interface M3Scheme {
  seed: string;
  variant: string;
  contrast: number;
  light: M3Roles;
  dark: M3Roles;
}

/** 按种子色 / 变体 / 对比度生成完整配色方案（明暗两套一起返回）。 */
export async function generateM3Scheme(
  seed: string,
  variant: string,
  contrast: string,
): Promise<M3Scheme> {
  return invoke<M3Scheme>("generate_m3_scheme", {
    seed,
    variant: VARIANTS[variant] ?? "tonal_spot",
    contrast: CONTRASTS[contrast] ?? 0,
  });
}
