// useM3Theme —— M3 配色主题的响应式状态管理
//
// 职责：
//  - 维护种子颜色 / 变体 / 对比度 / 明暗模式
//  - 调用后端（Rust crate）或前端镜像引擎生成 M3 方案
//  - 将方案写入 CSS 变量（applyM3Scheme），供 m3-theme.css 桥接到 Element Plus
//  - 持久化关键参数到 localStorage，实现配置同步
//
// 注意：本 composable 只负责“生成方案 + 写入 CSS 变量”。
// `.m3-theme` / `.dark` 这两个 class 由使用方（如展示页根节点）控制，
// 以便 M3 主题既可全局启用，也可局部作用于某个容器，互不干扰。

import { ref, shallowRef, watch, type Ref } from "vue";
import { applyM3Scheme } from "../lib/m3/applyM3Theme";
import { exportM3Scheme, generateM3SchemeSynced } from "../lib/m3/m3Client";
import type { M3Scheme, M3Variant } from "../lib/m3/types";
import { local, KEYS } from "../lib/persist";

export const M3_VARIANTS: { value: M3Variant; label: string }[] = [
  { value: "tonal_spot", label: "Tonal Spot（默认）" },
  { value: "monochrome", label: "Monochrome" },
  { value: "neutral", label: "Neutral" },
  { value: "vibrant", label: "Vibrant" },
  { value: "expressive", label: "Expressive" },
  { value: "fidelity", label: "Fidelity" },
  { value: "content", label: "Content" },
  { value: "rainbow", label: "Rainbow" },
  { value: "fruit_salad", label: "Fruit Salad" },
];

export const M3_PRESET_SEEDS: string[] = [
  "#6750A4", // M3 经典紫
  "#0061A4", // 蓝
  "#006D3B", // 绿
  "#BA1A1A", // 红
  "#7D5260", // 玫瑰
  "#386A20", // 橄榄
  "#FF8F00", // 橙
  "#006874", // 青
];

interface M3PersistedConfig {
  seed: string;
  variant: M3Variant;
  contrast: number;
  isDark: boolean;
}

function loadConfig(): M3PersistedConfig {
  const fallback: M3PersistedConfig = {
    seed: "#6750A4",
    variant: "tonal_spot",
    contrast: 0,
    isDark: false,
  };
  const saved = local.get<Partial<M3PersistedConfig> | null>(KEYS.m3Theme, null);
  return saved ? { ...fallback, ...saved } : fallback;
}

export function useM3Theme() {
  const persisted = loadConfig();

  const seed: Ref<string> = ref(persisted.seed);
  const variant: Ref<M3Variant> = ref(persisted.variant);
  const contrast: Ref<number> = ref(persisted.contrast);
  const isDark: Ref<boolean> = ref(persisted.isDark);

  const scheme: Ref<M3Scheme | null> = shallowRef<M3Scheme | null>(null);
  const loading = ref(false);
  const source = ref<"backend" | "frontend">("frontend");

  async function regenerate() {
    loading.value = true;
    try {
      const { scheme: result, source: src } = await generateM3SchemeSynced(seed.value, {
        variant: variant.value,
        contrast: contrast.value,
      });
      scheme.value = result;
      applyM3Scheme(result);
      source.value = src;
    } finally {
      loading.value = false;
    }
  }

  function setSeed(value: string) {
    seed.value = value;
  }
  function setVariant(v: M3Variant) {
    variant.value = v;
  }
  function setContrast(c: number) {
    contrast.value = c;
  }
  function toggleDark() {
    isDark.value = !isDark.value;
  }
  function setDark(v: boolean) {
    isDark.value = v;
  }

  /** 导出当前方案为 JSON 字符串（供配置同步 / 持久化） */
  function exportConfig(): string {
    return scheme.value ? exportM3Scheme(scheme.value) : "";
  }

  /** 持久化关键参数 */
  function persist() {
    const cfg: M3PersistedConfig = {
      seed: seed.value,
      variant: variant.value,
      contrast: contrast.value,
      isDark: isDark.value,
    };
    local.set(KEYS.m3Theme, cfg);
  }

  // 参数变化时自动重新生成并持久化
  watch([seed, variant, contrast], () => {
    void regenerate();
    persist();
  });
  watch(isDark, () => persist());

  return {
    seed,
    variant,
    contrast,
    isDark,
    scheme,
    loading,
    source,
    regenerate,
    setSeed,
    setVariant,
    setContrast,
    toggleDark,
    setDark,
    exportConfig,
    persist,
  };
}
