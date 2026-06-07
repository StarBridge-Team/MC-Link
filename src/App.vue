<script setup lang="ts">
import { ref } from "vue";
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from "@tauri-apps/api/core";
import TextSidebar from './components/TextSidebar.vue';
import IconSidebar from './components/IconSidebar.vue';
import HomePage from './components/HomePage.vue';
import ConnectPage from './components/connect/ConnectPage.vue';
import RelayPage from './components/RelayPage.vue';
import AdapterPage from './components/AdapterPage.vue';
import Kaifaing from './components/Kaifaing.vue';
import SettingPage from './components/setting/SettingPage.vue';

interface Toast {
  id: number;
  msg: string;
  isError: boolean;
}

const toasts = ref<Toast[]>([]);
let toastId = 0;

const activeIcon = ref("home");
const activeText = ref("account");
const isConnected = ref(false);
const playerName = ref(localStorage.getItem("player_name") || "玩家");

// 图标切换防抖，防止快速点击卡顿
let iconChangeTimer: ReturnType<typeof setTimeout> | null = null;
function handleIconChange(icon: string) {
  if (iconChangeTimer) clearTimeout(iconChangeTimer);
  iconChangeTimer = setTimeout(() => {
    if (icon === 'setting') activeText.value = 'account';
    activeIcon.value = icon;
  }, 100);
}

function handleTextChange(text: string) {
  activeText.value = text;
}

const iconItems = [
  { id: "home", icon: "bi-house", title: "首页" },
  { id: "connect", icon: "bi-wifi", title: "联机" },
  { id: "relay", icon: "bi-hdd-network", title: "中继" },
  { id: "team", icon: "bi-flag", title: "队伍" },
  { id: "adapter", icon: "bi-plug", title: "适配器" },
  { id: "setting", icon: "bi-gear", title: "设置" },
];

function showToast(msg: string) {
  const isError = msg.includes('错误') || msg.includes('失败');
  if (toasts.value.length >= 3) {
    toasts.value.shift();
  }
  const id = ++toastId;
  toasts.value.push({ id, msg, isError });
  setTimeout(() => removeToast(id), 2000);
}

function removeToast(id: number) {
  const index = toasts.value.findIndex(t => t.id === id);
  if (index !== -1) {
    toasts.value.splice(index, 1);
  }
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
  await invoke("close_window");
}
</script>

<template>
  <div class="window-wrapper">
    <div class="titlebar" data-tauri-drag-region>
      <div class="titlebar-drag-region">
        MC Link
      </div>
      <div class="window-controls">
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
            <HomePage v-if="activeIcon === 'home'" key="home" :show-toast="showToast" :player-name="playerName" @name-change="playerName = $event" />
            <ConnectPage v-else-if="activeIcon === 'connect'" key="connect" :show-toast="showToast" :player-name="playerName" @running-change="isConnected = $event" />
            <Kaifaing v-else-if="activeIcon === 'team'" key="team" />
            <RelayPage v-else-if="activeIcon === 'relay'" key="relay" :show-toast="showToast" />
            <AdapterPage v-else-if="activeIcon === 'adapter'" key="adapter" :show-toast="showToast" />
            <SettingPage v-else-if="activeIcon === 'setting'" key="setting" :active-tab="activeText" :show-toast="showToast" @back="activeIcon = 'home'" />
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

  </div>
</template>

<style>
@import "tailwindcss";
@import url('https://fonts.googleapis.com/css2?family=Poppins:wght@300;400;500;600;700&display=swap');

:root {
  --bg-primary: #1e1e2e;
  --bg-secondary: #28283e;
  --bg-tertiary: #31314a;
  --bg-card: rgba(40, 40, 62);
  --bg-hover: rgba(255, 255, 255, 0.08);
  --bg-window: #222222;
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
}

@media (prefers-color-scheme: light) {
  :root {
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
  background: transparent;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
}

.window-wrapper::before {
  content: '';
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.2);
  pointer-events: none;
  z-index: 0;
}

