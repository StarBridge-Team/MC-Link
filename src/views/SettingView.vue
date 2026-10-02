<script setup lang="ts">
import { computed, onMounted, watch, type Component } from "vue";
import { useRoute, useRouter } from "vue-router";
import AboutSection from "./settings/AboutSection.vue";
import GeneralSection from "./settings/GeneralSection.vue";
import HomepageSection from "./settings/HomepageSection.vue";
import PersonalizationSection from "./settings/PersonalizationSection.vue";
import UpdateSection from "./settings/UpdateSection.vue";
import { DEFAULT_SETTING_TAB, isSettingTab, type SettingTab } from "../router";
import { KEYS, local } from "../lib/persist";

/**
 * 设置页容器：按 `tab` 参数渲染对应分区。
 *
 * 分区的取舍：**同一件事只在一个分区里改**。所有个性化选项（外观 / 背景 /
 * 语言与地区）已合并进「个性化」，避免同一类设置散落在多个 tab。
 *
 * 「插件」原本在这里，现已移到「游戏」页（`views/GameView.vue`）：它和"选游戏 / 联机"
 * 是同一件事的不同侧面，放在设置里既难找、又要和一堆偏好项抢位置。
 */
const route = useRoute();
const router = useRouter();

const SECTIONS: Record<SettingTab, Component> = {
  personalization: PersonalizationSection,
  homepage: HomepageSection,
  general: GeneralSection,
  update: UpdateSection,
  about: AboutSection,
};

const tab = computed<SettingTab>(() =>
  isSettingTab(route.params.tab) ? route.params.tab : DEFAULT_SETTING_TAB,
);

const section = computed(() => SECTIONS[tab.value]);

// 记住上次看的分区：从别的页面点「设置」回来时直接落到它。
onMounted(() => {
  if (isSettingTab(route.params.tab)) {
    local.setString(KEYS.settingTab, route.params.tab);
    return;
  }
  const saved = local.getString(KEYS.settingTab);
  const target = isSettingTab(saved) ? saved : DEFAULT_SETTING_TAB;
  void router.replace({ name: "setting", params: { tab: target } });
});

watch(tab, (value) => local.setString(KEYS.settingTab, value));
</script>

<template>
  <div class="setting">
    <!-- 同 App.vue：不用 <Transition>，只给进入项一条动画（base.css 的 `page-in`） -->
    <component :is="section" :key="tab" />
  </div>
</template>

<style scoped>
.setting {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
</style>
