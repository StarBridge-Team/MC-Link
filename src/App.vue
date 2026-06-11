<script setup lang="ts">
import { ref, onMounted, onUnmounted, onDeactivated, nextTick } from "vue";
import { getCurrentWindow, LogicalPosition } from '@tauri-apps/api/window';
import { invoke } from "@tauri-apps/api/core";
import { applyPersStyle, resolveBgFile } from './composables/usePersonalization';
import type { Personalization } from './composables/usePersonalization';
import {
  tryInjectCachedFonts, tryInjectCachedIcons,
  injectGoogleFontsCss, injectBiCss,
  cacheFontsCss, cacheBiCss, isFullyCached,
} from './lib/resourceCache';
import SplashScreen from './components/SplashScreen.vue';
import TextSidebar from './components/TextSidebar.vue';
import IconSidebar from './components/IconSidebar.vue';
import HomePage from './components/HomePage.vue';
import ConnectPage from './components/connect/ConnectPage.vue';
import RelayPage from './components/RelayPage.vue';
import SettingPage from './components/setting/SettingPage.vue';

interface Toast {
  id: number;
  msg: string;
  isError: boolean;
}

const toasts = ref<Toast[]>([]);
let toastId = 0;
const toastTimers: ReturnType<typeof setTimeout>[] = [];

const activeIcon = ref("home");
const activeText = ref("account");
const isConnected = ref(false);
const appReady = ref(false);
const loadingStep = ref("正在初始化...");
const appVersion = ref("");
const playerName = ref(localStorage.getItem("player_name") || "玩家");
const resolvedBgUrl = ref("");
const homepageMode = ref("default");
const homepageValue = ref("");

function handleIconChange(icon: string) {
  if (icon === 'setting') activeText.value = 'account';
  activeIcon.value = icon;
}

function handleTextChange(text: string) {
  activeText.value = text;
}

const iconItems = [
  { id: "home", icon: "bi-house", title: "首页" },
  { id: "connect", icon: "bi-wifi", title: "联机" },
  { id: "relay", icon: "bi-hdd-network", title: "中继" },
  { id: "setting", icon: "bi-gear", title: "设置" },
];

function showToast(msg: string) {
  const isError = msg.includes('错误') || msg.includes('失败');
  if (toasts.value.length >= 3) {
    toasts.value.shift();
  }
  const id = ++toastId;
  toasts.value.push({ id, msg, isError });
  const timerId = window.setTimeout(() => removeToast(id), 2000);
  toastTimers.push(timerId);
}

function removeToast(id: number) {
  const index = toasts.value.findIndex(t => t.id === id);
  if (index !== -1) {
    toasts.value.splice(index, 1);
  }
}

function startDrag() {
  invoke("drag_window");
}

async function handleMinimize() {
  try {
    const window = await getCurrentWindow();
    await window.minimize();
  } catch (error) {
    console.error('Minimize error:', error);
  }
}

async function handleMaximize() {
  try {
    const window = await getCurrentWindow();
    if (await window.isMaximized()) {
      await window.unmaximize();
    } else {
      await window.maximize();
    }
  } catch (error) {
    console.error('Maximize error:', error);
  }
}

async function handleClose() {
  await saveWindowState();
  await invoke("close_window");
}

function applyPersSettings(data: Personalization) {
  applyPersStyle(data, resolvedBgUrl.value);
  // 同步主页设置
  homepageMode.value = data.homepage_mode || 'default';
  homepageValue.value = data.homepage_value || '';
}

/* ---- 窗口状态记忆 ---- */
interface WindowState {
  x: number;
  y: number;
  width: number;
  height: number;
}

const WINDOW_STATE_KEY = "window_state";
let saveTimer: ReturnType<typeof setTimeout> | null = null;

async function saveWindowState() {
  try {
    const window = getCurrentWindow();
    const pos = await window.outerPosition();
    const size = await window.outerSize();
    if (size.width < 100 || size.height < 100) return; // 忽略异常尺寸
    localStorage.setItem(
      WINDOW_STATE_KEY,
      JSON.stringify({ x: pos.x, y: pos.y, width: size.width, height: size.height } satisfies WindowState)
    );
  } catch { /* ignore */ }
}

function debouncedSaveState() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(saveWindowState, 500);
}

async function restoreWindowState() {
  const saved = localStorage.getItem(WINDOW_STATE_KEY);
  if (!saved) return false;
  try {
    const s: WindowState = JSON.parse(saved);
    if (s.width < 100 || s.height < 100) return false;
    await invoke("resize_window", { width: s.width, height: s.height, minWidth: 700, minHeight: 500, center: false });
    const window = getCurrentWindow();
    await window.setPosition(new LogicalPosition(s.x, s.y));
    return true;
  } catch {
    return false;
  }
}

