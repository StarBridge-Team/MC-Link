<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import ChipSelect, { type ChipOption } from "../../components/ui/ChipSelect.vue";
import InfoBar from "../../components/ui/InfoBar.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { useSettings } from "../../composables/useSettings";

/**
 * 个性化 → 首页。
 *
 * 这一项此前是"界面可见、能保存、但渲染层从未消费"的半成品；现在首页
 * （`HomeView.vue`）真的按 `homepage_mode` 渲染，这里负责写入。
 */
const { t } = useI18n();
const settings = useSettings();
const state = settings.state;

const urlInput = ref("");
const urlError = ref(false);

const modeOptions = computed<ChipOption<string>[]>(() => [
  { value: "default", label: t("homepage.modeDefault") },
  { value: "blank", label: t("homepage.modeBlank") },
  { value: "webpage", label: t("homepage.modeWebpage") },
]);

const modeHint = computed(() => {
  switch (state.homepage_mode) {
    case "blank":
      return t("homepage.modeBlankHint");
    case "webpage":
      return t("homepage.modeWebpageHint");
    default:
      return t("homepage.modeDefaultHint");
  }
});

watch(
  () => state.homepage_value,
  (value) => {
    urlInput.value = value ?? "";
  },
  { immediate: true },
);

function setMode(value: string) {
  settings.patch({ homepage_mode: value });
}

function applyUrl() {
  const url = urlInput.value.trim();
  if (!/^https?:\/\//i.test(url)) {
    urlError.value = true;
    return;
  }
  urlError.value = false;
  settings.patch({ homepage_mode: "webpage", homepage_value: url });
}

function clearUrl() {
  urlInput.value = "";
  urlError.value = false;
  settings.patch({ homepage_value: "" });
}
</script>

<template>
  <div class="scroll-area">
    <div class="grid stagger">
      <SettingCard :icon="'home'" :title="t('homepage.title')" :desc="t('homepage.desc')" wide>
        <div class="field-label">{{ t("homepage.mode") }}</div>
        <ChipSelect :model-value="state.homepage_mode" :options="modeOptions" @update:model-value="setMode" />
        <p class="hint">{{ modeHint }}</p>

        <template v-if="state.homepage_mode === 'webpage'">
          <div class="field-label">{{ t("homepage.url") }}</div>
          <div class="url-row" @keyup.enter="applyUrl">
            <var-input
              v-model="urlInput"
              class="url-row__input"
              :placeholder="t('homepage.urlPlaceholder')"
            />
            <var-button type="primary" @click="applyUrl">{{ t("common.apply") }}</var-button>
            <var-button text @click="clearUrl">{{ t("common.clear") }}</var-button>
          </div>
          <InfoBar v-if="urlError" kind="danger" :text="t('homepage.urlInvalid')" />
        </template>
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

.url-row {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  max-width: 640px;
}

.url-row__input {
  flex: 1;
  min-width: 0;
}
</style>
