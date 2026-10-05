<script setup lang="ts">
import GeneralCard from "../../components/settings/GeneralCard.vue";
import SettingsManifestSection from "./SettingsManifestSection.vue";

/**
 * 通用：语言与地区、玩家名，以及资源服务器下发的「更多设置」。
 *
 * 语言/地区属于"环境"而非"外观个性化"，所以独立成页，不塞进个性化里（个性化只管配色/外观/背景）。
 */
</script>

<template>
  <div class="scroll-area">
    <div class="grid stagger">
      <GeneralCard />
      <SettingsManifestSection />
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
