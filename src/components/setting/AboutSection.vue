<script setup lang="ts">
import { ref, onMounted } from "vue";
import { openUrl } from '@tauri-apps/plugin-opener';
import { convertFileSrc } from '@tauri-apps/api/core';
import { version as vueVersion } from 'vue';
import { getAppVersion, getTauriVersion, getAssetUrl } from "../../lib/api/app";

const appVersion = ref("");
const tauriVersion = ref("");
const aboutLoading = ref(true);
const iconUrl = ref("");

const developers = [
  { name: "粗狗", role: "项目发起人 / 核心开发", avatar: "http://q.qlogo.cn/g?b=qq&nk=196783749&s=640" },
  { name: "oiiaio猫", role: "Python开发", avatar: "http://q.qlogo.cn/g?b=qq&nk=3995090331&s=640" },
  { name: "", role: ""}
];

const contributors = [
  { name: "烤鱼", avatar: "http://q.qlogo.cn/g?b=qq&nk=1508668973&s=640" },
  { name: "雨凉", avatar: "http://q.qlogo.cn/g?b=qq&nk=3680388074&s=640" },
];

const showContributors = ref(false);

onMounted(async () => {
  try {
    const [v, tv] = await Promise.all([
      getAppVersion(),
      getTauriVersion(),
    ]);
    appVersion.value = v;
    tauriVersion.value = tv;
  } catch {
    appVersion.value = "?";
    tauriVersion.value = "?";
  }

  // 图标走本地缓存（prepare_app 已下载到 Assets/icons/）
  try {
    const localPath = await getAssetUrl("icons/mc-link-icon.png");
    if (localPath) iconUrl.value = convertFileSrc(localPath);
  } catch {
    /* ignore */
  }

  aboutLoading.value = false;
});
</script>

<template>
  <div class="about-page">
    <!-- 应用信息 -->
    <el-card class="about-card" shadow="never">
      <div class="app-info">
        <div class="app-icon">
          <img v-if="iconUrl" :src="iconUrl" alt="MC Link" class="app-icon__img" />
        </div>
        <div class="app-meta">
          <div class="app-title-row">
            <span class="app-name">MC Link</span>
            <el-tag type="info" size="small" effect="light">{{ aboutLoading ? '...' : appVersion }}</el-tag>
          </div>
          <div class="app-version-row">
            <span class="meta-item">
              <span class="meta-label">Vue</span>
              <span class="meta-value">{{ vueVersion }}</span>
            </span>
            <span class="meta-divider"></span>
            <span class="meta-item">
              <span class="meta-label">Tauri</span>
              <span class="meta-value">{{ tauriVersion }}</span>
            </span>
          </div>
          <div class="app-links">
            <a class="app-link" @click="openUrl('https://mclink.nsrwz.cn')">
              <i class="bi bi-globe2"></i>
              mclink.nsrwz.cn
            </a>
            <a class="app-link" @click="openUrl('https://github.com/DogerMMC/mc-link')">
              <i class="bi bi-github"></i>
              DogerMMC/mc-link
            </a>
          </div>
        </div>
      </div>
    </el-card>

    <!-- 开发者 -->
    <el-card class="about-card" shadow="never">
      <div class="section-title">开发者</div>
      <div class="dev-list">
        <div v-for="d in developers" :key="d.name" class="dev-item">
          <img class="dev-item__avatar" :src="d.avatar" :alt="d.name" />
          <div class="dev-item__info">
            <div class="dev-item__name">{{ d.name }}</div>
            <div class="dev-item__role">{{ d.role }}</div>
          </div>
        </div>
      </div>
    </el-card>

    <!-- 鸣谢 -->
    <el-card class="about-card" shadow="never">
      <template #header>
        <div class="collapsible-header" @click="showContributors = !showContributors">
          <span class="section-title">鸣谢</span>
          <i class="bi" :class="showContributors ? 'bi-chevron-up' : 'bi-chevron-down'"></i>
        </div>
      </template>
      <el-collapse-transition>
        <div v-show="showContributors" class="contributor-grid">
          <div v-for="c in contributors" :key="c.name" class="contributor-item">
            <img class="contributor-avatar" :src="c.avatar" :alt="c.name" :title="c.name" />
          </div>
        </div>
      </el-collapse-transition>
    </el-card>
  </div>
</template>

<style scoped>
.about-page {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
  overflow-y: auto;
  min-height: 0;
}

.about-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--r-lg);
}

.about-card :deep(.el-card__header) {
  padding: var(--sp-3) var(--sp-5);
  border-bottom: 1px solid var(--border-color);
}

.about-card :deep(.el-card__body) {
  padding: var(--sp-5);
}

.collapsible-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  user-select: none;
}

.collapsible-header .bi {
  color: var(--text-muted);
  transition: transform var(--motion-base) var(--ease);
}

/* 应用信息 */
.app-info {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-5);
}
.app-icon {
  width: 64px;
  height: 64px;
  border-radius: var(--r-lg);
  overflow: hidden;
  flex-shrink: 0;
}
.app-icon__img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.app-meta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.app-title-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  flex-wrap: wrap;
}
.app-name {
  font-size: var(--fs-3xl);
  font-weight: var(--fw-bold);
  color: var(--text-primary);
}
.app-version-row {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}
.meta-item {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
}
.meta-label {
  font-size: var(--fs-sm);
  color: var(--text-muted);
  font-weight: var(--fw-medium);
}
.meta-value {
  font-size: var(--fs-base);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  font-family: var(--font-mono);
}
.meta-divider {
  width: 1px;
  height: 14px;
  background: var(--border-color);
}
.app-links {
  display: flex;
  align-items: center;
  gap: var(--sp-5);
  flex-wrap: wrap;
}
.app-link {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  font-size: var(--fs-sm);
  color: var(--text-muted);
  cursor: pointer;
  text-decoration: none;
  transition: color var(--motion-base) var(--ease);
}
.app-link:hover {
  color: var(--accent-primary);
}
.app-link i {
  font-size: var(--fs-md);
}

.section-title {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  margin-bottom: var(--sp-3);
}

/* 开发者列表 */
.dev-list {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
.dev-item {
  display: flex;
  align-items: center;
  gap: var(--sp-4);
  padding: var(--sp-3) 0;
}
.dev-item + .dev-item {
  border-top: 1px solid var(--border-color);
}
.dev-item__avatar {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  object-fit: cover;
  flex-shrink: 0;
}
.dev-item__info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.dev-item__name {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
}
.dev-item__role {
  font-size: var(--fs-base);
  color: var(--text-muted);
}

/* 鸣谢头像 */
.contributor-grid {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-3);
}
.contributor-item {
  flex: 0 0 auto;
}
.contributor-avatar {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  object-fit: cover;
  display: block;
  transition: transform var(--motion-base) var(--ease),
              box-shadow var(--motion-base) var(--ease);
}
.contributor-avatar:hover {
  transform: scale(1.15);
  box-shadow: 0 0 0 2px var(--accent-primary);
}
</style>
