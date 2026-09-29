<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import AboutSection from "./AboutSection.vue";
import PersonalizationSection from "./PersonalizationSection.vue";
import { getSettingManifest } from "../../lib/api/settingMeta";
import type { SettingSection } from "../../lib/api/types";

const props = defineProps<{
  showToast: (msg: string) => void;
  activeTab: string;
  playerName?: string;
}>();

defineEmits<{
  (e: 'name-change', val: string): void;
}>();

const persRef = ref<InstanceType<typeof PersonalizationSection> | null>(null);

/** 设置项清单（拉取失败时为空，渲染逻辑仍按 activeTab 走兜底） */
const sections = ref<SettingSection[]>([]);

async function loadManifest() {
  try {
    const m = await getSettingManifest();
    sections.value = m.sections;
  } catch {
    /* 拉取失败时使用兜底逻辑 */
  }
}

onMounted(() => {
  void loadManifest();
});

watch(() => props.activeTab, (val) => {
  if (val === 'personalization') {
    persRef.value?.loadPersonalization();
  }
});
</script>

<template>
  <div class="setting-page">
    <div class="setting-content">
      <Transition name="page-fade" mode="out-in">
        <AboutSection v-if="activeTab === 'about'" key="about" />
        <PersonalizationSection v-else key="personalization" ref="persRef" :show-toast="showToast" />
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.setting-page {
  height: 100%;
  display: flex;
}

.setting-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
  overflow: hidden;
  min-height: 0;
}
</style>
