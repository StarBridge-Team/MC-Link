<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { showError, showSuccess } from "../../composables/useToast";
import SettingCard from "../../components/ui/SettingCard.vue";
import SettingsManifestSection from "./SettingsManifestSection.vue";
import { useDisplayNames } from "../../composables/useDisplayNames";
import { useSetup } from "../../composables/useSetup";
import { KEYS, local } from "../../lib/persist";

/**
 * 个性化 → 通用（语言 / 地区 / 玩家名 / 服务器下发的更多设置）。
 *
 * 语言与地区的可选清单**以后端返回为准**（`setup.rs` 的 `SUPPORTED_LANGUAGES` /
 * `SUPPORTED_REGIONS`），界面不硬编码；显示名取不到时回退成原始代码，
 * 因此后端新增语言/地区不会让界面崩。
 */
const { t } = useI18n();
const { regionLabel, languageLabel } = useDisplayNames();
const setup = useSetup();

const language = ref("");
const region = ref("");
const savingLocale = ref(false);

const playerName = ref(local.getString(KEYS.playerName));

const languageOptions = computed(() =>
  (setup.state.value?.supported_languages ?? []).map((code) => ({
    label: languageLabel(code),
    value: code,
  })),
);

const regionOptions = computed(() =>
  (setup.state.value?.supported_regions ?? []).map((code) => ({
    label: regionLabel(code),
    value: code,
  })),
);

onMounted(async () => {
  // `loadOnce`：引导状态在启动时已经读过一次，这里只补一次兜底请求。
  await setup.loadOnce();
  const state = setup.state.value;
  language.value = state?.language || state?.detected_language || "";
  region.value = state?.region || state?.detected_region || "";
});

/** 语言与地区必须一起提交：命令的签名就是 `(language, region)`。 */
async function persistLocale() {
  if (!language.value || !region.value) return;
  savingLocale.value = true;
  try {
    await setup.chooseLanguage(language.value, region.value);
    showSuccess(t("common.saved"));
  } catch (e) {
    showError(`${t("common.saveFailed")}: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    savingLocale.value = false;
  }
}

function savePlayerName() {
  local.setString(KEYS.playerName, playerName.value.trim());
  showSuccess(t("common.saved"));
}
</script>

<template>
  <div class="scroll-area">
    <div class="grid">
      <SettingCard
        :icon="'bi bi-translate'"
        :title="t('general.title')"
        :desc="t('general.desc')"
        wide
      >
        <div class="locale">
          <div class="locale__field">
            <span class="field-label">{{ t("general.language") }}</span>
            <var-select
              v-model="language"
              :options="languageOptions"
              :placeholder="t('general.language')"
              @update:model-value="persistLocale"
            />
          </div>
          <div class="locale__field">
            <span class="field-label">{{ t("general.region") }}</span>
            <var-select
              v-model="region"
              :options="regionOptions"
              :placeholder="t('general.region')"
              @update:model-value="persistLocale"
            />
          </div>
          <var-button type="primary" :loading="savingLocale" @click="persistLocale">
            {{ t("common.save") }}
          </var-button>
        </div>
        <p class="hint">{{ t("general.regionHint") }}</p>
      </SettingCard>

      <SettingCard :icon="'bi bi-person-badge'" :title="t('general.playerTitle')">
        <div class="player">
          <var-input
            v-model="playerName"
            class="player__input"
            :placeholder="t('general.playerNamePlaceholder')"
            :maxlength="32"
          />
          <var-button type="primary" @click="savePlayerName">{{ t("common.save") }}</var-button>
        </div>
      </SettingCard>

      <!-- 服务器没有下发任何分区时，这个组件本身不渲染，不会留下空卡片 -->
      <SettingsManifestSection />
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

.locale {
  display: flex;
  align-items: flex-end;
  gap: var(--sp-3);
  flex-wrap: wrap;
}

.locale__field {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
  min-width: 200px;
}

.player {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  max-width: 480px;
}

.player__input {
  flex: 1;
  min-width: 0;
}
</style>
