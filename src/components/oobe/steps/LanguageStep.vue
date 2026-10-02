<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useDisplayNames } from "../../../composables/useDisplayNames";

/**
 * 引导第一步：语言与地区。
 *
 * 选项来自后端返回的清单（`supported_languages` / `supported_regions`），
 * 界面不硬编码；预选值用系统检测到的 `detected_*`，而不是写死 zh-CN。
 */
const props = defineProps<{
  languages: string[];
  regions: string[];
  language: string;
  region: string;
}>();

const emit = defineEmits<{
  "update:language": [value: string];
  "update:region": [value: string];
}>();

const { t } = useI18n();
const { regionLabel, languageLabel } = useDisplayNames();

const languageOptions = computed(() =>
  props.languages.map((code) => ({ label: languageLabel(code), value: code })),
);

const regionOptions = computed(() =>
  props.regions.map((code) => ({ label: regionLabel(code), value: code })),
);
</script>

<template>
  <div class="step">
    <p class="hint">{{ t("oobe.detectedHint") }}</p>

    <label class="field">
      <span class="field-label">{{ t("oobe.languageLabel") }}</span>
      <var-select
        :model-value="language"
        :options="languageOptions"
        @update:model-value="(v: string) => emit('update:language', String(v))"
      />
    </label>

    <label class="field">
      <span class="field-label">{{ t("oobe.regionLabel") }}</span>
      <var-select
        :model-value="region"
        :options="regionOptions"
        @update:model-value="(v: string) => emit('update:region', String(v))"
      />
    </label>

    <p class="hint">{{ t("oobe.regionHint") }}</p>
  </div>
</template>

<style scoped>
.step {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.field {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
</style>