async function setDefaultWindowSize() {
  await invoke("resize_window", { width: 900, height: 650, minWidth: 700, minHeight: 500, center: true });
  await saveWindowState();
}/* -------------------- */

onMounted(async () => {
  const win = getCurrentWindow();

  // 0. 立即获取版本号 + 尽早加载个性化配置
  try {
    appVersion.value = await invoke<string>("get_app_version");
  } catch { /* ignore */ }
  try {
    const earlyPers = await invoke<Personalization>("get_personalization");
    applyPersStyle(earlyPers);
  } catch { /* ignore */ }

  // 1. 从缓存注入字体和图标（无网络，同步操作）
  loadingStep.value = "正在读取缓存...";
  const fontsOk = tryInjectCachedFonts();
  const iconsOk = tryInjectCachedIcons();

  // 2. 后端加载全部配置 + 首次运行时下载资源
  loadingStep.value = "正在加载资源...";
  interface AppInitData {
    personalization: Personalization;
    default_effect: string;
    relays: unknown[];
    app_version: string;
    tauri_version: string;
    google_fonts_css?: string | null;
    bootstrap_icons_css?: string | null;
  }
  let data: AppInitData | null = null;

  if (isFullyCached()) {
    // 资源已缓存，只需配置（同步命令，极快）
    try {
      data = await invoke<AppInitData>("init_app");
    } catch { /* ignore */ }
  } else {
    // 首次启动：后端并行加载配置 + 下载字体图标
    try {
      data = await invoke<AppInitData>("prepare_app");
    } catch { /* ignore */ }

    // 将后端下载的资源写入本地缓存
    if (data) {
      if (data.google_fonts_css) {
        cacheFontsCss(data.google_fonts_css);
        if (!fontsOk) injectGoogleFontsCss(data.google_fonts_css);
      }
      if (data.bootstrap_icons_css) {
        cacheBiCss(data.bootstrap_icons_css);
        if (!iconsOk) injectBiCss(data.bootstrap_icons_css);
      }
    }
  }

  // 3. 应用个性化配置
  loadingStep.value = "正在应用个性化...";
  if (data) {
    const settings = data.personalization;
    const defaultEffect = data.default_effect;

    if (!settings.transparent_effect || settings.transparent_effect === 'none' || settings.transparent_effect === '') {
      settings.transparent_effect = defaultEffect;
      invoke("save_personalization", { settings }).catch(() => {});
    }
    if (settings.background_value && !settings.background_value.startsWith('http')) {
      resolvedBgUrl.value = await resolveBgFile(settings.background_value);
    }
    applyPersSettings(settings);
    if (settings.transparent_effect && settings.transparent_effect !== 'none') {
      invoke("set_window_effect", { effect: settings.transparent_effect }).catch(() => {});
    }
  }

  // 4. 初始化适配器（陶瓦联机自动安装/启动）
  loadingStep.value = "正在准备适配器...";
  try {
    await invoke("adapter_startup_init");
  } catch { /* ignore */ }

  // 5. 恢复窗口至正常尺寸（优先读取记忆状态）
  loadingStep.value = "准备就绪";
  await nextTick();
  const restored = await restoreWindowState();
  if (!restored) {
    await setDefaultWindowSize();
  }
  // 监听窗口变化，记忆位置/大小
  try {
    await win.onResized(debouncedSaveState);
    await win.onMoved(debouncedSaveState);
  } catch { /* ignore */ }
  await nextTick();

  // 5. 一切就绪，先渲染主界面再退出闪屏
  appReady.value = true;
  await nextTick();
});

function clearAllTimers() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = null;
  toastTimers.forEach(t => clearTimeout(t));
  toastTimers.length = 0;
}

onUnmounted(clearAllTimers);
onDeactivated(clearAllTimers);
</script>

