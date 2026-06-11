<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import AboutSection from "./AboutSection.vue";
import AccountSection from "./AccountSection.vue";
import PersonalizationSection from "./PersonalizationSection.vue";
import Button from "../ui/Button.vue";

const props = defineProps<{
  showToast: (msg: string) => void;
  activeTab: string;
  playerName?: string;
}>();

const emit = defineEmits<{
  (e: 'name-change', val: string): void;
}>();

const content = ref("");
const loading = ref(false);
const dirty = ref(false);

const tabLabels: Record<string, string> = {
  account: "账号",
  network: "网络",
  personalization: "个性化",
  adapter: "适配器",
  connector: "联机器",
  about: "关于",
};

const persRef = ref<InstanceType<typeof PersonalizationSection> | null>(null);

async function loadContent() {
  loading.value = true;
  try {
    const result = await invoke<string>("get_setting", { section: props.activeTab });
    content.value = result;
  } catch {
    content.value = "# " + (tabLabels[props.activeTab] || props.activeTab) + "\n# 暂无配置\n";
  } finally {
    loading.value = false;
    dirty.value = false;
  }
}

async function saveContent() {
  try {
    await invoke("save_setting", { section: props.activeTab, content: content.value });
    props.showToast("设置已保存");
    dirty.value = false;
  } catch (e: any) {
    props.showToast("保存失败: " + e);
  }
}

function onContentInput(e: Event) {
  content.value = (e.target as HTMLTextAreaElement).value;
  dirty.value = true;
}

watch(() => props.activeTab, (val) => {
  if (val === 'personalization') {
    persRef.value?.loadPersonalization();
  } else if (val !== 'about' && val !== 'account') {
    loadContent();
  }
});
</script>

<template>
  <div class="setting-page">
    <div class="setting-content">
      <div class="setting-content-header">
        <h3>{{ tabLabels[activeTab] || activeTab }}</h3>
        <div class="setting-content-actions" v-if="activeTab !== 'about' && activeTab !== 'personalization' && activeTab !== 'account'">
          <Button
            variant="primary"
            :disabled="!dirty"
            @click="saveContent"
          >
            <i class="bi bi-check-lg"></i>
            保存
          </Button>
        </div>
      </div>

      <Transition name="page-fade" mode="out-in">
        <AboutSection v-if="activeTab === 'about'" key="about" />
        <AccountSection v-else-if="activeTab === 'account'" key="account" :show-toast="showToast" :player-name="playerName" @name-change="emit('name-change', $event)" />
        <PersonalizationSection v-else-if="activeTab === 'personalization'" key="personalization" ref="persRef" :show-toast="showToast" />

        <!-- 其他设置页面（network, adapter, connector） -->
        <div v-else key="editor" class="setting-editor">
          <div v-if="loading" class="setting-loading">加载中...</div>
          <textarea
            v-else
            class="setting-textarea"
            :value="content"
            @input="onContentInput"
            spellcheck="false"
          ></textarea>
        </div>
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
  padding: 20px 24px;
  overflow: hidden;
}

.setting-content-header {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}

.setting-content-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
  flex: 1;
}

.setting-content-actions {
  display: flex;
  gap: 8px;
}

.setting-editor {
  flex: 1;
  overflow: hidden;
  position: relative;
}

.setting-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-muted);
  font-size: 14px;
}

.setting-textarea {
  width: 100%;
  height: 100%;
  padding: 16px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
  font-size: 13px;
  line-height: 1.6;
  resize: none;
  outline: none;
  box-sizing: border-box;
  transition: border-color 0.2s ease;
}

.setting-textarea:focus {
  border-color: var(--accent-primary);
}
</style>