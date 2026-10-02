<script setup lang="ts">
import AppearanceCard from "../../components/settings/AppearanceCard.vue";
import BackgroundCard from "../../components/settings/BackgroundCard.vue";
import GeneralCard from "../../components/settings/GeneralCard.vue";
import SettingsManifestSection from "./SettingsManifestSection.vue";

/**
 * 个性化：把原先散在「外观 / 背景 / 通用」三个分区里的选项收敛到一处。
 *
 * 拆成三个卡片组件（`components/settings/*`）而不是堆在一个文件里：
 * 单文件超过 400 行就该拆，而且每张卡片的逻辑（外观、背景、语言/玩家）本就互不相关。
 * 末尾的 `SettingsManifestSection` 是资源服务器下发的「更多设置」，与前几项无关。
 */
</script>

<template>
  <div class="scroll-area">
    <div class="grid stagger">
      <AppearanceCard />
      <BackgroundCard />
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
