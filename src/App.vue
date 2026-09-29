<script setup lang="ts">
import { ref, onMounted, nextTick, provide, computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { applyPersStyle } from './composables/usePersonalization';
import type { Personalization } from './composables/usePersonalization';
import { useToast } from './composables/useToast';
import { useWindowControls } from './composables/useWindowControls';
import { useAppInit } from './composables/useAppInit';
import { savePersonalization } from './lib/api/config';
import './assets/design-system.css';
import TextSidebar from './components/TextSidebar.vue';
import IconSidebar from './components/IconSidebar.vue';

const route = useRoute();
const router = useRouter();
const { showToast } = useToast();
const {
  restoreWindowState,
  setDefaultWindowSize,
  setupWindowStateListeners,
  startDrag,
  handleMinimize,
  handleMaximize,
  handleClose,
} = useWindowControls();
const { resolvedBgUrl, initialize } = useAppInit();

const isConnected = ref(false);
const playerName = ref(localStorage.getItem("player_name") || "玩家");
const homepageMode = ref("default");
const homepageValue = ref("");

const activeIcon = computed(() => {
  const name = route.name as string | undefined;
  if (name === 'setting') return 'setting';
  if (name === 'connect') return 'connect';
  return 'home';
});

function handleIconChange(icon: string) {
  if (icon === 'setting') {
    void router.push({ name: 'setting', params: { tab: 'personalization' } });
  } else {
    void router.push({ name: icon });
  }
}

function handleTextChange(text: string) {
  void router.push({ name: 'setting', params: { tab: text } });
}

function navigateTo(icon: string, text?: string) {
  if (icon === 'setting' && text) {
    void router.push({ name: 'setting', params: { tab: text } });
  } else {
    void router.push({ name: icon });
  }
}
provide('navigateTo', navigateTo);

const iconItems = [
  { id: "home", icon: "bi-house", title: "首页" },
  { id: "connect", icon: "bi-wifi", title: "联机" },
  { id: "m3", icon: "bi-palette", title: "M3 主题" },
  { id: "setting", icon: "bi-gear", title: "设置" },
];

function applyPersSettings(data: Personalization) {
  applyPersStyle(data, resolvedBgUrl.value);
  homepageMode.value = data.homepage_mode || 'default';
  homepageValue.value = data.homepage_value || '';
}

onMounted(async () => {
  await initialize((settings) => {
    if (!settings.transparent_effect || settings.transparent_effect === 'none' || settings.transparent_effect === '') {
      settings.transparent_effect = 'mica';
      savePersonalization(settings).catch(() => {});
    }
    applyPersSettings(settings);
  });

  await nextTick();
  const restored = await restoreWindowState();
  if (!restored) {
    await setDefaultWindowSize();
  }
  try {
    await setupWindowStateListeners();
  } catch { /* ignore */ }
});
</script>

<template>
  <div class="window-wrapper window-wrapper--visible">
    <div class="titlebar" data-tauri-drag-region @mousedown="startDrag">
      <div class="titlebar-drag-region">
        MC Link
      </div>
      <div class="window-controls" @mousedown.stop>
        <button class="win-btn" @click="handleMinimize" title="最小化">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <rect x="2" y="5.5" width="8" height="1" rx="0.5" fill="currentColor"/>
          </svg>
        </button>
        <button class="win-btn" @click="handleMaximize" title="最大化">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <rect x="2" y="2" width="8" height="8" rx="1" stroke="currentColor" stroke-width="1" fill="none"/>
          </svg>
        </button>
        <button class="win-btn win-btn-close" @click="handleClose" title="关闭">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <path d="M3 3L9 9M9 3L3 9" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
          </svg>
        </button>
      </div>
    </div>
    <div class="window-container">
      <div class="content">
        <IconSidebar
          :active-icon="activeIcon"
          :icon-items="iconItems"
          :player-name="playerName"
          :show-toast="showToast"
          @icon-change="handleIconChange"
          v-if="!isConnected"
        />
        <TextSidebar
          :active-icon="activeIcon"
          :active-text="(route.params.tab as string) || 'personalization'"
          @text-change="handleTextChange"
          v-if="!isConnected && activeIcon === 'setting'"
        />
        <div class="page">
          <router-view v-slot="{ Component }">
            <Transition name="page-fade" mode="out-in">
              <KeepAlive>
                <component
                  :is="Component"
                  :show-toast="showToast"
                  :player-name="playerName"
                  :is-running="isConnected"
                  :active-tab="(route.params.tab as string) || 'personalization'"
                  @name-change="playerName = $event"
                  @running-change="isConnected = $event"
                  @back="router.push({ name: 'home' })"
                  :key="route.path"
                />
              </KeepAlive>
            </Transition>
          </router-view>
        </div>
      </div>
    </div>

    <video id="bg-video" class="bg-video" autoplay loop playsinline style="display:none"></video>
    <audio id="bg-music" class="bg-music" autoplay loop style="display:none"></audio>

  </div>
</template>

<style>
@import "tailwindcss";

:root {
  --bg-primary: #1e1e2e;
  --bg-secondary: #28283e;
  --bg-tertiary: #31314a;
  --bg-card: rgba(40, 40, 62);
  --bg-hover: rgba(255, 255, 255, 0.08);
  --bg-window: #18191a;
  --text-primary: #ffffff;
  --text-secondary: #e0e0e0;
  --text-muted: #a0a0b0;
  --font-english: 'Poppins', sans-serif;
  --accent-primary: #0066cc;
  --accent-secondary: #0052a3;
  --accent-hover: #1a7ae6;
  --border-color: rgba(255, 255, 255, 0.08);
  --border-hover: rgba(255, 255, 255, 0.15);
  --shadow-accent: color-mix(in srgb, var(--accent-primary) 25%, transparent);
  --window-container-bg: transparent;
  --titlebar-bg: transparent;
  --anim-speed: 1;
  --window-margin: 0;
  --window-radius: 0;
  --window-wrapper-bg: transparent;
  --window-wrapper-bg-image: none;
  --window-wrapper-bg-fit: 100% 100%;

  /* Element Plus 主题变量覆盖 */
  --el-color-primary: var(--accent-primary);
  --el-color-primary-light-3: color-mix(in srgb, var(--accent-primary) 70%, white);
  --el-color-primary-light-5: color-mix(in srgb, var(--accent-primary) 50%, white);
  --el-color-primary-light-7: color-mix(in srgb, var(--accent-primary) 30%, white);
  --el-color-primary-light-8: color-mix(in srgb, var(--accent-primary) 20%, white);
  --el-color-primary-light-9: color-mix(in srgb, var(--accent-primary) 12%, transparent);
  --el-color-primary-dark-2: color-mix(in srgb, var(--accent-primary) 80%, black);
  --el-color-success: #22c55e;
  --el-color-warning: #eab308;
  --el-color-danger: #ef4444;
  --el-color-error: #ef4444;
  --el-color-info: #909399;
  --el-bg-color: var(--bg-secondary);
  --el-bg-color-page: var(--bg-primary);
  --el-bg-color-overlay: var(--bg-secondary);
  --el-text-color-primary: var(--text-primary);
  --el-text-color-regular: var(--text-secondary);
  --el-text-color-secondary: var(--text-muted);
  --el-text-color-placeholder: var(--text-muted);
  --el-text-color-disabled: var(--text-muted);
  --el-border-color: var(--border-color);
  --el-border-color-light: var(--border-color);
  --el-border-color-lighter: var(--border-color);
  --el-border-color-extra-light: var(--border-color);
  --el-border-color-dark: var(--border-hover);
  --el-fill-color: var(--bg-soft);
  --el-fill-color-light: var(--bg-soft);
  --el-fill-color-lighter: var(--bg-soft);
  --el-fill-color-extra-light: var(--bg-soft);
  --el-fill-color-dark: var(--bg-hover);
  --el-fill-color-blank: transparent;
  --el-border-radius-base: 16px;
  --el-border-radius-small: 16px;
  --el-border-radius-round: var(--r-full);
  --el-font-size-base: var(--fs-md);
  --el-box-shadow: var(--shadow-lg);
  --el-box-shadow-light: var(--shadow-md);
  --el-disabled-bg-color: var(--bg-soft);
  --el-disabled-text-color: var(--text-muted);
}

:root[data-theme-mode="light"] {
  --bg-primary: #F0F2F5;
  --bg-secondary: #FFFFFF;
  --bg-tertiary: #FFFFFF;
  --bg-card: #FFFFFF;
  --bg-hover: rgba(0, 0, 0, 0.05);
  --bg-window: #f5f5f5;
  --text-primary: #2c3e50;
  --text-secondary: #5a6c7d;
  --text-muted: #8b9bb0;
  --font-english: 'Poppins', sans-serif;
  --accent-primary: #0099ff;
  --accent-secondary: #0077cc;
  --accent-hover: color-mix(in srgb, var(--accent-primary) 80%, white);
  --border-color: #e8e8e8;
  --border-hover: #d0d0d0;
  --shadow-accent: color-mix(in srgb, var(--accent-primary) 25%, transparent);
}

@media (prefers-color-scheme: light) {
  :root[data-theme-mode="system"] {
    --bg-primary: #F0F2F5;
    --bg-secondary: #FFFFFF;
    --bg-tertiary: #FFFFFF;
    --bg-card: #FFFFFF;
    --bg-hover: rgba(0, 0, 0, 0.05);
    --bg-window: #f5f5f5;
    --text-primary: #2c3e50;
    --text-secondary: #5a6c7d;
    --text-muted: #8b9bb0;
    --font-english: 'Poppins', sans-serif;
    --accent-primary: #0099ff;
    --accent-secondary: #0077cc;
    --accent-hover: color-mix(in srgb, var(--accent-primary) 80%, white);
    --border-color: #e8e8e8;
    --border-hover: #d0d0d0;
    --shadow-accent: color-mix(in srgb, var(--accent-primary) 25%, transparent);
  }
}

* {
  scrollbar-width: thin;
  scrollbar-color: var(--text-muted) transparent;
  font-family: var(--font-english), -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
}

body {
  background: transparent;
  margin: 0;
  padding: 0;
  overflow: hidden;
  user-select: none;
  -webkit-user-select: none;
}

.window-wrapper {
  width: 100vw;
  height: 100vh;
  background: var(--window-wrapper-bg, transparent);
  background-image: var(--window-wrapper-bg-image, none);
  background-size: var(--window-wrapper-bg-fit, 100% 100%);
  background-position: center;
  background-repeat: no-repeat;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
  opacity: 1;
}

.window-wrapper::before {
  content: '';
  position: absolute;
  inset: 0;
  background: var(--bg-overlay, var(--transparent-overlay, rgba(0, 0, 0, 0.2)));
  pointer-events: none;
  z-index: 0;
}

@media (prefers-color-scheme: light) {
  .window-wrapper::before {
    background: var(--bg-overlay, var(--transparent-overlay, transparent));
  }
}

.window-wrapper > * {
  position: relative;
  z-index: 1;
}

.window-container {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-radius: var(--window-radius);
  margin: var(--window-margin);
  background: var(--window-container-bg);
}

.homepage-iframe {
  width: 100%;
  height: 100%;
  border: none;
  display: block;
}

.homepage-blank {
  width: 100%;
  height: 100%;
  background: transparent;
}

.bg-video {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  object-fit: cover;
  z-index: -1;
  pointer-events: none;
}

.titlebar {
  height: 36px;
  background: var(--titlebar-bg);
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 var(--sp-5);
  user-select: none;
  box-sizing: border-box;
  flex-shrink: 0;
}

.titlebar-drag-region {
  flex: 1;
  height: 100%;
  display: flex;
  align-items: center;
  -webkit-app-region: drag;
  cursor: default;
  color: var(--text-secondary);
  font-size: var(--fs-base);
  font-weight: var(--fw-medium);
}

.window-controls {
  display: flex;
  align-items: center;
  -webkit-app-region: no-drag;
  z-index: 100;
}

.win-btn {
  width: 36px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background var(--motion-base) var(--ease), color var(--motion-base) var(--ease);
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--r-sm);
  margin-left: var(--sp-1);
}

