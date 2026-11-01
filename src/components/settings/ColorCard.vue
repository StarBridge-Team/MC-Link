<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../ui/ChipSelect.vue";
import ColorField from "../ui/ColorField.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";
import type { ThemeSeedSource } from "../../lib/api/types";

/**
 * 个性化 · 配色：种子来源 + 种子色 + 配色风格 + 对比度。
 *
 * 这里**只写配置**。调色板由后端算出（`generate_m3_scheme`），再由 `useColorScheme`
 * 把结果写成 `--md-sys-color-*` 变量——不是 `<m3e-theme>` 自己生成（那套已钉死，
 * 见 `App.vue` 的注释）。取值必须与 `@m3e/web` 的 `ThemeVariant` / `ContrastLevel`
 * 完全一致（kebab-case），否则会回落到默认值。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

/**
 * 选了非「手选」的来源后，下面的色板变成**回退值**（取色失败时仍用它），
 * 所以色板不禁用——只是换了个含义，禁用会让人以为这个色没了。
 */
const seedSource = computed(() => state.theme_seed_source || "manual");

const sourceOptions = computed<ChipOption<string>[]>(() => [
  { value: "manual", label: t("color.sourceManual") },
  { value: "background", label: t("color.sourceBackground") },
  { value: "system_wallpaper", label: t("color.sourceSystemWallpaper") },
  { value: "system_accent", label: t("color.sourceSystemAccent") },
]);

/** 每个来源取不到时的行为都一样（回退手选色），但取的是什么颜色不同，说明要跟上。 */
const sourceHint = computed(() => {
  switch (seedSource.value) {
    case "background":
      return t("color.sourceHintBackground");
    case "system_wallpaper":
      return t("color.sourceHintSystemWallpaper");
    case "system_accent":
      return t("color.sourceHintSystemAccent");
    default:
      return t("color.sourceHintManual");
  }
});

function setSource(value: string) {
  settings.patch({ theme_seed_source: value as ThemeSeedSource });
}

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
    <!-- 种子来源：手选 / 背景图 / 系统壁纸 / 系统颜色。
         "背景图"只对 `background_type = image` 有意义，但**不禁用**这个选项：
         它的实际行为是"取不到就回退手选色"，禁用会让人以为设置丢了。 -->
    <div class="field-label">{{ t("color.seedSource") }}</div>
    <ChipSelect :model-value="seedSource" :options="sourceOptions" @update:model-value="setSource" />
    <p class="hint">{{ sourceHint }}</p>

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
