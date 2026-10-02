<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import { version as vueVersion } from "vue";
import FieldRow from "../../components/ui/FieldRow.vue";
import InfoBar from "../../components/ui/InfoBar.vue";
import SettingCard from "../../components/ui/SettingCard.vue";
import { confirmAction, showError, showSuccess } from "../../composables/useToast";
import { useSetup } from "../../composables/useSetup";
import { getAppVersion, getTauriVersion } from "../../lib/api/app";
import { fetchCommunity } from "../../lib/api/community";
import { getRuntimeInfo } from "../../lib/api/update";
import type { Community, RuntimeInfo } from "../../lib/api/types";

/**
 * 个性化 → 关于。
 *
 * 版本信息来自本地命令（不联网）；社区数据来自资源服务器的缓存（可能 `stale`）。
 * 两条链路互不阻塞：社区数据拉不到时，版本信息照常显示。
 */
const { t } = useI18n();
const setup = useSetup();

const appVersion = ref("");
const tauriVer = ref("");
const runtime = ref<RuntimeInfo | null>(null);
const community = ref<Community | null>(null);
/** 社区数据是否已经问过一次后端（区分"在加载"与"拉不到"）。 */
const communitySettled = ref(false);

const REPO_URL = "https://github.com/StarBridge-Team/MC-Link";
const WEBSITE_URL = "https://www.xigo.top/";

const installModeText = computed(() =>
  runtime.value?.install_mode === "portable"
    ? t("update.modePortable")
    : runtime.value?.install_mode === "installed"
      ? t("update.modeInstalled")
      : t("common.unknown"),
);

const contributors = computed(() => community.value?.contributors ?? []);
const issues = computed(() => community.value?.issues ?? []);

async function open(url: string) {
  try {
    await openUrl(url);
  } catch (e) {
    showError(String(e));
  }
}

/** 重新走一遍引导：清掉语言/同意记录/游戏选择，下次启动生效。 */
async function resetOobe() {
  const ok = await confirmAction({
    title: t("oobe.resetHint"),
    message: t("oobe.subtitle"),
    confirmText: t("common.confirm"),
    cancelText: t("common.cancel"),
  });
  if (!ok) return;
  try {
    await setup.reset();
    showSuccess(t("oobe.resetDone"));
  } catch (e) {
    showError(String(e));
  }
}

onMounted(async () => {
  // 三项独立，任一失败不影响其余展示。
  await Promise.all([
    getAppVersion().then((v) => (appVersion.value = v)).catch(() => undefined),
    getTauriVersion().then((v) => (tauriVer.value = v)).catch(() => undefined),
    getRuntimeInfo().then((v) => (runtime.value = v)).catch(() => undefined),
    fetchCommunity()
      .then((v) => (community.value = v))
      .catch((e) => console.warn("[about] 社区数据获取失败:", e))
      .finally(() => (communitySettled.value = true)),
  ]);
});
</script>

