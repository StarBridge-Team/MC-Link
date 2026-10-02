<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../../components/ui/ChipSelect.vue";
import ColorField from "../../components/ui/ColorField.vue";
import FieldRow from "../../components/ui/FieldRow.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";
import { M3_CONTRAST, M3_PRESET_SEEDS, M3_VARIANTS, M3_VARIANT_LABEL_KEY } from "../../lib/m3/presets";
import type { M3Variant } from "../../lib/m3/types";

/**
 * 个性化 → 外观。
 *
 * 全部改动都经 `useSettings().patch()`：它是这套字段的唯一写入口，
 * 负责"立即生效 + 防抖落盘"。界面本身不碰 IPC，也不碰 localStorage。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

const modeOptions = computed<ChipOption<string>[]>(() => [
  { value: "system", label: t("appearance.modeSystem"), icon: "bi bi-circle-half" },
  { value: "light", label: t("appearance.modeLight"), icon: "bi bi-sun" },
  { value: "dark", label: t("appearance.modeDark"), icon: "bi bi-moon-stars" },
]);

// 后端只认 mica / acrylic / hud_window，"无"即清空材质。
// `transparent` 是历史取值，与 "none" 等效，这里不单独提供选项。
const effectOptions = computed<ChipOption<string>[]>(() => [
  { value: "none", label: t("appearance.effectNone"), icon: "bi bi-slash-circle" },
  { value: "mica", label: t("appearance.effectMica"), icon: "bi bi-window-stack" },
  { value: "acrylic", label: t("appearance.effectAcrylic"), icon: "bi bi-droplet-half" },
  { value: "hud_window", label: t("appearance.effectHud"), icon: "bi bi-layout-sidebar" },
]);

const variantOptions = computed<ChipOption<M3Variant>[]>(() =>
  M3_VARIANTS.map((variant) => ({
    value: variant,
    label: t(`m3.variantNames.${M3_VARIANT_LABEL_KEY[variant]}`),
  })),
);

const schemeSourceLabel = computed(() =>
  settings.schemeSource.value === "backend"
    ? t("appearance.schemeSourceBackend")
    : t("appearance.schemeSourceFrontend"),
);

function setThemeColor(value: string) {
  settings.patch({ theme_color: value });
}

function setMode(value: string) {
  settings.patch({ theme_mode: value });
}

function setEffect(value: string) {
  settings.patch({ transparent_effect: value });
}

// Varlet 的开关/滑块在类型上允许 number | number[]（区间模式），
// 这里统一收敛成配置需要的标量，避免把数组写进 yml。
function firstNumber(value: number | number[]): number {
  const n = Array.isArray(value) ? value[0] : value;
  return typeof n === "number" && Number.isFinite(n) ? n : 0;
}

function setAnimationEnabled(value: unknown) {
  settings.patch({ animation_enabled: value !== false });
}

function setAnimationSpeed(value: number | number[]) {
  settings.patch({ animation_speed: firstNumber(value) });
}

function setVariant(value: M3Variant) {
  settings.patchM3({ variant: value });
}

function setContrast(value: number | number[]) {
  settings.patchM3({ contrast: firstNumber(value) });
}
</script>

<template>
  <div class="scroll-area">
    <div class="grid">
      <SettingCard
        :icon="'bi bi-palette'"
        :title="t('appearance.title')"
        :desc="t('appearance.desc')"
        wide
      >
        <div class="field-label">{{ t("appearance.presets") }}</div>
        <div class="swatches">
          <button
            v-for="seed in M3_PRESET_SEEDS"
            :key="seed"
            class="swatch"
            :class="{ 'is-active': state.theme_color.toLowerCase() === seed.toLowerCase() }"
            type="button"
            :style="{ background: seed }"
            :title="seed"
            @click="setThemeColor(seed)"
          >
            <i v-if="state.theme_color.toLowerCase() === seed.toLowerCase()" class="bi bi-check-lg" />
          </button>
          <div class="swatches__custom">
            <ColorField :model-value="state.theme_color" size="large" @update:model-value="setThemeColor" />
          </div>
        </div>

        <div class="divider" />

        <div class="field-label">{{ t("appearance.mode") }}</div>
        <ChipSelect :model-value="state.theme_mode" :options="modeOptions" @update:model-value="setMode" />
      </SettingCard>

      <SettingCard :icon="'bi bi-droplet-half'" :title="t('appearance.effect')" :desc="t('appearance.effectDesc')" wide>
        <ChipSelect
          :model-value="settings.effect.value"
          :options="effectOptions"
          @update:model-value="setEffect"
        />
      </SettingCard>

      <SettingCard :icon="'bi bi-magic'" :title="t('appearance.animation')" :desc="t('appearance.animationDesc')">
        <FieldRow :label="t('appearance.animation')">
          <var-switch
            :model-value="state.animation_enabled"
            @update:model-value="setAnimationEnabled"
          />
        </FieldRow>
        <FieldRow v-if="state.animation_enabled" :label="t('appearance.animationSpeed')">
          <div class="slider-row">
            <var-slider
              class="slider-row__slider"
              :model-value="state.animation_speed"
              :min="0.25"
              :max="2"
              :step="0.25"
              @update:model-value="setAnimationSpeed"
            />
            <span class="slider-row__value mono">{{ state.animation_speed }}x</span>
          </div>
        </FieldRow>
      </SettingCard>

      <SettingCard :icon="'bi bi-sliders2'" :title="t('appearance.m3Title')" :desc="t('appearance.m3Desc')">
        <div class="field-label">{{ t("appearance.variant") }}</div>
        <ChipSelect
          small
          :model-value="settings.m3.variant"
          :options="variantOptions"
          @update:model-value="setVariant"
        />
        <FieldRow :label="t('appearance.contrast')">
          <div class="slider-row">
            <var-slider
              class="slider-row__slider"
              :model-value="settings.m3.contrast"
              :min="M3_CONTRAST.min"
              :max="M3_CONTRAST.max"
              :step="M3_CONTRAST.step"
              @update:model-value="setContrast"
            />
            <span class="slider-row__value mono">{{ settings.m3.contrast.toFixed(2) }}</span>
          </div>
        </FieldRow>
        <FieldRow :label="t('appearance.schemeSource')">
          <span class="hint">{{ schemeSourceLabel }}</span>
        </FieldRow>
      </SettingCard>
    </div>
  </div>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-4);
  align-content: start;
}

@media (max-width: 980px) {
  .grid {
    grid-template-columns: 1fr;
  }
}

.swatches {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-3);
}

.swatch {
  width: 40px;
  height: 40px;
  border-radius: var(--r-full);
  border: 1px solid var(--outline-variant);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  font-size: var(--fs-title);
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.45);
  transition: transform var(--motion-short) var(--ease-standard);
}

.swatch:hover {
  transform: translateY(-2px);
}

.swatch.is-active {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}

.swatches__custom {
  margin-left: var(--sp-2);
  padding-left: var(--sp-3);
  border-left: 1px solid var(--outline-variant);
}

.divider {
  height: 1px;
  background: var(--outline-variant);
}

.slider-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  width: 220px;
  max-width: 40vw;
}

.slider-row__slider {
  flex: 1;
  min-width: 0;
}

.slider-row__value {
  width: 48px;
  text-align: right;
  color: var(--text-secondary);
}
</style>
