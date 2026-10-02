<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../ui/ChipSelect.vue";
import FieldRow from "../ui/FieldRow.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";

/**
 * 个性化 · 外观：明暗模式、窗口材质、界面动画。
 *
 * 颜色由组件库（@m3e/web 的动态配色）生成，这里不提供自定义配色。
 * 全部改动经 `useSettings().patch()`（唯一写入口，立即生效 + 防抖落盘）。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

const modeOptions = computed<ChipOption<string>[]>(() => [
  { value: "system", label: t("appearance.modeSystem"), icon: "contrast" },
  { value: "light", label: t("appearance.modeLight"), icon: "light_mode" },
  { value: "dark", label: t("appearance.modeDark"), icon: "dark_mode" },
]);

// 后端只认 mica / acrylic / hud_window，"无"即清空材质。
// `transparent` 是历史取值，与 "none" 等效，这里不单独提供选项。
const effectOptions = computed<ChipOption<string>[]>(() => [
  { value: "none", label: t("appearance.effectNone"), icon: "block" },
  { value: "mica", label: t("appearance.effectMica"), icon: "window" },
  { value: "acrylic", label: t("appearance.effectAcrylic"), icon: "blur_on" },
  { value: "hud_window", label: t("appearance.effectHud"), icon: "side_navigation" },
]);

function setMode(value: string) {
  settings.patch({ theme_mode: value });
}

function setEffect(value: string) {
  settings.patch({ transparent_effect: value });
}

// m3e 控件的值经原生事件回传，这里统一收敛成配置需要的标量。
function onAnimationToggle(e: Event) {
  settings.patch({ animation_enabled: (e.target as HTMLInputElement).checked });
}

function onAnimationSpeed(e: Event) {
  const value = (e.target as HTMLElement & { value?: number }).value;
  settings.patch({ animation_speed: typeof value === "number" ? value : 1 });
}
</script>

<template>
  <SettingCard :icon="'contrast'" :title="t('appearance.title')" :desc="t('appearance.desc')" wide>
    <div class="field-label">{{ t("appearance.mode") }}</div>
    <ChipSelect :model-value="state.theme_mode" :options="modeOptions" @update:model-value="setMode" />
  </SettingCard>

  <SettingCard :icon="'blur_on'" :title="t('appearance.effect')" :desc="t('appearance.effectDesc')" wide>
    <ChipSelect
      :model-value="settings.effect.value"
      :options="effectOptions"
      @update:model-value="setEffect"
    />
  </SettingCard>

  <SettingCard
    :icon="'auto_awesome'"
    :title="t('appearance.animation')"
    :desc="t('appearance.animationDesc')"
    wide
  >
    <FieldRow :label="t('appearance.animation')">
      <m3e-switch :checked="state.animation_enabled" @change="onAnimationToggle" />
    </FieldRow>
    <FieldRow v-if="state.animation_enabled" :label="t('appearance.animationSpeed')">
      <div class="slider-row">
        <m3e-slider
          class="slider-row__slider"
          :min="0.25"
          :max="2"
          :step="0.25"
          @change="onAnimationSpeed"
        >
          <m3e-slider-thumb :value="state.animation_speed" />
        </m3e-slider>
        <span class="slider-row__value mono">{{ state.animation_speed }}x</span>
      </div>
    </FieldRow>
  </SettingCard>
</template>

<style scoped>
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
