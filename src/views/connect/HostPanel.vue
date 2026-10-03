<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useConnect } from "../../composables/useConnect";
import type { LocalGame } from "../../lib/api/types";
import StartDialog from "./StartDialog.vue";

/**
 * 房主模式：列出检测器扫到的本机游戏实例，每个卡片带「开始联机」按钮。
 * 点了按钮才弹窗（选适配器 + 按适配器 schema 出字段），符合"最少填写"。
 */
const { t } = useI18n();
const { localGames, scanLocalGames } = useConnect();

const dialogOpen = ref(false);
const activeGame = ref<LocalGame | null>(null);

onMounted(() => void scanLocalGames());

function openStart(game: LocalGame) {
  activeGame.value = game;
  dialogOpen.value = true;
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

    <!-- 没扫到游戏 → 空状态 -->
    <div v-if="localGames.length === 0" class="host__empty">
      <span class="icon-badge"><i class="material-symbols-rounded">router</i></span>
      <h2 class="host__empty-title">{{ t("connect.noScannedGames") }}</h2>
      <p class="hint">{{ t("connect.noScannedGamesDesc") }}</p>
    </div>

    <!-- 扫到的所有游戏：卡片网格，每张带开始联机按钮 -->
    <div v-else class="host__grid stagger">
      <m3e-card
        v-for="(g, index) in localGames"
        :key="g.id + g.port"
        variant="elevated"
        class="game-card"
        :style="{ '--stagger-index': index }"
      >
        <div class="game-card__body">
          <header class="game-card__head">
            <h3 class="game-card__name">{{ g.name }}</h3>
          </header>
          <p class="hint mono">{{ t("game.port") }} {{ g.port }}</p>
          <p class="hint ellipsis">{{ g.process }}</p>
          <div class="game-card__actions">
            <m3e-button variant="filled" @click="openStart(g)">
              <m3e-icon slot="icon" name="link" />
              {{ t("connect.startConnect") }}
            </m3e-button>
          </div>
        </div>
      </m3e-card>
    </div>

    <StartDialog
      v-if="dialogOpen"
      mode="host"
      :game="activeGame"
      :open="dialogOpen"
      @close="dialogOpen = false"
    />
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
