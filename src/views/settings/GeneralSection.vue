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

// m3e-select 的选中值经 `change` 事件回传（原生 CustomEvent），
// 值本身仍是字符串代码，这里读出来后再触发保存。
function selectValue(e: Event): string {
  const el = e.target as HTMLElement & { value?: string };
  return el.value ?? "";
}

function onLanguageChange(e: Event) {
  language.value = selectValue(e);
  void persistLocale();
}

function onRegionChange(e: Event) {
  region.value = selectValue(e);
  void persistLocale();
}
</script>

<template>
  <div class="scroll-area">
    <div class="grid stagger">
      <SettingCard
        :icon="'translate'"
        :title="t('general.title')"
        :desc="t('general.desc')"
        wide
      >
        <div class="locale">
          <div class="locale__field">
            <span class="field-label">{{ t("general.language") }}</span>
            <m3e-select :value="language" @change="onLanguageChange">
              <m3e-option v-for="opt in languageOptions" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </m3e-option>
            </m3e-select>
          </div>
          <div class="locale__field">
            <span class="field-label">{{ t("general.region") }}</span>
            <m3e-select :value="region" @change="onRegionChange">
              <m3e-option v-for="opt in regionOptions" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </m3e-option>
            </m3e-select>
          </div>
          <m3e-button variant="filled" :disabled="savingLocale" @click="persistLocale">
            {{ t("common.save") }}
          </m3e-button>
        </div>
        <p class="hint">{{ t("general.regionHint") }}</p>
      </SettingCard>

      <SettingCard :icon="'person'" :title="t('general.playerTitle')">
        <div class="player">
          <m3e-form-field variant="outlined" class="player__input">
            <label slot="label">{{ t("general.playerName") }}</label>
            <input v-model="playerName" maxlength="32" :placeholder="t('general.playerNamePlaceholder')" />
          </m3e-form-field>
          <m3e-button variant="filled" @click="savePlayerName">{{ t("common.save") }}</m3e-button>
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
