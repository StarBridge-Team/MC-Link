<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import EulaStep from "./steps/EulaStep.vue";
import GameStep from "./steps/GameStep.vue";
import LanguageStep from "./steps/LanguageStep.vue";
import { useSetup, type OobeStep } from "../../composables/useSetup";
import { showError } from "../../composables/useToast";

/**
 * 首次启动引导浮层。
 *
 * # 每一步都是一次真实写入
 *
 * 三步各自调用一个后端命令并立即落盘（语言/地区 → 同意记录 → 游戏选择），
 * 因此中途退出再进来会**接着上次的地方继续**——`pendingStep` 由后端返回的
 * `steps` 推导，前端不存"当前第几步"。
 *
 * # 最后一步是闸门
 *
 * 全部走完后才调 `complete_setup_command`：后端会校验语言 + 同意 + 游戏三者齐备，
 * 缺一即拒绝。所以界面无法"跳过中间步骤直接完成"，也就不需要在前端再写一套校验。
 */
const { t } = useI18n();
const setup = useSetup();

const step = ref<OobeStep>("language");
const language = ref("");
const region = ref("");
const gameId = ref("");
const busy = ref(false);

const STEPS: { id: OobeStep; labelKey: string; icon: string }[] = [
  { id: "language", labelKey: "oobe.stepLanguage", icon: "bi bi-translate" },
  { id: "eula", labelKey: "oobe.stepEula", icon: "bi bi-file-earmark-text" },
  { id: "game", labelKey: "oobe.stepGame", icon: "bi bi-controller" },
];

const stepIndex = computed(() => STEPS.findIndex((s) => s.id === step.value));

const languages = computed(() => setup.state.value?.supported_languages ?? []);
const regions = computed(() => setup.state.value?.supported_regions ?? []);

/** 上一步永远可回（已保存的步骤回退不会丢数据，重新走一遍即可）。 */
const canGoBack = computed(() => stepIndex.value > 0);

onMounted(() => {
  const state = setup.state.value;
  language.value = state?.language || state?.detected_language || "";
  region.value = state?.region || state?.detected_region || "";
  gameId.value = state?.game || "";
  step.value = setup.pendingStep.value ?? "language";
  if (step.value === "eula") void setup.loadLegal(language.value);
  if (step.value === "game") void setup.loadGames();
});

function goBack() {
  if (!canGoBack.value) return;
  step.value = STEPS[stepIndex.value - 1].id;
}

/** 前进：先执行当前步骤的写入，成功后才切到下一步。 */
async function goNext() {
  busy.value = true;
  try {
    if (step.value === "language") {
      if (!language.value || !region.value) return;
      await setup.chooseLanguage(language.value, region.value);
      void setup.loadLegal(language.value);
      step.value = "eula";
      return;
    }

    if (step.value === "eula") {
      await setup.agreeEula(language.value);
      void setup.loadGames();
      step.value = "game";
      return;
    }

    if (!gameId.value) return;
    await setup.chooseGame(gameId.value);
    await setup.finish(language.value, region.value);
  } catch (e) {
    showError(e instanceof Error ? e.message : String(e));
  } finally {
    busy.value = false;
  }
}

/** 语言步骤的"下一步"在未选齐时不可用。 */
const nextDisabled = computed(() => {
  if (busy.value || setup.saving.value) return true;
  if (step.value === "language") return !language.value || !region.value;
  if (step.value === "game") return !gameId.value;
  return false;
});
</script>

<template>
  <div class="oobe">
    <div class="oobe__card">
      <header class="oobe__head">
        <span class="oobe__logo"><i class="bi bi-boxes" /></span>
        <div>
          <h1 class="oobe__title">{{ t("oobe.title") }}</h1>
          <p class="oobe__subtitle">{{ t("oobe.subtitle") }}</p>
        </div>
      </header>

      <ol class="steps">
        <li
          v-for="(item, index) in STEPS"
          :key="item.id"
          class="steps__item"
          :class="{
            'is-active': index === stepIndex,
            'is-done': index < stepIndex,
          }"
        >
          <span class="steps__dot">
            <i :class="index < stepIndex ? 'bi bi-check-lg' : item.icon" />
          </span>
          <span class="steps__label">{{ t(item.labelKey) }}</span>
        </li>
      </ol>

      <div class="oobe__body">
        <LanguageStep
          v-if="step === 'language'"
          v-model:language="language"
          v-model:region="region"
          :languages="languages"
          :regions="regions"
        />

        <EulaStep
          v-else-if="step === 'eula'"
          :bundle="setup.legal.value"
          :loading="setup.legalLoading.value"
          @agree="goNext"
        />

        <GameStep
          v-else
          v-model:selected="gameId"
          :games="setup.games.value"
          :loading="setup.gamesLoading.value"
        />
      </div>

      <footer class="oobe__foot">
        <var-button text :disabled="!canGoBack" @click="goBack">
          <i class="bi bi-arrow-left" />
          <span>{{ t("oobe.back") }}</span>
        </var-button>
        <span class="grow" />
        <!-- EULA 步骤的确认按钮在正文旁边，这里只保留前进/完成 -->
        <var-button
          v-if="step !== 'eula'"
          type="primary"
          :disabled="nextDisabled"
          :loading="busy"
          @click="goNext"
        >
          {{ step === "game" ? t("oobe.finish") : t("oobe.next") }}
        </var-button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.oobe {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--sp-6);
  background: color-mix(in srgb, var(--scrim) 62%, transparent);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
}

.oobe__card {
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
  width: min(680px, 100%);
  max-height: 100%;
  padding: var(--sp-6);
  border-radius: var(--r-xl);
  background: var(--surface-container-high);
  box-shadow: var(--elevation-3);
  overflow: hidden;
}

.oobe__head {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
}

.oobe__logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  flex-shrink: 0;
  border-radius: var(--r-md);
  background: var(--primary-container);
  color: var(--on-primary-container);
  font-size: 24px;
}

.oobe__title {
  font-size: var(--fs-headline);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.oobe__subtitle {
  font-size: var(--fs-label);
  color: var(--text-muted);
}

.steps {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  margin: 0;
  padding: 0;
  list-style: none;
}

.steps__item {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  color: var(--text-muted);
  font-size: var(--fs-label);
}

.steps__dot {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: var(--r-full);
  background: var(--surface-container-highest);
  color: var(--text-secondary);
  font-size: var(--fs-label);
}

.steps__item.is-active {
  color: var(--text-primary);
  font-weight: var(--fw-semibold);
}

.steps__item.is-active .steps__dot {
  background: var(--primary);
  color: var(--on-primary);
}

.steps__item.is-done .steps__dot {
  background: var(--secondary-container);
  color: var(--on-secondary-container);
}

.oobe__body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.oobe__foot {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}
</style>
