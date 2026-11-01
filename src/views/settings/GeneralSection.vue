<script setup lang="ts">
import { useI18n } from "vue-i18n";
import InfoBar from "../../components/ui/InfoBar.vue";
import GeneralCard from "../../components/settings/GeneralCard.vue";
import SettingsManifestSection from "./SettingsManifestSection.vue";
import { useSetup } from "../../composables/useSetup";

/**
 * 通用：语言与地区、玩家名，以及资源服务器下发的「更多设置」。
 *
 * 语言/地区属于"环境"而非"外观个性化"，所以独立成页，不塞进个性化里（个性化只管配色/外观/背景）。
 */
const { t } = useI18n();
const setup = useSetup();
</script>

<template>
  <div class="scroll-area">
    <!--
      通用设置的失败出口：此前 `setup.error` 没有任何组件消费，
      保存失败只留在一个没人看的 ref 里（用户以为存上了，重启回退）。
    -->
    <InfoBar
      v-if="setup.error.value"
      kind="danger"
      :text="`${t('common.saveFailed')}: ${setup.error.value}`"
    />

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
