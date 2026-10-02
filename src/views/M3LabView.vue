<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../components/ui/ChipSelect.vue";
import ColorField from "../components/ui/ColorField.vue";
import EmptyState from "../components/ui/EmptyState.vue";
import FieldRow from "../components/ui/FieldRow.vue";
import SettingCard from "../components/ui/SettingCard.vue";
import { showSuccess } from "../composables/useToast";
import { useSettings } from "../composables/useSettings";
import { exportM3Scheme, generateM3SchemeSynced } from "../lib/m3/m3Client";
import { M3_CONTRAST, M3_PRESET_SEEDS, M3_VARIANTS, M3_VARIANT_LABEL_KEY } from "../lib/m3/presets";
import type { M3Roles, M3Scheme, M3Variant } from "../lib/m3/types";

/**
 * 配色实验室（`/m3`）。
 *
 * 纯**预览**工具：用任意种子色 + 变体 + 对比度生成一套 M3 配色，把调色板与角色
 * 展示出来，并可复制 JSON。**它不再写进应用主题**——应用配色由组件库（Varlet 的
 * MD3 主题）生成，见 `lib/theme.ts`。这里只是个配色生成器，方便挑颜色。
 *
 * 方案优先由后端 `generate_m3_scheme`（成熟 crate）计算；非 Tauri 环境或调用失败时
 * 回退到前端镜像引擎，并在界面上标出来源。
 */
const { t } = useI18n();
const settings = useSettings();

// 本地草稿，不写进应用主题、也不写盘：切换这些值只是重算下面的预览。
const seed = ref(settings.state.theme_color || "#0066cc");
const variant = ref<M3Variant>("tonal_spot");
const contrast = ref(0);
const scheme = ref<M3Scheme | null>(null);
const schemeSource = ref<"backend" | "frontend">("frontend");

const variantOptions = computed<ChipOption<M3Variant>[]>(() =>
  M3_VARIANTS.map((v) => ({
    value: v,
    label: t(`m3.variantNames.${M3_VARIANT_LABEL_KEY[v]}`),
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

async function regenerate() {
  const { scheme: next, source } = await generateM3SchemeSynced(seed.value, {
    variant: variant.value,
    contrast: contrast.value,
  });
  scheme.value = next;
  schemeSource.value = source;
}

function setSeed(value: string) {
  seed.value = value;
}

function setVariant(value: M3Variant) {
  variant.value = value;
}

function setContrast(value: number | number[]) {
  const n = Array.isArray(value) ? value[0] : value;
  contrast.value = typeof n === "number" ? n : 0;
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

onMounted(regenerate);
// 任意参数变了就重算预览；用 deep 是因为 contrast 是标量、无需 deep，但统一监听省事。
watch([seed, variant, contrast], () => void regenerate());
</script>

<template>
  <div class="lab">
    <div class="lab__panel">
      <SettingCard :icon="'palette'" :title="t('m3.title')" :desc="t('m3.desc')" wide>
        <div class="field-label">{{ t("m3.seed") }}</div>
        <div class="seeds">
          <button
            v-for="s in M3_PRESET_SEEDS"
            :key="s"
            class="seed"
            :class="{ 'is-active': seed.toLowerCase() === s.toLowerCase() }"
            type="button"
            :style="{ background: s }"
            :title="s"
            @click="setSeed(s)"
          />
          <span class="seeds__custom">
            <ColorField :model-value="seed" size="large" @update:model-value="setSeed" />
          </span>
        </div>

        <div class="field-label">{{ t("m3.variant") }}</div>
        <ChipSelect
          small
          :model-value="variant"
          :options="variantOptions"
          @update:model-value="setVariant"
        />

        <FieldRow :label="t('m3.contrast')">
          <div class="slider-row">
            <var-slider
              class="slider-row__slider"
              :model-value="contrast"
              :min="M3_CONTRAST.min"
              :max="M3_CONTRAST.max"
              :step="M3_CONTRAST.step"
              @update:model-value="setContrast"
            />
            <span class="slider-row__value mono">{{ contrast.toFixed(2) }}</span>
          </div>
        </FieldRow>

        <FieldRow :label="t('m3.source')">
          <span class="hint">
            {{ schemeSource === "backend"
              ? t("appearance.schemeSourceBackend")
              : t("appearance.schemeSourceFrontend") }}
          </span>
        </FieldRow>

        <div class="actions">
          <var-button size="small" text :disabled="!scheme" @click="copyScheme">
            <i class="material-symbols-rounded">clipboard</i>
            <span>{{ t("m3.copyJson") }}</span>
          </var-button>
        </div>
      </SettingCard>
    </div>

    <div class="lab__panel lab__panel--wide">
      <EmptyState
        v-if="!scheme"
        icon="hourglass_top"
        :title="t('common.loading')"
      />
      <template v-else>
        <SettingCard :icon="'view_column'" :title="t('m3.palette')" wide>
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

        <SettingCard :icon="'checklist'" :title="t('m3.roles')" wide>
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
