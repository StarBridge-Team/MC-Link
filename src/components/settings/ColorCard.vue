<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../ui/ChipSelect.vue";
import ColorField from "../ui/ColorField.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";

/**
 * 个性化 · 配色：种子色 + 配色风格 + 对比度。
 *
 * 这里**只写配置**，真正的调色板由 `<m3e-theme>`（App.vue 根节点）按 M3 规范生成，
 * 因此改完立即生效且随明暗自动调整。取值必须与 `@m3e/web` 的 `ThemeVariant` /
 * `ContrastLevel` 完全一致（kebab-case），否则组件会回落到默认值。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

const variantOptions = computed<ChipOption<string>[]>(() => [
  { value: "tonal-spot", label: t("color.variantNames.tonalSpot") },
  { value: "vibrant", label: t("color.variantNames.vibrant") },
  { value: "expressive", label: t("color.variantNames.expressive") },
  { value: "neutral", label: t("color.variantNames.neutral") },
  { value: "monochrome", label: t("color.variantNames.monochrome") },
  { value: "fidelity", label: t("color.variantNames.fidelity") },
  { value: "content", label: t("color.variantNames.content") },
  { value: "rainbow", label: t("color.variantNames.rainbow") },
  { value: "fruit-salad", label: t("color.variantNames.fruitSalad") },
]);

const contrastOptions = computed<ChipOption<string>[]>(() => [
  { value: "standard", label: t("color.contrastNames.standard") },
  { value: "medium", label: t("color.contrastNames.medium") },
  { value: "high", label: t("color.contrastNames.high") },
]);

function setSeed(value: string) {
  settings.patch({ theme_color: value });
}

function setVariant(value: string) {
  settings.patch({ theme_variant: value });
}

function setContrast(value: string) {
  settings.patch({ theme_contrast: value });
}
</script>

<template>
  <SettingCard :icon="'palette'" :title="t('color.title')" :desc="t('color.desc')" wide>
    <div class="color-row">
      <span class="field-label">{{ t("color.seed") }}</span>
      <ColorField :model-value="state.theme_color" size="large" @update:model-value="setSeed" />
    </div>

    <div class="field-label">{{ t("color.variant") }}</div>
    <ChipSelect
      :model-value="state.theme_variant"
      :options="variantOptions"
      @update:model-value="setVariant"
    />

    <div class="field-label">{{ t("color.contrast") }}</div>
    <ChipSelect
      small
      :model-value="state.theme_contrast"
      :options="contrastOptions"
      @update:model-value="setContrast"
    />
  </SettingCard>
</template>

<style scoped>
.color-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}
</style>
