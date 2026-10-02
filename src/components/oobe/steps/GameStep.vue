<script setup lang="ts">
import { useI18n } from "vue-i18n";
import type { GameInfo } from "../../../lib/api/types";

/**
 * 引导第三步：选择首个游戏。
 *
 * 清单来自 `game_list`（`methods` 由插件声明派生），因此这里不硬编码任何游戏名。
 * 选中项会写入 `setup.yml` 的 `game`，供插件路由引擎挑选适配器。
 */
defineProps<{
  games: GameInfo[];
  loading: boolean;
  selected: string;
}>();

const emit = defineEmits<{ "update:selected": [value: string] }>();

const { t } = useI18n();
</script>

<template>
  <div class="step">
    <p class="hint">{{ t("oobe.gameHint") }}</p>

    <p v-if="loading" class="hint">{{ t("common.loading") }}</p>
    <p v-else-if="games.length === 0" class="hint">{{ t("oobe.gameEmpty") }}</p>

    <div v-else class="games">
      <button
        v-for="game in games"
        :key="game.id"
        class="game"
        :class="{ 'is-active': game.id === selected }"
        type="button"
        @click="emit('update:selected', game.id)"
      >
        <span class="game__name">{{ game.name || game.id }}</span>
        <span class="game__meta">
          <span class="mono">{{ game.id }}</span>
          <span v-if="game.requiresCoupler" class="tag">{{ t("oobe.gameRequiresCoupler") }}</span>
        </span>
        <i v-if="game.id === selected" class="material-symbols-rounded game__check">check_circle</i>
      </button>
    </div>
  </div>
</template>

<style scoped>
.step {
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  min-height: 0;
}

.games {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-2);
  max-height: 280px;
  overflow-y: auto;
}

@media (max-width: 640px) {
  .games {
    grid-template-columns: 1fr;
  }
}

.game {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  padding: var(--sp-3) var(--sp-4);
  border-radius: var(--r-md);
  border: 1px solid var(--outline-variant);
  background: var(--surface-container-low);
  color: var(--text-secondary);
  text-align: left;
  transition: border-color var(--motion-short) var(--ease-standard),
    background-color var(--motion-short) var(--ease-standard);
}

.game:hover {
  border-color: var(--primary);
}

.game.is-active {
  background: var(--secondary-container);
  border-color: transparent;
  color: var(--on-secondary-container);
}

.game__name {
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  color: var(--text-primary);
}

.game__meta {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  font-size: var(--fs-xs);
  color: var(--text-muted);
}

.game__check {
  position: absolute;
  top: var(--sp-3);
  right: var(--sp-3);
  font-size: 20px;
  color: var(--primary);
}

.tag {
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 var(--sp-2);
  border-radius: var(--r-full);
  background: var(--warning-container);
  color: var(--on-warning-container);
}
</style>
