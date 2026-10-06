<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import FieldRow from "../ui/FieldRow.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";

/**
 * 个性化 · 背景不透明度与暗色遮罩。
 *
 * 这两项都是"在背景与内容之间加一层半透明"，只是颜色来源不同：
 *   背景不透明度 → 用背景自身的颜色变淡（露出下层：窗口材质或桌面）；
 *   暗色遮罩     → 叠一层黑，提升文字可读性。
 * 所以放同一张卡，但分两组、各有独立开关。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

/**
 * 背景不透明度只对"没有媒体载体"的背景有意义。
 *
 * default（透出材质）与 solid（自建纯色）下面是空的/单色，调 alpha 就是"让下层露出来"；
 * 图片与视频的载体是媒体本身，媒体下方还有一层 surface，调它的 alpha 等于给媒体蒙一层雾
 * 而不是让它变透 —— 那两类要的效果由模糊卡片负责。
 */
const opacityApplies = computed(
  () => state.background_type === "default" || state.background_type === "solid",
);

/** 遮罩只在真有背景内容时才谈得上"遮盖"；default 背景下面没有东西可遮。 */
const overlayApplies = computed(() => state.background_type !== "default");

type NumericKey =
  | "background_opacity"
  | "background_opacity_dark"
  | "background_overlay_opacity"
  | "background_overlay_opacity_dark";

/**
 * 拖动中只改观感，落盘交给松手。
 *
 * 这些滑块都是高频事件（拖动一次几十个），每个都触发一次磁盘写既没有必要、
 * 也会让文件写入频繁打断渲染。`saveNow()` 由 `m3e-slider-thumb` 的 `change` 触发。
 */
function setNumber(key: NumericKey, e: Event) {
  const value = (e.target as HTMLElement & { value?: number }).value;
  settings.patch({ [key]: clampNum(value) }, { silent: true });
}

function commit() {
  void settings.saveNow();
}

function onOverlayToggle(e: Event) {
  settings.patch({ background_overlay: (e.target as HTMLInputElement).checked });
}

/** 取不到值或非法值时给 0：NaN 一旦流进配置，滑块的 value 会永久错位。 */
function clampNum(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, Math.round(value)));
}
</script>

<template>
  <SettingCard
    :icon="'opacity'"
    :title="t('background.opacity')"
    :desc="t('background.opacityDesc')"
  >
    <template v-if="opacityApplies">
      <p class="hint">{{ t("background.opacityHint") }}</p>
      <FieldRow :label="t('background.opacityLight')">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="0" :max="100" :step="5">
            <m3e-slider-thumb
              :value="state.background_opacity"
              @input="setNumber('background_opacity', $event)"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">{{ Math.round(state.background_opacity) }}%</span>
        </div>
      </FieldRow>
      <FieldRow :label="t('background.opacityDark')">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="0" :max="100" :step="5">
            <m3e-slider-thumb
              :value="state.background_opacity_dark"
              @input="setNumber('background_opacity_dark', $event)"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">
            {{ Math.round(state.background_opacity_dark) }}%
          </span>
        </div>
      </FieldRow>
    </template>
    <p v-else class="hint">{{ t("background.opacityNa") }}</p>
  </SettingCard>

  <SettingCard :icon="'layers'" :title="t('background.overlay')" :desc="t('background.overlayDesc')">
    <FieldRow :label="t('background.overlay')">
      <m3e-switch
        :checked="state.background_overlay"
        :disabled="!overlayApplies"
        @change="onOverlayToggle"
      />
    </FieldRow>

    <!--
      遮罩强度分深浅两档：深色背景本来就更暗，同一个数值会更"糊"。两条并排显示
      （而不是跟着当前明暗只显示一条），用户才看得出它们是成对的，也便于对照着调。
    -->
    <template v-if="state.background_overlay && overlayApplies">
      <FieldRow :label="t('background.overlayOpacityLight')">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="0" :max="100" :step="5">
            <m3e-slider-thumb
              :value="state.background_overlay_opacity"
              @input="setNumber('background_overlay_opacity', $event)"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">
            {{ Math.round(state.background_overlay_opacity) }}%
          </span>
        </div>
      </FieldRow>
      <FieldRow :label="t('background.overlayOpacityDark')">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="0" :max="100" :step="5">
            <m3e-slider-thumb
              :value="state.background_overlay_opacity_dark"
              @input="setNumber('background_overlay_opacity_dark', $event)"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">
            {{ Math.round(state.background_overlay_opacity_dark) }}%
          </span>
        </div>
      </FieldRow>
    </template>
    <p v-if="!overlayApplies" class="hint">{{ t("background.overlayNa") }}</p>
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
