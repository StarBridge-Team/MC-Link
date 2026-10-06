<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import FieldRow from "../ui/FieldRow.vue";
import SettingCard from "../ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";
import type { PersonalizationSettings } from "../../lib/api/types";

/**
 * 个性化 · 背景模糊（分深浅两档）。
 *
 * 一次只有一个载体生效，所以只显示对应的那一对滑块：
 *   图片 / 视频 → 糊媒体层；default 背景 → 糊它透出的那一层（材质或调色板背景）。
 * 三对滑块共用同一套模板（字段名按载体拼出来），避免复制三遍而漏改。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

type BlurKey =
  | "background_image_blur_light"
  | "background_image_blur_dark"
  | "background_video_blur_light"
  | "background_video_blur_dark"
  | "seed_blur_light"
  | "seed_blur_dark";

/** 当前背景的载体：决定显示哪一个模糊滑块（一次只可能有一类生效）。 */
const blurKind = computed<"image" | "video" | "seed" | null>(() => {
  if (state.background_type === "image" && state.background_value) return "image";
  if (state.background_type === "video" && state.background_value) return "video";
  // default = 透出材质（或选「无」时露出调色板背景层）；纯色糊了还是同一个色，无意义。
  if (state.background_type === "default") return "seed";
  return null;
});

/**
 * 三类模糊的字段名都遵循 `<前缀>_blur_<档位>`。
 *
 * `seed` 那一对不带 `background_` 前缀（历史字段名），其余两类带，见后端
 * `config::PersonalizationSettings`。
 */
function blurPrefix(): "background_image" | "background_video" | "seed" {
  const kind = blurKind.value ?? "image";
  return kind === "seed" ? "seed" : `background_${kind}`;
}

function blurKey(level: "light" | "dark"): BlurKey {
  return `${blurPrefix()}_blur_${level}` as BlurKey;
}

function blurValue(level: "light" | "dark"): number {
  return state[blurKey(level)] ?? 0;
}

/** 模糊上限与 `background.ts` 的 `BLUR_MAX` 一致；滑块的调节区间到此为止。 */
const BLUR_MAX = 40;

function setBlur(level: "light" | "dark", e: Event) {
  const value = (e.target as HTMLElement & { value?: number }).value;
  const px = typeof value === "number" && Number.isFinite(value) ? value : 0;
  const key = blurKey(level);
  settings.patch(
    { [key]: Math.min(BLUR_MAX, Math.max(0, Math.round(px))) } as Partial<PersonalizationSettings>,
    { silent: true },
  );
}

function commit() {
  void settings.saveNow();
}
</script>

<template>
  <SettingCard
    :icon="'blur_on'"
    :title="t('background.blur')"
    :desc="t('background.blurDesc')"
  >
    <template v-if="blurKind">
      <p class="hint">
        {{
          blurKind === "image"
            ? t("background.blurTargetImage")
            : blurKind === "video"
              ? t("background.blurTargetVideo")
              : t("background.blurTargetSeed")
        }}
      </p>
      <FieldRow :label="t('background.blurLight')">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="0" :max="BLUR_MAX" :step="1">
            <m3e-slider-thumb
              :value="blurValue('light')"
              @input="setBlur('light', $event)"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">{{ Math.round(blurValue("light")) }}px</span>
        </div>
      </FieldRow>
      <FieldRow :label="t('background.blurDark')">
        <div class="slider-row">
          <m3e-slider class="slider-row__slider" :min="0" :max="BLUR_MAX" :step="1">
            <m3e-slider-thumb
              :value="blurValue('dark')"
              @input="setBlur('dark', $event)"
              @change="commit"
            />
          </m3e-slider>
          <span class="slider-row__value mono">{{ Math.round(blurValue("dark")) }}px</span>
        </div>
      </FieldRow>
    </template>
    <p v-else class="hint">{{ t("background.blurNa") }}</p>
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
