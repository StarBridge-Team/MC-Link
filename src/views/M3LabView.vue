<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../components/ui/ChipSelect.vue";
import ColorField from "../components/ui/ColorField.vue";
import EmptyState from "../components/ui/EmptyState.vue";
import FieldRow from "../components/ui/FieldRow.vue";
import SettingCard from "../components/ui/SettingCard.vue";
import { showSuccess } from "../composables/useToast";
import { useSettings } from "../composables/useSettings";
import { exportM3Scheme } from "../lib/m3/m3Client";
import { M3_CONTRAST, M3_PRESET_SEEDS, M3_VARIANTS, M3_VARIANT_LABEL_KEY } from "../lib/m3/presets";
import type { M3Roles, M3Variant } from "../lib/m3/types";

/**
 * 配色实验室（`/m3`）。
 *
 * # 没有"预览态"
 *
 * 这里的控件直接改**当前生效**的配色（`useSettings`），而不是维护一份草稿再"应用"。
 * 理由：配色是全局观感，用户想看的就是"整个界面换成这样是什么效果"，
 * 一份只在本页生效的草稿反而会让人误判。所有改动都可随时在同样位置改回去。
 *
 * # 与后端的接口同步
 *
 * 方案优先由后端 `generate_m3_scheme`（成熟 crate）计算；非 Tauri 环境或调用失败时
 * 回退到前端镜像引擎，并在界面上标出来源——"看起来一样"和"算得一样"是两件事。
 */
const { t } = useI18n();
const settings = useSettings();

const scheme = computed(() => settings.scheme.value);

const variantOptions = computed<ChipOption<M3Variant>[]>(() =>
  M3_VARIANTS.map((variant) => ({
    value: variant,
    label: t(`m3.variantNames.${M3_VARIANT_LABEL_KEY[variant]}`),
  })),
);

/** 调色板顺序即 M3 的六个色调色板。 */
const PALETTE_KEYS = [
  "primary",
  "secondary",
  "tertiary",
  "neutral",
  "neutral_variant",
  "error",
] as const;

const palettes = computed(() => {
  const current = scheme.value;
  if (!current) return [];
  return PALETTE_KEYS.map((key) => ({
    key,
    tones: Object.entries(current.palettes[key]?.tones ?? {})
      .map(([tone, color]) => ({ tone: Number(tone), color }))
      .sort((a, b) => a.tone - b.tone),
  }));
});

/** 角色展示用面板。只列常用角色，避免把 47 个字段全铺出来。 */
const ROLE_KEYS = [
  "primary",
  "on_primary",
  "primary_container",
  "on_primary_container",
  "secondary",
  "secondary_container",
  "tertiary",
  "tertiary_container",
  "error",
  "error_container",
  "surface",
  "on_surface",
  "surface_variant",
  "surface_container",
  "surface_container_high",
  "surface_container_highest",
  "outline",
  "outline_variant",
  "inverse_surface",
  "inverse_on_surface",
] as const;

const lightRoles = computed(() => roleRows(scheme.value?.light));
const darkRoles = computed(() => roleRows(scheme.value?.dark));

function roleRows(roles: M3Roles | undefined) {
  if (!roles) return [];
  return ROLE_KEYS.map((key) => ({
    key,
    label: t(`m3.role.${key}`),
    color: roles[key] ?? "",
  })).filter((row) => row.color !== "");
}

function setSeed(value: string) {
  settings.patch({ theme_color: value });
  showSuccess(t("m3.applied"));
}

function setVariant(value: M3Variant) {
  settings.patchM3({ variant: value });
}

function setContrast(value: number | number[]) {
  const n = Array.isArray(value) ? value[0] : value;
  settings.patchM3({ contrast: typeof n === "number" ? n : 0 });
}

async function copyScheme() {
  const current = scheme.value;
  if (!current) return;
  try {
    await navigator.clipboard.writeText(exportM3Scheme(current));
    showSuccess(t("m3.copied"));
  } catch (e) {
    console.warn("[m3] 写入剪贴板失败:", e);
  }
}
</script>

