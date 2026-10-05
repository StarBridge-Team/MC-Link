<script setup lang="ts">
import AppearanceCard from "../../components/settings/AppearanceCard.vue";
import BackgroundBlurCard from "../../components/settings/BackgroundBlurCard.vue";
import BackgroundMusicCard from "../../components/settings/BackgroundMusicCard.vue";
import BackgroundOpacityCard from "../../components/settings/BackgroundOpacityCard.vue";
import BackgroundSourceCard from "../../components/settings/BackgroundSourceCard.vue";
import ColorCard from "../../components/settings/ColorCard.vue";

/**
 * 个性化：配色、外观（明暗/材质/动画）、背景。
 *
 * 背景拆成四张卡（来源 / 不透明度与遮罩 / 模糊 / 音乐）：这些设置彼此独立 ——
 * 换背景类型不影响模糊强度，音乐更是与"背景长什么样"无关。挤在一张卡里时
 * 十几个控件连成一片，既分不清主次，也找不到想调的那一项。
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
      <BackgroundSourceCard />
      <BackgroundOpacityCard />
      <BackgroundBlurCard />
      <BackgroundMusicCard />
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
