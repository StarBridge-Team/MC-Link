<script setup lang="ts">
import AppearanceCard from "../../components/settings/AppearanceCard.vue";
import BackgroundCard from "../../components/settings/BackgroundCard.vue";
import ColorCard from "../../components/settings/ColorCard.vue";

/**
 * 个性化：配色、外观（明暗/材质/动画）、背景。
 *
 * 只放"外观层面"的选项。语言/地区与玩家名属于通用（`GeneralSection`），
 * 资源服务器下发的「更多设置」也在通用页——避免同一类设置散落或重名。
 */
</script>

<template>
  <div class="scroll-area">
    <div class="grid stagger">
      <ColorCard />
      <AppearanceCard />
      <BackgroundCard />
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
</style>
