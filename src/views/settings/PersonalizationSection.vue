<script setup lang="ts">
import { useI18n } from "vue-i18n";
import InfoBar from "../../components/ui/InfoBar.vue";
import AppearanceCard from "../../components/settings/AppearanceCard.vue";
import BackgroundBlurCard from "../../components/settings/BackgroundBlurCard.vue";
import BackgroundMusicCard from "../../components/settings/BackgroundMusicCard.vue";
import BackgroundOpacityCard from "../../components/settings/BackgroundOpacityCard.vue";
import BackgroundSourceCard from "../../components/settings/BackgroundSourceCard.vue";
import ColorCard from "../../components/settings/ColorCard.vue";
import { useSettings } from "../../composables/useSettings";

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
const { t } = useI18n();
const settings = useSettings();
</script>

<template>
  <div class="scroll-area">
    <!--
      落盘状态必须看得见：这些卡片全部经 `settings.patch()` 改设置，
      而 `patch()` 是"先改内存、400ms 后落盘"。落盘失败（配置目录只读 / 磁盘满 /
      序列化失败）时界面照样变，用户零感知，重启后回退。
    -->
    <InfoBar
      v-if="settings.saveError.value"
      kind="danger"
      :text="`${t('common.saveFailed')}: ${settings.saveError.value}`"
    />
    <InfoBar
      v-else-if="settings.saving.value"
      kind="info"
      :text="t('common.save')"
    />

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