@media (prefers-color-scheme: light) {
  .window-wrapper::before {
    background: transparent;
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
  border-radius: 8px;
  margin: 0 5px 5px 5px;
  background: var(--bg-window);
}

.titlebar {
  height: 36px;
  background: transparent;
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
  box-sizing: border-box;
}

.text-muted {
  color: var(--text-muted);
}

.input-group {
  margin-bottom: 20px;
}

.input-group label {
  display: block;
  margin-bottom: 8px;
  color: var(--text-secondary);
  font-weight: 500;
  font-size: 14px;
}

.input-group input {
  width: 100%;
  padding: 12px;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  box-sizing: border-box;
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  transition: all 0.2s ease;
}

.input-group input:focus {
  outline: none;
  background: rgba(255, 255, 255, 0.15);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}

.hint {
  margin: 15px 0;
  color: var(--text-primary);
  font-size: 13px;
  line-height: 1.6;
}

.btn {
  transition: all 0.2s ease;
  border-radius: 8px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 500;
  padding: 12px 24px;
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
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-primary {
  background: var(--accent-primary);
  color: #ffffff;
  box-shadow: 0 2px 8px rgba(0, 102, 204, 0.3);
}

.btn-primary:hover {
  background: var(--accent-hover);
}

.btn-danger {
  background: #ef4444;
  color: #ffffff;
  box-shadow: 0 2px 8px rgba(239, 68, 68, 0.3);
}

.btn-danger:hover {
  background: #f87171;
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

/* Animation 1: 全局页面淡出 + 上浮 */
.page-fade-enter-active {
  transition: opacity 450ms cubic-bezier(0.2, 0.8, 0.2, 1),
              transform 450ms cubic-bezier(0.2, 0.8, 0.2, 1);
}
.page-fade-enter-from {
  opacity: 0;
  transform: translateY(20px);
}
.page-fade-leave-active {
  transition: opacity 250ms ease-out, transform 250ms ease-out;
}
.page-fade-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

/* Animation 2: 通用按压反馈 */
button:active, a:active, [role="button"]:active,
.tb-btn:active, .mode-btn:active, .adapter-item:active,
.sidebar-icon-item:active, .sidebar-text-item:active {
  transform: scale(0.96) !important;
  transition: transform 120ms ease;
}

/* Animation 3: 列表交错浮出 */
@keyframes fade-up-in {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
.stagger-list > .stagger-item {
  opacity: 0;
  animation: fade-up-in 450ms cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
}
.stagger-list > .stagger-item:nth-child(1) { animation-delay: 0ms; }
.stagger-list > .stagger-item:nth-child(2) { animation-delay: 50ms; }
.stagger-list > .stagger-item:nth-child(3) { animation-delay: 100ms; }
.stagger-list > .stagger-item:nth-child(4) { animation-delay: 150ms; }
.stagger-list > .stagger-item:nth-child(5) { animation-delay: 200ms; }
.stagger-list > .stagger-item:nth-child(6) { animation-delay: 250ms; }
.stagger-list > .stagger-item:nth-child(7) { animation-delay: 300ms; }
.stagger-list > .stagger-item:nth-child(8) { animation-delay: 350ms; }
.stagger-list > .stagger-item:nth-child(9) { animation-delay: 400ms; }
.stagger-list > .stagger-item:nth-child(10) { animation-delay: 450ms; }

/* Animation 4: 状态颜色平滑过渡 */
button, a, [role="button"], input, select, textarea, label,
.tb-btn, .mode-btn, .adapter-item {
  transition: background-color 0.2s ease, color 0.2s ease, border-color 0.2s ease;
}

/* Dialog styles */
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

.dialog-input {
  box-sizing: border-box;
  width: 100%;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-size: 14px;
  letter-spacing: 1px;
  transition: border-color 0.2s ease;
}

.dialog-input:focus {
  outline: none;
  border-color: var(--accent-primary);
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 20px 24px;
}
</style>