<template>
  <div class="scroll-area">
    <div class="grid stagger">
      <SettingCard :icon="'info'" :title="t('about.title')" wide>
        <div class="hero">
          <span class="hero__logo"><i class="material-symbols-rounded">category</i></span>
          <div>
            <h3 class="hero__name">{{ t("app.name") }}</h3>
            <p class="hint">{{ t("app.tagline") }}</p>
          </div>
          <span class="grow" />
          <var-button size="small" text @click="open(REPO_URL)">
            <i class="material-symbols-rounded">code</i>
            <span>{{ t("about.repo") }}</span>
          </var-button>
          <var-button size="small" text @click="open(WEBSITE_URL)">
            <i class="material-symbols-rounded">public</i>
            <span>{{ t("about.website") }}</span>
          </var-button>
        </div>

        <div class="rows">
          <FieldRow :label="t('about.version')">
            <span class="mono">{{ appVersion || "—" }}</span>
          </FieldRow>
          <FieldRow :label="t('about.tauriVersion')">
            <span class="mono">{{ tauriVer || "—" }}</span>
          </FieldRow>
          <FieldRow :label="t('about.vueVersion')">
            <span class="mono">{{ vueVersion }}</span>
          </FieldRow>
          <FieldRow :label="t('about.installForm')">
            <span>{{ installModeText }}</span>
          </FieldRow>
          <FieldRow v-if="runtime?.exe_path" :label="t('about.exePath')">
            <span class="mono path selectable">{{ runtime.exe_path }}</span>
          </FieldRow>
          <FieldRow v-if="runtime?.data_dir" :label="t('about.dataDir')">
            <span class="mono path selectable">{{ runtime.data_dir }}</span>
          </FieldRow>
        </div>

        <div class="actions">
          <var-button size="small" text @click="resetOobe">
            <i class="material-symbols-rounded">refresh</i>
            <span>{{ t("oobe.resetHint") }}</span>
          </var-button>
        </div>
      </SettingCard>

      <SettingCard :icon="'group'" :title="t('about.contributors')">
        <InfoBar v-if="community?.stale" kind="warning" :text="t('about.stale')" />
        <InfoBar v-if="community?.error" kind="danger" :text="`${t('about.communityError')}: ${community.error}`" />
        <!-- 拉取失败时不要一直显示"加载中"：那是在撒谎说请求还在进行 -->
        <InfoBar
          v-else-if="communitySettled && !community"
          kind="danger"
          :text="t('about.communityError')"
        />
        <p v-if="!communitySettled" class="hint">{{ t("common.loading") }}</p>
        <p v-else-if="contributors.length === 0 && community" class="hint">{{ t("common.empty") }}</p>
        <div v-else class="people">
          <button
            v-for="person in contributors"
            :key="person.login"
            class="person"
            type="button"
            :title="`${person.login} · ${person.contributions}`"
            @click="open(person.html_url)"
          >
            <var-avatar :size="32">
              <img :src="person.avatar_url" :alt="person.login" />
            </var-avatar>
            <span class="person__name ellipsis">{{ person.login }}</span>
          </button>
        </div>
      </SettingCard>

      <SettingCard :icon="'forum'" :title="t('about.issues')">
        <p v-if="issues.length === 0" class="hint">{{ t("about.noIssues") }}</p>
        <div v-else class="issues">
          <button
            v-for="issue in issues"
            :key="issue.number"
            class="issue"
            type="button"
            @click="open(issue.html_url)"
          >
            <span class="issue__number mono">#{{ issue.number }}</span>
            <span class="issue__title ellipsis">{{ issue.title }}</span>
          </button>
        </div>
        <div class="actions">
          <var-button size="small" text @click="open(`${REPO_URL}/issues/new`)">
            <i class="material-symbols-rounded">edit_square</i>
            <span>{{ t("about.openIssue") }}</span>
          </var-button>
        </div>
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

@media (max-width: 980px) {
  .grid {
    grid-template-columns: 1fr;
  }
}

.hero {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  flex-wrap: wrap;
}

.hero__logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: var(--r-md);
  background: var(--primary-container);
  color: var(--on-primary-container);
  font-size: 24px;
}

.hero__name {
  font-size: var(--fs-headline);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.rows {
  display: flex;
  flex-direction: column;
  max-width: 720px;
}

.rows :deep(.field-row + .field-row) {
  border-top: 1px solid var(--outline-variant);
}

.path {
  max-width: 460px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-secondary);
}

.actions {
  display: flex;
  gap: var(--sp-2);
  flex-wrap: wrap;
}

.people {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
}

.person {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  max-width: 180px;
  padding: var(--sp-1) var(--sp-2);
  border-radius: var(--r-full);
  color: var(--text-secondary);
  font-size: var(--fs-label);
}

.person:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.issues {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.issue {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  height: 32px;
  padding: 0 var(--sp-2);
  border-radius: var(--r-sm);
  color: var(--text-secondary);
  font-size: var(--fs-base);
  text-align: left;
}

.issue:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.issue__number {
  color: var(--text-muted);
  flex-shrink: 0;
}

.issue__title {
  min-width: 0;
}
</style>
