// M3 配色引擎（TypeScript 实现）
//
// 作为 Rust 后端 `generate_m3_scheme` 命令的镜像实现（fallback）：
// 当应用运行在浏览器预览（vite dev）或 Tauri 后端不可用 / 报错时，
// 前端使用本引擎生成与后端算法一致（同结构、同角色映射、同变体/对比度语义）的
// M3 方案，保证前后端配色数据视觉统一、可同步。
//
// 说明：后端使用成熟的 `material-colors` crate 做精确的 HCT 计算；
// 本 TS 实现采用等价的“种子色相 + 受控色度”色调调色板模型，
// 并复用与后端完全一致的 M3 角色 -> 色阶映射与变体色度系数，
// 因此视觉高度贴近 M3 规范。

import type {
  M3Mode,
  M3Palettes,
  M3Roles,
  M3Scheme,
  M3Semantic,
  M3Variant,
} from "./types";

/** M3 标准 13 个色阶 */
const TONES = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99, 100];

export function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

export function hexToRgb(hex: string): { r: number; g: number; b: number } {
  let h = hex.trim().replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  if (h.length === 8) h = h.slice(2); // AARRGGBB -> RGB
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return { r, g, b };
}

export function rgbToHex(r: number, g: number, b: number): string {
  const c = (v: number) =>
    clamp(Math.round(v), 0, 255).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}

function rgbToHsl(r: number, g: number, b: number): { h: number; s: number; l: number } {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  let h = 0;
  let s = 0;
  const d = max - min;
  if (d !== 0) {
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case r:
        h = (g - b) / d + (g < b ? 6 : 0);
        break;
      case g:
        h = (b - r) / d + 2;
        break;
      default:
        h = (r - g) / d + 4;
    }
    h *= 60;
  }
  return { h, s, l };
}

/** 由色相 / 饱和度 / 明度生成 hex（HSL 模型，足够贴近 M3 观感） */
export function makeColor(h: number, s: number, l: number): string {
  h = ((h % 360) + 360) % 360;
  s = clamp(s, 0, 1);
  l = clamp(l, 0, 1);
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  let r = 0;
  let g = 0;
  let b = 0;
  if (h < 60) {
    r = c;
    g = x;
  } else if (h < 120) {
    r = x;
    g = c;
  } else if (h < 180) {
    g = c;
    b = x;
  } else if (h < 240) {
    g = x;
    b = c;
  } else if (h < 300) {
    r = x;
    b = c;
  } else {
    r = c;
    b = x;
  }
  return rgbToHex((r + m) * 255, (g + m) * 255, (b + m) * 255);
}

/** 色度随色阶的变化曲线：中间色阶最饱和，两端趋近灰 */
function chromaShape(t: number): number {
  return Math.pow(Math.max(0, 1 - Math.abs(t - 50) / 50), 0.8);
}

type PaletteKey =
  | "primary"
  | "secondary"
  | "tertiary"
  | "neutral"
  | "neutral_variant"
  | "error";

/** 每个调色板的色相 / 色度（用于按任意色阶取色） */
interface Ramp {
  hue: number;
  chroma: number;
}

type Ramps = Record<PaletteKey, Ramp>;

/**
 * 各变体相对基础色度的系数（近似 Material 官方语义）：
 * monochrome 完全去色、neutral 低饱和、vibrant/rainbow 高饱和……
 * 与后端 `material-colors` 的变体保持一致的“语义方向”。
 */
const VARIANT_CHROMA: Record<M3Variant, number> = {
  monochrome: 0,
  neutral: 0.12,
  tonal_spot: 1,
  fidelity: 1,
  content: 1,
  vibrant: 1.5,
  expressive: 1.3,
  rainbow: 1.6,
  fruit_salad: 1.6,
};

const VARIANT_TERTIARY_HUE: Record<M3Variant, number> = {
  monochrome: 0,
  neutral: 0,
  tonal_spot: 60,
  fidelity: 60,
  content: 60,
  vibrant: 60,
  expressive: 120,
  rainbow: 0,
  fruit_salad: 120,
};

