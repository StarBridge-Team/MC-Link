<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import SettingCard from "../ui/SettingCard.vue";
import InfoBar from "../ui/InfoBar.vue";
import { showError, showSuccess } from "../../composables/useToast";
import { useDisplayNames } from "../../composables/useDisplayNames";
import { useSetup } from "../../composables/useSetup";
import { errorMessage } from "../../ipc/client";
import { useAccountStore } from "../../stores/account";

/**
 * 个性化 · 语言与地区 / 玩家名。
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

/**
 * 玩家名现在由**后端账号服务**持有（`Setting/account.yml` + 离线 UUID）。
 * 这里只负责编辑与提交，不再直接写 localStorage。
 */
const account = useAccountStore();
const playerName = ref("");

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

  // `load()` 内含一次性迁移：老版本只把玩家名存在 localStorage，这里注册进后端。
  playerName.value = (await account.load())?.name ?? "";
});

/**
 * 语言与地区必须成对提交：后端命令的签名就是 `(language, region)`。
 *
 * 缺一个时不能静默返回——用户会以为保存成功了，重启后回退。
 */
const LOCALE_INCOMPLETE = "语言与地区必须成对提交：请先补全缺失的一项";

/** 缺字段时持续提示（InfoBar），不只靠一次性 toast。 */
const localeIncomplete = computed(() => !language.value || !region.value);

/** 语言与地区必须一起提交：命令的签名就是 `(language, region)`。 */
async function persistLocale() {
  if (!language.value || !region.value) {
    // 缺字段时**必须给反馈**：此前直接 return，用户以为存上了，重启回退。
    showError(LOCALE_INCOMPLETE);
    return;
  }
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

async function savePlayerName() {
  try {
    // 改名会换 UUID（离线模式的身份就是名字的哈希），因此只在用户点保存时提交。
    await account.register(playerName.value);
    showSuccess(t("common.saved"));
  } catch (e) {
    showError(`${t("common.saveFailed")}: ${errorMessage(e)}`);
  }
}

// m3e-select 的选中值经 `change` 事件回传，值本身仍是字符串代码。
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
  <SettingCard :icon="'translate'" :title="t('general.title')" :desc="t('general.desc')" wide>
    <div class="locale">
      <!-- 必须成对提交：缺一个就没法调后端命令，明确说出来而不是静默吞掉 -->
      <InfoBar v-if="localeIncomplete" kind="warning" :text="LOCALE_INCOMPLETE" />
      <!--
        选择器必须包在 `m3e-form-field` 里。
        裸 `<m3e-select>` 没有表单字段容器提供宽度与排版上下文，实测只渲染出右侧箭头、
        选中值不显示——组件文档里的每个示例也都是 form-field + label 的组合。
      -->
      <m3e-form-field variant="outlined" class="locale__field">
        <label slot="label" for="lang-select">{{ t("general.language") }}</label>
        <m3e-select
          id="lang-select"
          panel-class="app-select-panel"
          :value="language"
          @change="onLanguageChange"
        >
          <m3e-option v-for="opt in languageOptions" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </m3e-option>
        </m3e-select>
      </m3e-form-field>

      <m3e-form-field variant="outlined" class="locale__field">
        <label slot="label" for="region-select">{{ t("general.region") }}</label>
        <m3e-select
          id="region-select"
          panel-class="app-select-panel"
          :value="region"
          @change="onRegionChange"
        >
          <m3e-option v-for="opt in regionOptions" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </m3e-option>
        </m3e-select>
      </m3e-form-field>

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
    <!-- 离线模式的身份就是 UUID：显示出来便于排查"对方认不出我"这类问题 -->
    <p v-if="account.account?.uuid" class="player__uuid">
      {{ t("general.offlineUuid") }} · <span class="mono">{{ account.account.uuid }}</span>
    </p>
  </SettingCard>
</template>

<style scoped>
.locale {
  display: flex;
  align-items: flex-end;
  gap: var(--sp-3);
  flex-wrap: wrap;
}

/* 宽度由这里给：`m3e-form-field` 自身是行内尺寸，不撑开就只剩一个箭头。 */
.locale__field {
  min-width: 220px;
}

/* 输入框容器与托盘面板同档圆角（30px）。
 *
 * 两个令牌都要给：`--m3e-form-field-container-shape` 是通用容器，
 * `--m3e-outlined-form-field-container-shape` 是 outlined 变体专用。
 * 只给后者时，标签缺口（notch）那一侧的角不受它控制，会出现"只有一半圆角"。 */
.locale__field,
.player__input {
  --m3e-form-field-container-shape: 30px;
  --m3e-outlined-form-field-container-shape: 30px;
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

.player__uuid {
  margin: var(--sp-2) 0 0;
  font-size: var(--fs-xs);
  color: var(--text-muted);
  word-break: break-all;
}

.mono {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
}
</style>
