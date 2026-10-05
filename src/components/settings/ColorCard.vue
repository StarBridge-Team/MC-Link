<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../ui/ChipSelect.vue";
import ColorField from "../ui/ColorField.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";

/**
 * 个性化 · 配色：种子色（或"跟随背景图"）+ 配色风格 + 对比度。
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
 * 开启"跟随背景图"后，手选的种子色变成**回退值**（取色失败时仍用它），
 * 所以色板不禁用——只是换了个含义，禁用会让人以为这个色没了。
 */
function setFromBackground(value: boolean) {
  settings.patch({ theme_from_background: value });
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
    <!-- 跟随背景图：只对"图片"背景有意义，其它背景类型下这个开关是空转的 -->
    <div class="color-row color-row--between">
      <div class="color-row__text">
        <span class="field-label">{{ t("color.followBackground") }}</span>
        <span class="hint">{{ t("color.followBackgroundHint") }}</span>
      </div>
      <m3e-switch
        :checked="state.theme_from_background"
        @change="setFromBackground(($event.target as HTMLInputElement).checked)"
      />
    </div>

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

/* 左侧一段说明文字 + 右侧控件：文字可换行，控件不被压缩 */
.color-row--between {
  justify-content: space-between;
  align-items: flex-start;
}

.color-row__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
</style>