.win-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.win-btn:active {
  background: rgba(255, 255, 255, 0.05);
}

.win-btn-close:hover {
  background: rgba(239, 68, 68, 0.8);
  color: white;
}

.content {
  flex: 1;
  display: flex;
  overflow: hidden;
  min-height: 0;
}

.page {
  flex: 1;
  padding: var(--sp-7);
  overflow-y: auto;
  scrollbar-gutter: stable;
  contain: layout style;
  box-sizing: border-box;
  min-width: 0;
  min-height: 0;
}

.text-muted {
  color: var(--text-muted);
}

.hint {
  margin: var(--sp-4) 0;
  color: var(--text-primary);
  font-size: var(--fs-base);
  line-height: var(--lh-normal);
}

/* 全局按压缩放 (替代 animations.css 重复定义) */
button, a, [role="button"], input, select, textarea, label,
.tab-btn, .mode-btn, .adapter-item,
.list-card, .chip {
  transition: background-color var(--motion-base) var(--ease),
              color var(--motion-base) var(--ease),
              border-color var(--motion-base) var(--ease),
              transform var(--motion-fast) var(--ease);
}

.no-animations,
.no-animations *,
.no-animations::before,
.no-animations::after {
  transition-duration: 0s !important;
  animation-duration: 0s !important;
  animation-delay: 0s !important;
  transition-delay: 0s !important;
}

.no-animations button:active,
.no-animations a:active,
.no-animations [role="button"]:active,
.no-animations .tab-btn:active,
.no-animations .mode-btn:active,
.no-animations .adapter-item:active,
.no-animations .sidebar-icon-item:active,
.no-animations .sidebar-text-item:active,
.no-animations .list-card:active,
.no-animations .chip:active {
  transform: none !important;
}
</style>