export function buildRamps(seed: string, variant: M3Variant = "tonal_spot"): Ramps {
  const { r, g, b } = hexToRgb(seed);
  const { h, s } = rgbToHsl(r, g, b);
  const base = clamp(s, 0.12, 1) * (VARIANT_CHROMA[variant] ?? 1);
  return {
    primary: { hue: h, chroma: base },
    secondary: { hue: h, chroma: base * 0.48 },
    tertiary: { hue: (h + (VARIANT_TERTIARY_HUE[variant] ?? 60)) % 360, chroma: base * 0.48 },
    neutral: { hue: h, chroma: base * 0.06 },
    neutral_variant: { hue: h, chroma: base * 0.12 },
    error: { hue: 25, chroma: 0.5 },
  };
}

function rampTone(ramp: Ramp, tone: number): string {
  return makeColor(ramp.hue, ramp.chroma * chromaShape(tone), tone / 100);
}

/** 将内存中的 ramps 展开为可序列化的调色板（与后端结构一致） */
function rampsToPalettes(ramps: Ramps): M3Palettes {
  const out = {} as M3Palettes;
  (Object.keys(ramps) as PaletteKey[]).forEach((key) => {
    const tones: Record<number, string> = {};
    for (const t of TONES) tones[t] = rampTone(ramps[key], t);
    out[key] = { tones };
  });
  return out;
}

// M3 角色 -> [调色板, 色阶] 映射（与官方 Material Theme Builder 一致）
const ROLE_MAP: Record<M3Mode, Record<string, [PaletteKey, number]>> = {
  light: {
    primary: ["primary", 40],
    on_primary: ["primary", 100],
    primary_container: ["primary", 90],
    on_primary_container: ["primary", 10],
    inverse_primary: ["primary", 80],
    primary_fixed: ["primary", 90],
    on_primary_fixed: ["primary", 10],
    primary_fixed_dim: ["primary", 80],
    on_primary_fixed_variant: ["primary", 30],

    secondary: ["secondary", 40],
    on_secondary: ["secondary", 100],
    secondary_container: ["secondary", 90],
    on_secondary_container: ["secondary", 10],
    secondary_fixed: ["secondary", 90],
    on_secondary_fixed: ["secondary", 10],
    secondary_fixed_dim: ["secondary", 80],
    on_secondary_fixed_variant: ["secondary", 30],

    tertiary: ["tertiary", 40],
    on_tertiary: ["tertiary", 100],
    tertiary_container: ["tertiary", 90],
    on_tertiary_container: ["tertiary", 10],
    tertiary_fixed: ["tertiary", 90],
    on_tertiary_fixed: ["tertiary", 10],
    tertiary_fixed_dim: ["tertiary", 80],
    on_tertiary_fixed_variant: ["tertiary", 30],

    error: ["error", 40],
    on_error: ["error", 100],
    error_container: ["error", 90],
    on_error_container: ["error", 10],

    background: ["neutral", 99],
    on_background: ["neutral", 10],
    surface: ["neutral", 99],
    on_surface: ["neutral", 10],
    surface_variant: ["neutral_variant", 90],
    on_surface_variant: ["neutral_variant", 30],
    outline: ["neutral_variant", 50],
    outline_variant: ["neutral_variant", 80],
    shadow: ["neutral", 0],
    scrim: ["neutral", 0],
    inverse_surface: ["neutral", 20],
    inverse_on_surface: ["neutral", 95],
    surface_dim: ["neutral", 87],
    surface_bright: ["neutral", 98],
    surface_container_lowest: ["neutral", 100],
    surface_container_low: ["neutral", 96],
    surface_container: ["neutral", 94],
    surface_container_high: ["neutral", 92],
    surface_container_highest: ["neutral", 90],
  },
  dark: {
    primary: ["primary", 80],
    on_primary: ["primary", 20],
    primary_container: ["primary", 30],
    on_primary_container: ["primary", 90],
    inverse_primary: ["primary", 40],
    primary_fixed: ["primary", 90],
    on_primary_fixed: ["primary", 10],
    primary_fixed_dim: ["primary", 80],
    on_primary_fixed_variant: ["primary", 30],

    secondary: ["secondary", 80],
    on_secondary: ["secondary", 20],
    secondary_container: ["secondary", 30],
    on_secondary_container: ["secondary", 90],
    secondary_fixed: ["secondary", 90],
    on_secondary_fixed: ["secondary", 10],
    secondary_fixed_dim: ["secondary", 80],
    on_secondary_fixed_variant: ["secondary", 30],

    tertiary: ["tertiary", 80],
    on_tertiary: ["tertiary", 20],
    tertiary_container: ["tertiary", 30],
    on_tertiary_container: ["tertiary", 90],
    tertiary_fixed: ["tertiary", 90],
    on_tertiary_fixed: ["tertiary", 10],
    tertiary_fixed_dim: ["tertiary", 80],
    on_tertiary_fixed_variant: ["tertiary", 30],

    error: ["error", 80],
    on_error: ["error", 20],
    error_container: ["error", 30],
    on_error_container: ["error", 90],

    background: ["neutral", 6],
    on_background: ["neutral", 90],
    surface: ["neutral", 6],
    on_surface: ["neutral", 90],
    surface_variant: ["neutral_variant", 30],
    on_surface_variant: ["neutral_variant", 80],
    outline: ["neutral_variant", 60],
    outline_variant: ["neutral_variant", 30],
    shadow: ["neutral", 0],
    scrim: ["neutral", 0],
    inverse_surface: ["neutral", 98],
    inverse_on_surface: ["neutral", 10],
    surface_dim: ["neutral", 6],
    surface_bright: ["neutral", 24],
    surface_container_lowest: ["neutral", 4],
    surface_container_low: ["neutral", 10],
    surface_container: ["neutral", 12],
    surface_container_high: ["neutral", 17],
    surface_container_highest: ["neutral", 22],
  },
};

