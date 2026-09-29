// M3 配色方案类型定义
// 与 src-tauri/src/m3/commands.rs 中的 serde 返回结构保持一致，
// 前后端通过此结构进行接口 / 配置同步。

export type M3Variant =
  | "tonal_spot"
  | "monochrome"
  | "neutral"
  | "vibrant"
  | "expressive"
  | "fidelity"
  | "content"
  | "rainbow"
  | "fruit_salad";

export type M3Mode = "light" | "dark";

/** 单个色调调色板：色阶 -> hex */
export interface TonalRamp {
  tones: Record<number, string>;
}

/** 完整的 M3 色调调色板集合 */
export interface M3Palettes {
  primary: TonalRamp;
  secondary: TonalRamp;
  tertiary: TonalRamp;
  neutral: TonalRamp;
  neutral_variant: TonalRamp;
  error: TonalRamp;
}

/** 符合 M3 规范的颜色角色集合（全部为 hex 字符串） */
export interface M3Roles {
  // primary
  primary: string;
  on_primary: string;
  primary_container: string;
  on_primary_container: string;
  inverse_primary: string;
  primary_fixed: string;
  on_primary_fixed: string;
  primary_fixed_dim: string;
  on_primary_fixed_variant: string;
  // secondary
  secondary: string;
  on_secondary: string;
  secondary_container: string;
  on_secondary_container: string;
  secondary_fixed: string;
  on_secondary_fixed: string;
  secondary_fixed_dim: string;
  on_secondary_fixed_variant: string;
  // tertiary
  tertiary: string;
  on_tertiary: string;
  tertiary_container: string;
  on_tertiary_container: string;
  tertiary_fixed: string;
  on_tertiary_fixed: string;
  tertiary_fixed_dim: string;
  on_tertiary_fixed_variant: string;
  // error
  error: string;
  on_error: string;
  error_container: string;
  on_error_container: string;
  // surface / background
  background: string;
  on_background: string;
  surface: string;
  on_surface: string;
  surface_variant: string;
  on_surface_variant: string;
  outline: string;
  outline_variant: string;
  shadow: string;
  scrim: string;
  inverse_surface: string;
  inverse_on_surface: string;
  surface_dim: string;
  surface_bright: string;
  surface_container_lowest: string;
  surface_container_low: string;
  surface_container: string;
  surface_container_high: string;
  surface_container_highest: string;
  // 允许任意额外角色（前向兼容）
  [key: string]: string;
}

export interface M3SemanticColor {
  light: M3Roles;
  dark: M3Roles;
}

export interface M3Semantic {
  success: M3SemanticColor;
  warning: M3SemanticColor;
  error: M3SemanticColor;
  info: M3SemanticColor;
}

/** 完整的 M3 配色方案 */
export interface M3Scheme {
  seed: string;
  variant: M3Variant;
  contrast: number;
  palettes: M3Palettes;
  light: M3Roles;
  dark: M3Roles;
  semantic: M3Semantic;
}

/** 生成配色方案的请求参数 */
export interface M3GenerateOptions {
  variant?: M3Variant;
  contrast?: number;
}
