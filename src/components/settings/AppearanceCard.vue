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

/**
 * 当前材质是否支持"染色浓度"。
 *
 * **只有 Acrylic**：Windows 上 `window_vibrancy::apply_acrylic` 会接收一个 color
 * （`tint_color` 给的 alpha 就是它），而 `apply_mica` 只设
 * `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_MAINWINDOW` —— 材质浓淡完全由系统合成器决定，
 * 传什么进去都会被丢掉。
 *
 * macOS 的 hud_window 同理不受这个参数控制（材质状态由 `EffectState` 决定）。
 */
const tintSupported = computed(() => settings.effect.value === "acrylic");

// m3e 控件的值经原生事件回传，这里统一收敛成配置需要的标量。
function onAnimationToggle(e: Event) {
  settings.patch({ animation_enabled: (e.target as HTMLInputElement).checked });
}

function onAnimationSpeed(e: Event) {
  const value = (e.target as HTMLElement & { value?: number }).value;
  settings.patch({ animation_speed: typeof value === "number" ? value : 1 });
}

/** 材质浓度：拖动时 `silent`（只改观感 + 材质），松手才落盘。 */
function onEffectTint(e: Event) {
  const value = valueOf(e);
  settings.patch({ effect_tint: clampPercent(value) }, { silent: true });
}

function onEffectTintCommit() {
  void settings.saveNow();
}

/** 从原生事件里取数值；取不到时返回 0（而不是让 NaN 流进配置）。 */
function valueOf(e: Event): number {
  const value = (e.target as HTMLElement & { value?: number }).value;
  return typeof value === "number" ? value : 0;
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, Math.round(value)));
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
    <!--
      材质染色浓度：只对**真的支持染色**的材质显示。
      Windows 的 Mica 实现只设 `DWMWA_SYSTEMBACKDROP_TYPE`，不接受任何浓度参数，
      摆一个拖了没反应的滑块比不显示更让人困惑；Acrylic 才接受 color。
      「看得见多少材质」由背景卡片的「背景不透明度」负责，不是这里。
    -->
    <template v-if="tintSupported">
      <FieldRow :label="t('appearance.effectTint')">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="0" :max="100" :step="5">
            <m3e-slider-thumb
              :value="state.effect_tint"
              @input="onEffectTint"
              @change="onEffectTintCommit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">{{ Math.round(state.effect_tint) }}%</span>
        </div>
      </FieldRow>
      <p class="hint">{{ t("appearance.effectTintHint") }}</p>
    </template>
    <!-- Mica 不支持染色：明确告知，并把用户引到真正能调的那个旋钮上。 -->
    <p v-else-if="state.transparent_effect === 'mica'" class="hint">
      {{ t("appearance.effectTintMicaHint") }}
    </p>
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

.hint {
  font-size: var(--fs-label);
  color: var(--text-muted);
  line-height: var(--lh-normal);
}
</style>