/**
 * 依据对比度微调色阶：正值提高对比（亮者更亮、暗者更暗）。
 * 与 M3 对比度语义方向一致，用于前后端一致的观感。
 */
function adjustTone(tone: number, contrast: number): number {
  if (contrast === 0) return tone;
  const sign = tone >= 50 ? 1 : -1;
  return clamp(Math.round(tone + sign * contrast * 14), 0, 100);
}

function rolesFromRamps(ramps: Ramps, mode: M3Mode, contrast: number): M3Roles {
  const map = ROLE_MAP[mode];
  const roles = {} as M3Roles;
  for (const key of Object.keys(map)) {
    const [pk, tone] = map[key];
    roles[key] = rampTone(ramps[pk], adjustTone(tone, contrast));
  }
  return roles;
}

function semanticFromSeed(
  seed: string,
  variant: M3Variant,
  contrast: number
): { light: M3Roles; dark: M3Roles } {
  const ramps = buildRamps(seed, variant);
  return {
    light: rolesFromRamps(ramps, "light", contrast),
    dark: rolesFromRamps(ramps, "dark", contrast),
  };
}

/** 计算完整的 M3 方案（与后端 `generate_m3_scheme` 返回结构一致） */
export function generateM3Scheme(
  seed: string,
  options: { variant?: M3Variant; contrast?: number } = {}
): M3Scheme {
  const variant = options.variant ?? "tonal_spot";
  const contrast = clamp(options.contrast ?? 0, -1, 1);
  const ramps = buildRamps(seed, variant);
  const palettes = rampsToPalettes(ramps);
  const semantic: M3Semantic = {
    success: semanticFromSeed("#2e7d32", variant, contrast),
    warning: semanticFromSeed("#ed6c02", variant, contrast),
    error: semanticFromSeed("#d32f2f", variant, contrast),
    info: semanticFromSeed(seed, variant, contrast),
  };
  return {
    seed,
    variant,
    contrast,
    palettes,
    light: rolesFromRamps(ramps, "light", contrast),
    dark: rolesFromRamps(ramps, "dark", contrast),
    semantic,
  };
}
