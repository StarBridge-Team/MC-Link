<script setup lang="ts">
import { useI18n } from "vue-i18n";

/**
 * 「市场」：**刻意先空着**。
 *
 * 后端目前没有任何"插件目录 / 下载 / 安装"命令（插件安装器与共享运行时都还没实现），
 * 所以这里不塞任何占位数据 —— 一个看起来能装插件、实际装不了的市场，比一个诚实的
 * 空页面更容易误导人，而且会让人以为"是网络/权限问题"而不是"功能还没做"。
 *
 * 等后端有了对应的命令（形如 `market_list` / `market_install`），把目录列表接在这里
 * 即可；卡片排版沿用「游戏」那一页的样式即可保持一致。
 */
const { t } = useI18n();
</script>

<template>
  <div class="scroll-area">
    <div class="market">
      <span class="icon-badge"><i class="material-symbols-rounded">storefront</i></span>
      <h2 class="market__title">{{ t("game.marketEmptyTitle") }}</h2>
      <p class="market__desc">{{ t("game.marketEmptyDesc") }}</p>
      <p class="hint">{{ t("game.marketEmptyHint") }}</p>
    </div>
  </div>
</template>

<style scoped>
.market {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--sp-3);
  min-height: 320px;
  text-align: center;
}

.market__title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  line-height: var(--lh-tight);
}

.market__desc {
  max-width: 46em;
  font-size: var(--fs-base);
  color: var(--text-muted);
  line-height: var(--lh-normal);
}
</style>
