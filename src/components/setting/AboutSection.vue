<script setup lang="ts">
import { ref, onMounted } from "vue";
import { openUrl } from '@tauri-apps/plugin-opener';
import { invoke } from "@tauri-apps/api/core";
import { version as vueVersion } from 'vue';

const appVersion = ref("");
const tauriVersion = ref("");
const aboutLoading = ref(true);

onMounted(async () => {
  try {
    const [v, tv] = await Promise.all([
      invoke<string>("get_app_version"),
      invoke<string>("get_tauri_version"),
    ]);
    appVersion.value = v;
    tauriVersion.value = tv;
  } catch {
    appVersion.value = "?";
    tauriVersion.value = "?";
  } finally {
    aboutLoading.value = false;
  }
});
</script>

<template>
  <div class="about-page">
    <div class="about-card">
      <div class="about-card-left">
        <div class="about-icon">
          <img src="/mc-link-icon.png" alt="MC Link" class="about-icon-img" />
        </div>
      </div>
      <div class="about-card-right">
        <div class="about-title-row">
          <span class="about-app-name">MC Link</span>
          <span class="about-version-badge">{{ aboutLoading ? '...' : appVersion }}</span>
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

    <div class="about-card">
      <div class="about-card-body">
        <div class="about-card-title">开发者</div>
        <div class="about-dev-list">
          <div class="about-dev-item">
            <img class="about-avatar" src="http://q.qlogo.cn/g?b=qq&nk=196783749&s=640" alt="avatar" />
            <div class="about-dev-info">
              <span class="about-dev-name">粗狗</span>
              <span class="about-dev-role">项目发起人 / 核心开发</span>
            </div>
          </div>
          <div class="about-dev-item">
            <img class="about-avatar" src="http://q.qlogo.cn/g?b=qq&nk=3995090331&s=640" alt="avatar" />
            <div class="about-dev-info">
              <span class="about-dev-name">oiiaio猫</span>
              <span class="about-dev-role">Python开发</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="about-card">
      <div class="about-card-body">
        <div class="about-card-title">鸣谢</div>
        <div class="about-dev-list">
          <div class="about-dev-item">
            <img class="about-avatar" src="http://q.qlogo.cn/g?b=qq&nk=1508668973&s=640" alt="avatar" />
            <div class="about-dev-info">
              <span class="about-dev-name">烤鱼</span>
              <span class="about-dev-role">节点贡献者 / 找bug小能手</span>
            </div>
          </div>
          <div class="about-dev-item">
            <img class="about-avatar" src="http://q.qlogo.cn/g?b=qq&nk=3680388074&s=640" alt="avatar" />
            <div class="about-dev-info">
              <span class="about-dev-name">雨凉</span>
              <span class="about-dev-role">工作室核心成员 / 节点贡献者</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
  align-items: flex-start;
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

.about-card-body {
  flex: 1;
  min-width: 0;
}

.about-card-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 6px;
}

.about-dev-item {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 18px;
  color: var(--text-primary);
  padding: 16px 0;
}

.about-dev-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.about-dev-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.about-dev-name {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
}

.about-dev-role {
  font-size: 13px;
  color: var(--text-muted);
}

.about-avatar {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  object-fit: cover;
  flex-shrink: 0;
}

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