<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
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

// --- 本地游戏扫描（后端动作管理器在应用打开时自动扫描，这里只收事件） ---
interface LocalGame {
  process: string;
  game_name: string;
  scanner: string;
  adapter: string;
}

const localGame = ref<LocalGame | null>(null);
const scanning = ref(true);
const hostMode = ref(false);
let unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  unlisteners.push(
    await listen<{ status: string; count?: number }>("local-game-status", (e) => {
      scanning.value = e.payload.status === "scanning";
    }),
  );
  unlisteners.push(
    await listen<LocalGame>("local-game-found", (e) => {
      localGame.value = e.payload;
      hostMode.value = true; // 扫到局域网游戏自动切换房主
    }),
  );
});

onUnmounted(() => {
  unlisteners.forEach((u) => u());
});

function toggleMode() {
  hostMode.value = !hostMode.value;
}

async function rescan() {
  scanning.value = true;
  localGame.value = null;
  try {
    await invoke("run_open_actions");
  } catch {
    /* 忽略：扫描能力由插件提供，无插件时本就无结果 */
  }
}
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
      icon="link"
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
      <span class="icon-badge icon-badge--small"><i class="material-symbols-rounded">broadcast_on_home</i></span>
      <div class="grow">
        <h2 class="section-title">{{ t("home.defaultTitle") }}</h2>
        <p class="hint">{{ t("home.defaultDesc") }}</p>
      </div>
    </div>

    <!-- 底部：左侧扫描状态 + 右侧联机模式卡片，二者在 Y 方向居中对齐 -->
    <div class="home__footer">
      <div class="home__scan">
      <button
        class="home__scan-btn"
        :disabled="scanning"
        :title="t('home.scanRetry')"
        @click="rescan"
      >
        <i class="material-symbols-rounded">refresh</i>
      </button>
      <div class="home__scan-info">
        <template v-if="localGame">
          <span class="home__scan-process">
            <i class="material-symbols-rounded">videogame_asset</i>
            {{ localGame.process }}
          </span>
          <small class="home__scan-meta">
            <i class="material-symbols-rounded">radar</i>
            {{ localGame.scanner }}
            <span class="dot">·</span>
            <i class="material-symbols-rounded">router</i>
            {{ localGame.adapter || t('home.noAdapter') }}
          </small>
        </template>
        <span v-else class="home__scan-loading">
          <i class="material-symbols-rounded">search</i>
          {{ t('home.scanning') }}
        </span>
      </div>
    </div>

    <!-- 右下角：联机模式卡片（点击切换成员 / 房主；扫到局域网游戏自动变房主） -->
    <m3e-card variant="elevated" clickable class="home__mode" @click="toggleMode">
      <div class="home__mode-head">
        <i class="material-symbols-rounded home__mode-icon">{{ hostMode ? 'crown' : 'group' }}</i>
        <div class="home__mode-text">
          <strong>{{ hostMode ? t('home.host') : t('home.member') }}</strong>
          <small>{{ t('home.modeHint') }}</small>
        </div>
      </div>
      <m3e-button disabled class="home__mode-start">
        <i slot="icon" class="material-symbols-rounded">link</i>
        {{ t('home.startCoop') }}
      </m3e-button>
    </m3e-card>
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
  position: relative;
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

/* 底部区域：左扫描状态 + 右联机模式卡片，整体在 Y 方向居中对齐 */
.home__footer {
  position: absolute;
  left: var(--sp-6);
  right: var(--sp-6);
  bottom: var(--sp-6);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-4);
  pointer-events: none;
}

.home__footer > * {
  pointer-events: auto;
}

/* 左下角：本地游戏扫描状态 */
.home__scan {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  max-width: 320px;
}

.home__scan-btn {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  flex: none;
  border: none;
  border-radius: 50%;
  background: var(--surface-container-high);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background-color var(--motion-short) var(--ease-standard);
}

.home__scan-btn:hover:not(:disabled) {
  background: var(--surface-container-highest);
}

.home__scan-btn:disabled {
  cursor: default;
  opacity: 0.7;
}

.home__scan-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.home__scan-process {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: var(--fw-medium);
  color: var(--text-primary);
}

.home__scan-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--text-secondary);
  font-size: var(--fs-label);
}

.home__scan-loading {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-secondary);
}

.home__scan-info .material-symbols-rounded {
  font-size: 18px;
}

.home__scan-meta .dot {
  opacity: 0.5;
}

/* 右下角：联机模式卡片 */
.home__mode {
  width: 220px;
  flex: none;
  --m3e-card-radius: 18px;
  border-radius: 18px;
  overflow: hidden;
}

.home__mode-head {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: var(--sp-3);
  padding-bottom: var(--sp-1);
}

.home__mode-icon {
  font-size: 28px;
  color: var(--primary);
}

.home__mode-text {
  display: flex;
  flex-direction: column;
}

.home__mode-text strong {
  color: var(--text-primary);
}

.home__mode-text small {
  color: var(--text-secondary);
}

.home__mode-start {
  display: block;
  width: auto;
  margin: 0 var(--sp-3) var(--sp-3);
}
</style>
