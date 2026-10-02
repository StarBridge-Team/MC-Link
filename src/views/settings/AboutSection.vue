<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import { version as vueVersion } from "vue";
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

/** 版本信息磁贴：把散落的键值收敛成一张网格。 */
const stats = computed(() => [
  { key: "version", label: t("about.version"), value: appVersion.value || "—" },
  { key: "tauri", label: t("about.tauriVersion"), value: tauriVer.value || "—" },
  { key: "vue", label: t("about.vueVersion"), value: vueVersion },
  { key: "install", label: t("about.installForm"), value: installModeText.value },
]);

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
    <div class="about">
      <!-- Hero：品牌标识 + 名称 + 版本 + 外链动作 -->
      <m3e-card variant="elevated" class="hero">
        <div slot="content" class="hero__inner">
          <span class="hero__logo"><i class="material-symbols-rounded">category</i></span>
          <div class="hero__text">
            <h2 class="hero__name">{{ t("app.name") }}</h2>
            <p class="hero__tagline">{{ t("app.tagline") }}</p>
            <span class="hero__chip mono">{{ appVersion || "—" }}</span>
          </div>
          <div class="hero__actions">
            <m3e-button variant="tonal" @click="open(REPO_URL)">
              <m3e-icon slot="icon" name="code" />
              <span>{{ t("about.repo") }}</span>
            </m3e-button>
            <m3e-button variant="tonal" @click="open(WEBSITE_URL)">
              <m3e-icon slot="icon" name="public" />
              <span>{{ t("about.website") }}</span>
            </m3e-button>
          </div>
        </div>
      </m3e-card>

      <!-- 版本信息：网格磁贴 -->
      <SettingCard :icon="'badge'" :title="t('about.buildInfo')" wide>
        <div class="stats">
          <div v-for="s in stats" :key="s.key" class="stat">
            <span class="stat__label">{{ s.label }}</span>
            <span class="stat__value mono">{{ s.value }}</span>
          </div>
        </div>
        <div class="paths">
          <div v-if="runtime?.exe_path" class="path-row">
            <span class="stat__label">{{ t("about.exePath") }}</span>
            <span class="path selectable mono">{{ runtime.exe_path }}</span>
          </div>
          <div v-if="runtime?.data_dir" class="path-row">
            <span class="stat__label">{{ t("about.dataDir") }}</span>
            <span class="path selectable mono">{{ runtime.data_dir }}</span>
          </div>
        </div>
        <div class="actions">
          <m3e-button @click="resetOobe">
            <m3e-icon slot="icon" name="restart_alt" />
            <span>{{ t("oobe.resetHint") }}</span>
          </m3e-button>
        </div>
      </SettingCard>

      <!-- 贡献者：头像墙 -->
      <SettingCard :icon="'group'" :title="t('about.contributors')" wide>
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
            <m3e-avatar class="person__avatar">
              <img :src="person.avatar_url" :alt="person.login" />
            </m3e-avatar>
            <span class="person__name ellipsis">{{ person.login }}</span>
          </button>
        </div>
      </SettingCard>

      <!-- 议题 -->
      <SettingCard :icon="'forum'" :title="t('about.issues')" wide>
        <p v-if="issues.length === 0" class="hint">{{ t("about.noIssues") }}</p>
        <div v-else class="issues">
          <button
            v-for="issue in issues"
            :key="issue.number"
            class="issue"
            type="button"
            @click="open(issue.html_url)"
          >
            <m3e-icon name="bug_report" class="issue__icon" />
            <span class="issue__number mono">#{{ issue.number }}</span>
            <span class="issue__title ellipsis">{{ issue.title }}</span>
          </button>
        </div>
        <div class="actions">
          <m3e-button variant="tonal" @click="open(`${REPO_URL}/issues/new`)">
            <m3e-icon slot="icon" name="edit_square" />
            <span>{{ t("about.openIssue") }}</span>
          </m3e-button>
        </div>
      </SettingCard>
    </div>
  </div>
</template>

<style scoped>
.about {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  align-content: start;
}

/* ---------- Hero ---------- */
.hero__inner {
  display: flex;
  align-items: center;
  gap: var(--sp-5);
  flex-wrap: wrap;
}

.hero__logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 72px;
  height: 72px;
  flex-shrink: 0;
  border-radius: var(--r-xl);
  background: linear-gradient(
    135deg,
    var(--primary-container),
    color-mix(in srgb, var(--tertiary-container) 70%, var(--primary-container))
  );
  color: var(--on-primary-container);
  font-size: 40px;
}

.hero__text {
  flex: 1;
  min-width: 180px;
}

.hero__name {
  font-size: var(--fs-display);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  line-height: var(--lh-tight);
}

.hero__tagline {
  margin-top: 4px;
  font-size: var(--fs-body);
  color: var(--text-secondary);
}

.hero__chip {
  display: inline-flex;
  align-items: center;
  margin-top: var(--sp-3);
  height: 24px;
  padding: 0 var(--sp-3);
  border-radius: var(--r-full);
  background: var(--secondary-container);
  color: var(--on-secondary-container);
  font-size: var(--fs-xs);
}

.hero__actions {
  display: flex;
  gap: var(--sp-2);
  flex-shrink: 0;
}

/* ---------- 版本信息 ---------- */
.stats {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: var(--sp-3);
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: var(--sp-3) var(--sp-4);
  border-radius: var(--r-md);
  background: var(--surface-container);
}

.stat__label {
  font-size: var(--fs-label);
  color: var(--text-muted);
}

.stat__value {
  font-size: var(--fs-body);
  color: var(--text-primary);
  font-weight: var(--fw-medium);
}

.paths {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

.path-row {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.path {
  max-width: 100%;
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

/* ---------- 贡献者 ---------- */
.people {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
}

.person {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  max-width: 200px;
  padding: var(--sp-1) var(--sp-2) var(--sp-1) var(--sp-1);
  border-radius: var(--r-full);
  background: var(--surface-container);
  color: var(--text-secondary);
  font-size: var(--fs-label);
  transition: background-color var(--motion-short) var(--ease-standard),
    color var(--motion-short) var(--ease-standard);
}

.person:hover {
  background: var(--secondary-container);
  color: var(--on-secondary-container);
}

.person__avatar {
  --m3e-avatar-size: 28px;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
}

.person__name {
  min-width: 0;
}

/* ---------- 议题 ---------- */
.issues {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.issue {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  height: 40px;
  padding: 0 var(--sp-3);
  border-radius: var(--r-sm);
  color: var(--text-secondary);
  font-size: var(--fs-base);
  text-align: left;
  transition: background-color var(--motion-short) var(--ease-standard),
    color var(--motion-short) var(--ease-standard);
}

.issue:hover {
  background: color-mix(in srgb, var(--on-surface) 8%, transparent);
  color: var(--text-primary);
}

.issue__icon {
  flex-shrink: 0;
  font-size: var(--fs-title);
  color: var(--text-muted);
}

.issue__number {
  flex-shrink: 0;
  color: var(--text-muted);
}

.issue__title {
  min-width: 0;
}
</style>
