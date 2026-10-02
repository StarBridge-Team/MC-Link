<script setup lang="ts">
import { computed, onMounted, watch, type Component } from "vue";
import { useRoute, useRouter } from "vue-router";
import AboutSection from "./settings/AboutSection.vue";
import BackgroundSection from "./settings/BackgroundSection.vue";
import GeneralSection from "./settings/GeneralSection.vue";
import HomepageSection from "./settings/HomepageSection.vue";
import PersonalizationSection from "./settings/PersonalizationSection.vue";
import PluginSection from "./settings/PluginSection.vue";
import UpdateSection from "./settings/UpdateSection.vue";
import { DEFAULT_SETTING_TAB, isSettingTab, type SettingTab } from "../router";
import { KEYS, local } from "../lib/persist";

/**
 * 设置页容器：按 `tab` 参数渲染对应分区。
 *
 * 分区的取舍：**同一件事只在一个分区里改**。外观与背景分成两个 tab，
 * 是因为背景那张卡片展开后很长，混在一起会让"只想换个主题色"的人翻很久。
 */
const route = useRoute();
const router = useRouter();

const SECTIONS: Record<SettingTab, Component> = {
  personalization: PersonalizationSection,
  background: BackgroundSection,
  homepage: HomepageSection,
  general: GeneralSection,
  plugins: PluginSection,
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
    <!-- 与 App.vue 同样刻意不用 `mode="out-in"`：它会让切换后的内容永久不挂载 -->
    <Transition name="page">
      <component :is="section" :key="tab" />
    </Transition>
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
