<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useRouter } from "vue-router";
import EmptyState from "../components/ui/EmptyState.vue";
import { useSettings } from "../composables/useSettings";
import { useConnect } from "../composables/useConnect";
import { runOpenActions } from "../lib/api/app";
import { listLocalGames } from "../lib/api/connect";
import type { LocalGameFound } from "../lib/api/types";

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
const router = useRouter();
const { requestEntry } = useConnect();

const mode = computed(() => settings.state.homepage_mode || "default");
const url = computed(() => (settings.state.homepage_value || "").trim());

const greeting = computed(() => {
  const hour = new Date().getHours();
  if (hour < 6) return t("home.evening");
  if (hour < 9) return t("home.morning");
  if (hour < 12) return t("home.forenoon");
  if (hour < 13) return t("home.noon");
  if (hour < 18) return t("home.afternoon");
  return t("home.evening");
});

/** 只在地址合法时才渲染 iframe，否则给出可操作的提示而不是一个空白框。 */
const iframeUrl = computed(() => (/^https?:\/\//i.test(url.value) ? url.value : ""));

// --- 本地游戏扫描（后端动作管理器在应用打开时自动扫描，这里只收事件） ---
//
// 结构用统一的 `LocalGameFound`（`lib/api/types`），不再本地声明一份：
// 此前这里与 `typesConnect.ts` 各有一份同名字段不同的接口，类型系统拦不住任何漂移。
const localGame = ref<LocalGameFound | null>(null);
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
    await listen<LocalGameFound>("local-game-found", (e) => {
      localGame.value = e.payload;
      hostMode.value = true; // 扫到局域网游戏自动切换房主
    }),
  );

  // 先订阅、再拉取当前结果。
  //
  // 扫描在应用打开时就跑完了，事件是一次性广播、不会补发：如果这个页面比事件晚一步
  // （启动竞态、页面重载），光靠监听会永远停在"正在寻找本地游戏…"。
  // 拉取走的是同一个缓存，所以两者不会打架。
  try {
    const games = await listLocalGames();
    const first = games[0];
    if (first) {
      // `connect_local_games` 回传的就是完整的展示结构，直接用，不要再"翻译"一遍
      // （此前按 `LocalGame` 的形状重拼，把 `scanner`/`adapter` 丢成空串）。
      localGame.value = first;
      hostMode.value = true;
      scanning.value = false;
    }
  } catch {
    /* 拉不到就继续等事件/手动重扫 */
  }
});

onUnmounted(() => {
  unlisteners.forEach((u) => u());
});

function toggleMode() {
  hostMode.value = !hostMode.value;
}

/**
 * 重新扫描。
 *
 * 刻意**不清空 `localGame`**：扫描失败或没扫到任何东西时，用户手里的结果比一块空白有用；
 * 清空还会让"扫到 → 点重扫 → 卡片消失"看起来像结果丢了。
 */
async function rescan() {
  if (scanning.value) return;
  scanning.value = true;
  try {
    await runOpenActions();
  } catch {
    // 扫描能力由插件提供，无插件时本就无结果。
    // 用 `finally` 复位状态：不依赖 `local-game-status` 事件一定送达，
    // 否则一次失败会让按钮永久停在 disabled。
    scanning.value = false;
  }
}

/**
 * 「开始联机」。
 *
 * 这里不自己跑联机流程，而是跳转到联机页、并请求它落在对应角色上：
 * 房主模式带上扫到的进程名，联机页会自动替用户按下那张卡片的「开始联机」，
 * 等价于"在联机页点了那个进程的开始联机按钮"。
 */
function startCoop() {
  if (hostMode.value) requestEntry("host", localGame.value?.process);
  else requestEntry("member");
  void router.push({ name: "connect" });
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
      <!--
        房主模式必须先扫到本机游戏才有意义（联机页那张卡片的动作就是"为这个进程开房"）；
        成员模式不受此限，点进去直接填邀请码。
      -->
      <m3e-button
        variant="filled"
        class="home__mode-start"
        :disabled="hostMode && !localGame"
        @click="startCoop"
      >
        <m3e-icon slot="icon" name="link" />
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
