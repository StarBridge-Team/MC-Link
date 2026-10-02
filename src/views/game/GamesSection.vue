<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import InfoBar from "../../components/ui/InfoBar.vue";
import { useGameSearch } from "../../composables/useGameSearch";
import { listGames } from "../../lib/api/plugin";
import type { GameInfo } from "../../lib/api/types";

/**
 * 「游戏」：后端游戏表（`game_list`）。
 *
 * **搜索交给后端**，理由与插件那边一致：`game_list` 返回 `total` / `matched`，只有让后端
 * 按同一口径过滤，计数才与列表同源。前端再自己过滤一遍，迟早会和"别名、方法"这些只有
 * 后端才知道的字段对不上 —— 那类"列表里有一条、计数里没有"的问题最难查。
 */
const { t } = useI18n();
const { queries } = useGameSearch();

const games = ref<GameInfo[]>([]);
const total = ref(0);
const matched = ref(0);
const loading = ref(false);
const error = ref<string | null>(null);

let searchTimer: ReturnType<typeof setTimeout> | null = null;

const keyword = computed(() => queries.games.trim());

async function load() {
  loading.value = true;
  error.value = null;
  try {
    const result = await listGames(keyword.value || undefined);
    games.value = result.games;
    total.value = result.total;
    matched.value = result.matched;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

// 搜索框在「游戏」页的工具栏上（切换模式时它不该动）：350ms 防抖，避免每敲一个字就跨一次 IPC
watch(keyword, () => {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void load(), 350);
});

onMounted(() => void load());

onUnmounted(() => {
  if (searchTimer) clearTimeout(searchTimer);
});
</script>

<template>
  <div class="scroll-area">
    <InfoBar v-if="error" kind="danger" :text="`${t('game.errorTitle')}: ${error}`">
      <m3e-button size="small" @click="load">{{ t("common.retry") }}</m3e-button>
    </InfoBar>

    <div v-else class="games">
      <p class="hint">
        {{ t("game.gamesTotal", { count: total }) }}
        <span v-if="keyword"> · {{ t("game.gamesMatched", { count: matched }) }}</span>
      </p>

      <p v-if="loading" class="hint">{{ t("common.loading") }}</p>

      <!-- 搜不到与"表里没有"是两件事：前者该提示换关键词，后者才是真的空 -->
      <div v-else-if="games.length === 0" class="games__empty">
        <span class="icon-badge"><i class="material-symbols-rounded">search_off</i></span>
        <h2 class="games__empty-title">
          {{ keyword ? t("game.gamesEmptyTitle") : t("game.gamesEmptyAllTitle") }}
        </h2>
        <p class="hint">{{ keyword ? t("game.gamesEmptyDesc") : t("game.gamesEmptyAllDesc") }}</p>
      </div>

      <div v-else class="games__grid stagger">
        <m3e-card
          v-for="(game, index) in games"
          :key="game.id"
          variant="elevated"
          class="game-card"
          :style="{ '--stagger-index': index }"
        >
          <div class="game-card__body">
            <header class="game-card__head">
              <h3 class="game-card__name">{{ game.name }}</h3>
              <span class="mono game-card__id">{{ game.id }}</span>
            </header>

            <p v-if="game.aliases.length" class="game-card__aliases">
              {{ game.aliases.join(" · ") }}
            </p>

            <div class="game-card__meta">
              <span class="tag mono">{{ game.transport }}</span>
              <span class="tag mono">{{ t("game.port") }} {{ game.port }}</span>
              <span v-for="method in game.methods" :key="method" class="tag">{{ method }}</span>
            </div>

            <p v-if="game.preferredAdapter" class="hint mono">
              {{ t("game.preferredAdapter") }}: {{ game.preferredAdapter }}
            </p>
            <p v-if="game.requiresCoupler" class="hint game-card__warn">
              <i class="material-symbols-rounded">info</i>
              {{ t("game.requiresCoupler") }}
            </p>
          </div>
        </m3e-card>
      </div>
    </div>
  </div>
</template>

<style scoped>
.games {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.games__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--sp-4);
}

/* 游戏卡片用 m3e-card 的 elevated 变体，与插件页（PluginCard）保持同一种卡片语言。
   hover 抬升交给变体自己的 elevation，不再手写 box-shadow；默认插槽不带内边距，
   内容包一层 .game-card__body 把内边距补回来。 */
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

.game-card__id {
  font-size: var(--fs-xs);
  color: var(--text-muted);
}

.game-card__aliases {
  font-size: var(--fs-label);
  color: var(--text-muted);
  line-height: var(--lh-normal);
}

.game-card__meta {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
  margin-top: var(--sp-1);
}

.game-card__warn {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
}

.game-card__warn i {
  font-size: 16px;
}

.tag {
  display: inline-flex;
  align-items: center;
  padding: 2px var(--sp-2);
  border: 1px solid var(--outline-variant);
  border-radius: var(--r-full);
  font-size: var(--fs-xs);
  color: var(--text-muted);
}

.games__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--sp-3);
  min-height: 280px;
  text-align: center;
}

.games__empty-title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}
</style>