<template>
  <SplashScreen :ready="appReady" :version="appVersion" :step="loadingStep" />
  <div class="window-wrapper" :class="{ 'window-wrapper--visible': appReady }">
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
          :active-text="activeText"
          @text-change="handleTextChange"
          v-if="!isConnected && activeIcon === 'setting'"
        />
        <div class="page">
          <Transition name="page-fade" mode="out-in">
            <KeepAlive>
              <template v-if="activeIcon === 'home' && homepageMode === 'blank'" key="home-blank">
                <div class="homepage-blank"></div>
              </template>
              <iframe v-else-if="activeIcon === 'home' && homepageMode === 'webpage'" key="home-webpage" class="homepage-iframe" :src="homepageValue" frameborder="0" allowfullscreen></iframe>
              <HomePage v-else-if="activeIcon === 'home'" key="home" :show-toast="showToast" :player-name="playerName" @name-change="playerName = $event" />
              <ConnectPage v-else-if="activeIcon === 'connect'" key="connect" :show-toast="showToast" :player-name="playerName" @running-change="isConnected = $event" />
              <RelayPage v-else-if="activeIcon === 'relay'" key="relay" :show-toast="showToast" />
              <SettingPage v-else-if="activeIcon === 'setting'" key="setting" :active-tab="activeText" :show-toast="showToast" :player-name="playerName" @name-change="playerName = $event" @back="activeIcon = 'home'" />
            </KeepAlive>
          </Transition>
        </div>
      </div>
    </div>

    <div class="toast-container">
      <TransitionGroup name="toast">
        <div
          v-for="toast in toasts"
          :key="toast.id"
          class="message"
          :class="{ error: toast.isError }"
        >
          {{ toast.msg }}
        </div>
      </TransitionGroup>
    </div>

    <!-- 背景视频 -->
    <video id="bg-video" class="bg-video" autoplay loop playsinline style="display:none"></video>
    <!-- 背景音乐 -->
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
  --shadow-accent: rgba(0, 102, 204, 0.25);
  --window-container-bg: var(--bg-window);
  --titlebar-bg: transparent;
  --anim-speed: 1;
  --window-margin: 0 5px 5px 5px;
  --window-radius: 8px;
  --window-wrapper-bg: transparent;
  --window-wrapper-bg-image: none;
  --window-wrapper-bg-fit: 100% 100%;
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
    --accent-hover: #33adff;
    --border-color: #e8e8e8;
    --border-hover: #d0d0d0;
    --shadow-accent: rgba(0, 153, 255, 0.15);
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
      --accent-hover: #33adff;
      --border-color: #e8e8e8;
      --border-hover: #d0d0d0;
      --shadow-accent: rgba(0, 153, 255, 0.15);
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
  opacity: 0;
  transition: opacity calc(0.4s * var(--anim-speed, 1)) ease;
}

.window-wrapper--visible {
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
  padding: 0 16px;
  user-select: none;
  box-sizing: border-box;
}

.titlebar-drag-region {
  flex: 1;
  height: 100%;
  display: flex;
  align-items: center;
  -webkit-app-region: drag;
  cursor: default;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
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
  transition: all 0.15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  margin-left: 4px;
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
}

.page {
  flex: 1;
  padding: 30px;
  overflow-y: auto;
  scrollbar-gutter: stable;
  contain: layout style;
  box-sizing: border-box;
}

.text-muted {
  color: var(--text-muted);
}

.hint {
  margin: 15px 0;
  color: var(--text-primary);
  font-size: 13px;
  line-height: 1.6;
}

.toast-container {
  position: fixed;
  bottom: 15px;
  left: 15px;
  display: flex;
  flex-direction: column-reverse;
  gap: 6px;
  z-index: 1000;
}

.message {
  padding: 8px 16px;
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  max-width: 360px;
  backdrop-filter: blur(10px);
  font-size: 13px;
  line-height: 1.4;
}

.message.error {
  background: rgba(239, 68, 68, 0.2);
}

.toast-enter-active {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.toast-leave-active {
  transition: all 0.2s ease-out;
  position: absolute;
  right: auto;
  bottom: auto;
}

.toast-move {
  transition: transform 0.2s ease-out;
}

.toast-enter-from {
  opacity: 0;
  transform: translateX(-30px);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(-20px) scale(0.95);
}


button, a, [role="button"], input, select, textarea, label,
.tb-btn, .mode-btn, .adapter-item {
  transition: background-color 0.2s ease, color 0.2s ease, border-color 0.2s ease;
}


.dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  backdrop-filter: blur(4px);
}

.dialog-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 16px;
  width: 460px;
  max-height: 85vh;
  overflow-y: auto;
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.35);
}

.dialog-card-small {
  width: 380px;
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 20px 24px 0;
}

.dialog-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-close {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  transition: background-color 0.2s ease, color 0.2s ease;
}

.dialog-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.dialog-section {
  padding: 16px 24px 0;
}

.dialog-label {
  display: block;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.dialog-desc {
  font-size: 11.5px;
  color: var(--text-muted);
  margin: 8px 0 0;
  line-height: 1.4;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 20px 24px;
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
.no-animations .tb-btn:active,
.no-animations .mode-btn:active,
.no-animations .adapter-item:active,
.no-animations .sidebar-icon-item:active,
.no-animations .sidebar-text-item:active {
  transform: none !important;
}
</style>