<template>
  <div class="lab">
    <div class="lab__panel">
      <SettingCard :icon="'bi bi-palette'" :title="t('m3.title')" :desc="t('m3.desc')" wide>
        <div class="field-label">{{ t("m3.seed") }}</div>
        <div class="seeds">
          <button
            v-for="seed in M3_PRESET_SEEDS"
            :key="seed"
            class="seed"
            :class="{ 'is-active': settings.state.theme_color.toLowerCase() === seed.toLowerCase() }"
            type="button"
            :style="{ background: seed }"
            :title="seed"
            @click="setSeed(seed)"
          />
          <span class="seeds__custom">
            <ColorField
              :model-value="settings.state.theme_color"
              size="large"
              @update:model-value="setSeed"
            />
          </span>
        </div>

        <div class="field-label">{{ t("m3.variant") }}</div>
        <ChipSelect
          small
          :model-value="settings.m3.variant"
          :options="variantOptions"
          @update:model-value="setVariant"
        />

        <FieldRow :label="t('m3.contrast')">
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

        <FieldRow :label="t('m3.source')">
          <span class="hint">
            {{ settings.schemeSource.value === "backend"
              ? t("appearance.schemeSourceBackend")
              : t("appearance.schemeSourceFrontend") }}
          </span>
        </FieldRow>

        <div class="actions">
          <var-button size="small" text :disabled="!scheme" @click="copyScheme">
            <i class="bi bi-clipboard" />
            <span>{{ t("m3.copyJson") }}</span>
          </var-button>
        </div>
      </SettingCard>
    </div>

    <div class="lab__panel lab__panel--wide">
      <EmptyState
        v-if="!scheme"
        icon="bi bi-hourglass-split"
        :title="t('common.loading')"
      />
      <template v-else>
        <SettingCard :icon="'bi bi-columns-gap'" :title="t('m3.palette')" wide>
          <div v-for="palette in palettes" :key="palette.key" class="ramp">
            <span class="ramp__label mono">{{ palette.key }}</span>
            <div class="ramp__tones">
              <span
                v-for="tone in palette.tones"
                :key="tone.tone"
                class="ramp__tone"
                :style="{ background: tone.color }"
                :title="`${tone.tone} · ${tone.color}`"
              />
            </div>
          </div>
        </SettingCard>

        <SettingCard :icon="'bi bi-list-check'" :title="t('m3.roles')" wide>
          <div class="roles">
            <div class="roles__column">
              <div class="field-label">{{ t("m3.rolesLight") }}</div>
              <div v-for="row in lightRoles" :key="row.key" class="role">
                <span class="role__chip" :style="{ background: row.color }" />
                <span class="role__label ellipsis">{{ row.label }}</span>
                <span class="role__hex mono">{{ row.color }}</span>
              </div>
            </div>
            <div class="roles__column">
              <div class="field-label">{{ t("m3.rolesDark") }}</div>
              <div v-for="row in darkRoles" :key="row.key" class="role">
                <span class="role__chip" :style="{ background: row.color }" />
                <span class="role__label ellipsis">{{ row.label }}</span>
                <span class="role__hex mono">{{ row.color }}</span>
              </div>
            </div>
          </div>
        </SettingCard>
      </template>
    </div>
  </div>
</template>

<style scoped>
.lab {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 360px minmax(0, 1fr);
  gap: var(--sp-4);
  padding: var(--sp-6);
  overflow: hidden;
}

@media (max-width: 1100px) {
  .lab {
    grid-template-columns: 1fr;
    grid-template-rows: auto minmax(0, 1fr);
  }
}

.lab__panel {
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.lab__panel--wide {
  gap: var(--sp-4);
}

.seeds {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
}

.seed {
  width: 32px;
  height: 32px;
  border-radius: var(--r-full);
  border: 1px solid var(--outline-variant);
}

.seed.is-active {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}

.seeds__custom {
  margin-left: var(--sp-2);
  padding-left: var(--sp-3);
  border-left: 1px solid var(--outline-variant);
}

.slider-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  width: 200px;
}

.slider-row__slider {
  flex: 1;
  min-width: 0;
}

.slider-row__value {
  width: 44px;
  text-align: right;
  color: var(--text-secondary);
}

.actions {
  display: flex;
  gap: var(--sp-2);
}

.ramp {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}

.ramp__label {
  width: 108px;
  flex-shrink: 0;
  color: var(--text-muted);
}

.ramp__tones {
  display: flex;
  flex: 1;
  min-width: 0;
  border-radius: var(--r-sm);
  overflow: hidden;
}

.ramp__tone {
  flex: 1;
  height: 32px;
}

.roles {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-5);
}

.roles__column {
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
  min-width: 0;
}

.role {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  min-width: 0;
}

.role__chip {
  width: 20px;
  height: 20px;
  flex-shrink: 0;
  border-radius: var(--r-xs);
  border: 1px solid var(--outline-variant);
}

.role__label {
  flex: 1;
  min-width: 0;
  font-size: var(--fs-label);
  color: var(--text-secondary);
}

.role__hex {
  flex-shrink: 0;
  color: var(--text-muted);
}
</style>
