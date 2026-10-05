<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { useI18n } from "vue-i18n";
import { useConnect } from "../../composables/useConnect";
import type { LocalGame } from "../../lib/api/types";

/**
 * 房主模式：列出检测器扫到的本机游戏实例，每个卡片带「开始联机」按钮。
 * 点了按钮才弹窗（选适配器 + 按适配器 schema 出字段），符合"最少填写"。
 *
 * 弹窗状态由父级 `ConnectView` 持有——首页的跨页请求也要能打开它。
 */
const emit = defineEmits<{ (e: "start", game: LocalGame): void }>();

const { t } = useI18n();
const { filteredLocalGames, noMatch, scanLocalGames } = useConnect();

/**
 * 自动重扫描间隔。
 *
 * 游戏中途启动/退出很常见，只靠手动点「重新扫描」很容易漏；且扫描本身很轻
 * （本机检测器一次查询）。扫描在途时会跳过本轮，见 `scanLocalGames`。
 */
const RESCAN_INTERVAL = 5000;

let timer: number | null = null;

onMounted(() => {
  void scanLocalGames();
  timer = window.setInterval(() => void scanLocalGames(), RESCAN_INTERVAL);
});

onUnmounted(() => {
  if (timer !== null) window.clearInterval(timer);
  timer = null;
});

function openStart(game: LocalGame) {
  emit("start", game);
}
</script>

<template>
  <div class="host scroll-area">
    <div class="host__head">
      <span class="hint">{{ t("connect.hostModeNote") }}</span>
      <m3e-button size="small" variant="tonal" :disabled="false" @click="scanLocalGames()">
        <m3e-icon slot="icon" name="refresh" />
        {{ t("connect.rescan") }}
      </m3e-button>
    </div>

    <!-- 没扫到游戏，或被搜索词筛空 → 空状态（两种原因文案不同） -->
    <div v-if="filteredLocalGames.length === 0" class="host__empty">
      <m3e-icon :name="noMatch ? 'search_off' : 'router'" class="host__empty-icon" />
      <h2 class="host__empty-title">
        {{ noMatch ? t("connect.noSearchMatch") : t("connect.noScannedGames") }}
      </h2>
      <p v-if="!noMatch" class="hint">{{ t("connect.noScannedGamesDesc") }}</p>
    </div>

    <!-- 扫到的所有游戏：卡片网格，每张带开始联机按钮 -->
    <div v-else class="host__grid stagger">
      <m3e-card
        v-for="(g, index) in filteredLocalGames"
        :key="g.id + g.port"
        variant="elevated"
        class="game-card"
        :style="{ '--stagger-index': index }"
      >
        <div class="game-card__body">
          <header class="game-card__head">
            <h3 class="game-card__name">{{ g.name }}</h3>
          </header>
          <!--
            第一行是局域网广播里的服务器名（检测器把它放在 `process` 字段里），
            第二行才是端口。所以这里不能按"进程名"来标——首页展示的是同一个值。
          -->
          <p class="hint ellipsis">{{ g.process }}</p>
          <p class="hint mono">{{ t("game.port") }} {{ g.port }}</p>
          <div class="game-card__actions">
            <m3e-button variant="filled" @click="openStart(g)">
              <m3e-icon slot="icon" name="link" />
              {{ t("connect.startConnect") }}
            </m3e-button>
          </div>
        </div>
      </m3e-card>
    </div>

  </div>
</template>

<style scoped>
.host {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.host__head {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}
.host__head .hint {
  flex: 1;
}

.host__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--sp-3);
  min-height: 260px;
  text-align: center;
}
.host__empty-icon {
  font-size: 40px;
  opacity: 0.6;
  color: var(--text-muted);
}
.host__empty-title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}

.host__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--sp-4);
}

/* 复用 GamesSection 的游戏卡片语言：m3e-card elevated + 内容包一层 body 补内边距 */
.game-card {
  --m3e-elevated-card-container-color: var(--card-bg);
  --m3e-elevated-card-hover-container-elevation: 2;
}
.game-card__body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
  padding: var(--sp-5);
}
.game-card__head {
  display: flex;
  align-items: baseline;
  gap: var(--sp-2);
  min-width: 0;
}
.game-card__name {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  line-height: var(--lh-tight);
}
.ellipsis {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.game-card__actions {
  margin-top: var(--sp-2);
}
</style>
