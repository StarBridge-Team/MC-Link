<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import EmptyState from "../components/ui/EmptyState.vue";
import { useSettings } from "../composables/useSettings";

/**
 * 首页。
 *
 * 三种模式（后端字段 `homepage_mode`，此前"有存无用"，现在真正接上渲染）：
 *   default —— 问候语 + 联机入口说明；
 *   blank   —— 什么都不渲染，只留一块可透出背景的空白；
 *   webpage —— 内嵌 iframe 显示用户填的地址。
 *
 * `frame-src *` 已在 `tauri.conf.json` 的 CSP 里为此预留，不要收紧。
 */
const { t } = useI18n();
const settings = useSettings();

const mode = computed(() => settings.state.homepage_mode || "default");
const url = computed(() => (settings.state.homepage_value || "").trim());

const greeting = computed(() => {
  const hour = new Date().getHours();
  if (hour < 12) return t("home.morning");
  if (hour < 18) return t("home.afternoon");
  return t("home.evening");
});

/** 只在地址合法时才渲染 iframe，否则给出可操作的提示而不是一个空白框。 */
const iframeUrl = computed(() => (/^https?:\/\//i.test(url.value) ? url.value : ""));
</script>

<template>
  <!-- 空白模式：整块透明区域，让背景与窗口材质完全露出 -->
  <div v-if="mode === 'blank'" class="blank" />

  <!-- 网页模式 -->
  <div v-else-if="mode === 'webpage'" class="webpage">
    <iframe
      v-if="iframeUrl"
      class="webpage__frame"
      :src="iframeUrl"
      referrerpolicy="no-referrer"
    />
    <EmptyState
      v-else
      icon="bi bi-link-45deg"
      :title="t('home.webpageMissingUrl')"
      :desc="t('home.webpageMissingUrlHint')"
    />
  </div>

  <!-- 默认模式 -->
  <div v-else class="home">
    <div class="home__hero">
      <h1 class="home__greeting">{{ greeting }}</h1>
      <p class="home__question">{{ t("app.name") }} · {{ t("home.question") }}</p>
    </div>

    <div class="home__card">
      <span class="icon-badge icon-badge--small"><i class="bi bi-broadcast" /></span>
      <div class="grow">
        <h2 class="section-title">{{ t("home.defaultTitle") }}</h2>
        <p class="hint">{{ t("home.defaultDesc") }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.blank {
  flex: 1;
  min-height: 0;
}

.webpage {
  flex: 1;
  min-height: 0;
  display: flex;
}

.webpage__frame {
  flex: 1;
  width: 100%;
  border: none;
  background: var(--surface);
}

.home {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-6);
  padding: var(--sp-7) var(--sp-6);
  overflow-y: auto;
}

.home__hero {
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
}

.home__greeting {
  font-size: var(--fs-display);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  line-height: var(--lh-tight);
}

.home__question {
  font-size: var(--fs-body);
  color: var(--text-secondary);
}

.home__card {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  padding: var(--sp-5);
  border-radius: var(--r-lg);
  background: var(--surface-container-low);
  border: 1px solid var(--outline-variant);
  max-width: 560px;
}
</style>
