<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { version as vueVersion } from 'vue';
import { openUrl } from '@tauri-apps/plugin-opener';

const props = defineProps<{
  showToast: (msg: string) => void;
  activeTab: string;
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

// About page data
const appVersion = ref("");
const tauriVersion = ref("");

onMounted(async () => {
  try {
    appVersion.value = await invoke<string>("get_app_version");
    tauriVersion.value = await invoke<string>("get_tauri_version");
  } catch {
    appVersion.value = "?";
    tauriVersion.value = "?";
  }
});

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
  if (val !== 'about') {
    loadContent();
  }
});
</script>

<template>
  <div class="setting-page">
    <div class="setting-content">
      <div class="setting-content-header">
        <h3>{{ tabLabels[activeTab] || activeTab }}</h3>
        <div class="setting-content-actions" v-if="activeTab !== 'about'">
          <button class="btn btn-primary" :disabled="!dirty" @click="saveContent">
            <i class="bi bi-check-lg"></i>
            保存
          </button>
        </div>
      </div>

      <!-- 关于页面 -->
      <div v-if="activeTab === 'about'" class="about-page">
        <div class="about-card">
          <div class="about-card-left">
            <div class="about-icon">
              <img src="/mc-link-icon.png" alt="MC Link" class="about-icon-img" />
            </div>
          </div>
          <div class="about-card-right">
            <div class="about-title-row">
              <span class="about-app-name">MC Link</span>
              <span class="about-version-badge">{{ appVersion }}</span>
            </div>
            <div class="about-meta-row">
              <span class="about-meta-item">
                <span class="about-meta-label">Vue</span>
                <span class="about-meta-value">{{ vueVersion }}</span>
              </span>
              <span class="about-meta-divider"></span>
              <span class="about-meta-item">
                <span class="about-meta-label">Tauri</span>
                <span class="about-meta-value">{{ tauriVersion }}</span>
              </span>
            </div>
            <div class="about-links">
              <a class="about-link" @click="openUrl('https://mclink.nsrwz.cn')" title="官网">
                <i class="bi bi-globe2"></i>
                mclink.nsrwz.cn
              </a>
              <a class="about-link" @click="openUrl('https://github.com/DogerMMC/mc-link')" title="GitHub">
                <i class="bi bi-github"></i>
                DogerMMC/mc-link
              </a>
            </div>
          </div>
        </div>
      </div>

      <!-- 其他设置页面 -->
      <div v-else class="setting-editor">
        <div v-if="loading" class="setting-loading">加载中...</div>
        <textarea
          v-else
          class="setting-textarea"
          :value="content"
          @input="onContentInput"
          spellcheck="false"
        ></textarea>
      </div>
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

.btn {
  transition: all 0.2s ease;
  border-radius: 8px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 500;
  padding: 8px 16px;
  border: none;
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.btn:hover {
  background: rgba(255, 255, 255, 0.15);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-primary {
  background: var(--accent-primary);
  color: #ffffff;
  box-shadow: 0 2px 8px rgba(0, 102, 204, 0.3);
}

.btn-primary:hover:not(:disabled) {
  background: var(--accent-hover);
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

/* ===== 关于页面 ===== */
.about-page {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
}

.about-card {
  background: var(--bg-card, rgba(255,255,255,0.06));
  border: 1px solid var(--border-color, rgba(255,255,255,0.08));
  border-radius: 12px;
  padding: 24px;
  display: flex;
  align-items: center;
  gap: 20px;
  box-shadow: 0 2px 12px rgba(0,0,0,0.1);
}

.about-card-left {
  flex-shrink: 0;
}

.about-icon {
  width: 64px;
  height: 64px;
  border-radius: 14px;
  overflow: hidden;
}

.about-icon-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.about-card-right {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.about-title-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.about-app-name {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
}

.about-version-badge {
  display: inline-block;
  padding: 2px 10px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 600;
  background: var(--accent-primary);
  color: #fff;
  line-height: 1.6;
}

.about-meta-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.about-meta-item {
  display: flex;
  align-items: center;
  gap: 6px;
}

.about-meta-label {
  font-size: 12px;
  color: var(--text-muted);
  font-weight: 500;
}

.about-meta-value {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
}

.about-meta-divider {
  width: 1px;
  height: 14px;
  background: var(--border-color, rgba(255,255,255,0.15));
}

.about-links {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 4px;
}

.about-link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-muted);
  cursor: pointer;
  text-decoration: none;
  transition: color 0.15s ease;
}

.about-link:hover {
  color: var(--accent-primary);
}

.about-link i {
  font-size: 14px;
}

/* Light mode overrides */
@media (prefers-color-scheme: light) {
  .about-card {
    background: rgba(255,255,255,0.8);
    border-color: rgba(0,0,0,0.08);
  }
}

[data-theme="light"] .about-card {
  background: rgba(255,255,255,0.8);
  border-color: rgba(0,0,0,0.08);
}
</style>
