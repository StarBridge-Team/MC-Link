<script setup lang="ts">
import { ref, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { applyPersStyle, resolveBgFile, loadPersSettings, savePersSettings, getDefaultPers } from "../../composables/usePersonalization";
import type { Personalization } from "../../composables/usePersonalization";
import Card from "../ui/Card.vue";
import Input from "../ui/Input.vue";
import Button from "../ui/Button.vue";
import Switch from "../ui/Switch.vue";
import Slider from "../ui/Slider.vue";

const props = defineProps<{
  showToast: (msg: string) => void;
}>();

const persLoading = ref(false);

const themePresets = [
  { name: "深邃蓝", color: "#0066cc" },
  { name: "翡翠绿", color: "#10b981" },
  { name: "罗兰紫", color: "#8b5cf6" },
  { name: "珊瑚红", color: "#ef4444" },
  { name: "琥珀橙", color: "#f59e0b" },
  { name: "樱花粉", color: "#ec4899" },
  { name: "极夜黑", color: "#1e1e2e" },
  { name: "冰川灰", color: "#6b7280" },
];

const effects = [
  { value: "none", label: "无" },
  { value: "transparent", label: "透明" },
  { value: "mica", label: "Mica (Windows 11)" },
  { value: "acrylic", label: "亚克力 (Windows 10)" },
  { value: "hud_window", label: "HUD Window (macOS)" },
];

const bgTypes = [
  { value: "default", label: "默认" },
  { value: "solid", label: "纯色" },
  { value: "transparent", label: "透明" },
  { value: "image", label: "图片" },
  { value: "video", label: "视频" },
];

const fitModes = [
  { value: "scale-to-fill", label: "拉伸填充" },
  { value: "aspect-fit", label: "等比适应" },
  { value: "aspect-fill", label: "等比填充" },
  { value: "width-fix", label: "宽度固定" },
  { value: "height-fix", label: "高度固定" },
];

interface BackgroundFileItem {
  name: string;
  is_video: boolean;
}

const backgroundFiles = ref<BackgroundFileItem[]>([]);
const bgFileLoading = ref(false);
const resolvedBgUrl = ref("");
const bgUrlInput = ref("");

const musicFiles = ref<BackgroundFileItem[]>([]);
const bgMusicUrlInput = ref("");

const homepageUrlInput = ref("");

// 跨组件缓存
let persCache: Personalization | null = null;

const persSettings = ref<Personalization>(getDefaultPers());

let autoSaveTimer: ReturnType<typeof setTimeout> | null = null;

onUnmounted(() => {
  if (autoSaveTimer) clearTimeout(autoSaveTimer);
});

function onPersChange() {
  applyPersStyleForSection();
  autoSave();
}

function applyPersStyleForSection() {
  applyPersStyle(persSettings.value, resolvedBgUrl.value);
}

async function loadPersonalization() {
  persLoading.value = true;
  try {
    if (persCache) {
      persSettings.value = { ...persCache };
    } else {
      const data = await loadPersSettings();
      persSettings.value = data;
      if (data.background_value && !data.background_value.startsWith('http')) {
        resolvedBgUrl.value = await resolveBgFile(data.background_value);
      }
    }
  } catch {
    // 使用默认值
  } finally {
    persLoading.value = false;
  }
  if (!persCache) applyPersStyleForSection();
  loadBackgroundFiles();
}

async function loadBackgroundFiles() {
  bgFileLoading.value = true;
  try {
    const allFiles = await invoke<BackgroundFileItem[]>("get_background_files");
    backgroundFiles.value = allFiles;
    musicFiles.value = allFiles.filter(f => {
      const ext = f.name.split('.').pop()?.toLowerCase();
      return ['mp3', 'wav', 'ogg', 'flac', 'aac', 'm4a'].includes(ext || '');
    });
  } catch {
    backgroundFiles.value = [];
    musicFiles.value = [];
  } finally {
    bgFileLoading.value = false;
  }
}

async function savePersonalization() {
  try {
    await savePersSettings(persSettings.value);
    persCache = { ...persSettings.value };
  } catch (e: any) {
    props.showToast("保存失败: " + e);
  }
}

function autoSave() {
  if (autoSaveTimer) clearTimeout(autoSaveTimer);
  autoSaveTimer = setTimeout(() => savePersonalization(), 400);
}

function selectTheme(color: string) {
  persSettings.value.theme_color = color;
  onPersChange();
}

function setEffect(effect: string) {
  persSettings.value.transparent_effect = effect;
  invoke("set_window_effect", { effect }).catch(() => {});
  onPersChange();
}

function getMusicModes() {
  const modes = [
    { value: "none", label: "无" },
    { value: "file", label: "从文件加载" },
    { value: "url", label: "从URL加载" },
  ];
  if (persSettings.value.background_type === 'video' && persSettings.value.background_value) {
    modes.push({ value: "video", label: "使用视频音频" });
  }
  return modes;
}

function selectMusicFile(name: string) {
  persSettings.value.music_value = name;
  persSettings.value.music_mode = 'file';
  bgMusicUrlInput.value = '';
  onPersChange();
  applyPersStyleForSection();
}

function applyMusicUrl() {
  const url = bgMusicUrlInput.value.trim();
  if (!url) return;
  persSettings.value.music_value = url;
  persSettings.value.music_mode = 'url';
  onPersChange();
  applyPersStyleForSection();
}

function selectBgFile(name: string, isVideo: boolean) {
  persSettings.value.background_value = name;
  persSettings.value.background_type = isVideo ? 'video' : 'image';
  bgUrlInput.value = '';
  invoke<string>("get_background_file_url", { filename: name })
    .then(path => {
      resolvedBgUrl.value = convertFileSrc(path);
      applyPersStyleForSection();
    })
    .catch(() => {
      resolvedBgUrl.value = '';
      applyPersStyleForSection();
    });
  onPersChange();
}

function selectBgUrl() {
  const url = bgUrlInput.value.trim();
  if (!url) return;
  persSettings.value.background_value = url;
  const isVideo = /\.(mp4|webm|avi|mov|mkv)(\?|$)/i.test(url) ||
    /(youtube\.com|youtu\.be|bilibili\.com)/i.test(url);
  persSettings.value.background_type = isVideo ? 'video' : 'image';
  onPersChange();
}

function applyHomepageUrl() {
  const url = homepageUrlInput.value.trim();
  if (!url) return;
  persSettings.value.homepage_value = url;
  persSettings.value.homepage_mode = 'webpage';
  onPersChange();
}

defineExpose({ loadPersonalization });
</script>

<template>
  <div v-if="persLoading" class="setting-loading">加载中...</div>
  <div v-else class="personalization-page">
    <!-- 主题色 -->
    <Card>
      <div class="pers-card-title">主题色</div>
      <div class="pers-colors">
        <div
          v-for="preset in themePresets"
          :key="preset.color"
          class="pers-color-item"
          :class="{ active: persSettings.theme_color === preset.color }"
          @click="selectTheme(preset.color)"
        >
          <div class="pers-color-swatch" :style="{ background: preset.color }">
            <i v-if="persSettings.theme_color === preset.color" class="bi bi-check-lg"></i>
          </div>
          <span class="pers-color-name">{{ preset.name }}</span>
        </div>
        <div class="pers-color-item pers-color-custom">
          <label class="pers-color-swatch pers-color-picker" :style="{ background: persSettings.theme_color }">
            <input type="color" v-model="persSettings.theme_color" class="pers-color-input" @input="onPersChange" />
            <i class="bi bi-eyedropper"></i>
          </label>
          <span class="pers-color-name">自定义</span>
        </div>
      </div>
    </Card>

    <!-- 动画 -->
    <Card>
      <div class="pers-card-title">动画</div>
      <div class="pers-row">
        <span>启用动画</span>
        <Switch v-model="persSettings.animation_enabled" @change="onPersChange" />
      </div>
      <div class="pers-row" v-if="persSettings.animation_enabled">
        <span>动画速度</span>
        <div class="pers-speed">
          <Slider v-model="persSettings.animation_speed" :min="0.25" :max="2" :step="0.25" @input="onPersChange" />
          <span class="pers-speed-label">{{ persSettings.animation_speed }}x</span>
        </div>
      </div>
    </Card>

    <!-- 透明效果 -->
    <Card>
      <div class="pers-card-title">透明效果</div>
      <div class="pers-effects">
        <div
          v-for="effect in effects"
          :key="effect.value"
          class="pers-effect-item"
          :class="{ active: persSettings.transparent_effect === effect.value }"
          @click="setEffect(effect.value)"
        >
          {{ effect.label }}
        </div>
      </div>
    </Card>

    <!-- 背景 -->
    <Card>
      <div class="pers-card-title">背景</div>
      <div class="pers-bg-types">
        <div
          v-for="bt in bgTypes"
          :key="bt.value"
          class="pers-bg-type-item"
          :class="{ active: persSettings.background_type === bt.value }"
          @click="persSettings.background_type = bt.value; onPersChange()"
        >
          {{ bt.label }}
        </div>
      </div>
      <div v-if="persSettings.background_type === 'solid'" class="pers-bg-solid">
        <input type="color" v-model="persSettings.background_value" class="pers-color-input-inline" @input="autoSave" />
        <span class="pers-color-hex">{{ persSettings.background_value || '#18191a' }}</span>
      </div>
      <div v-if="persSettings.background_type === 'image' || persSettings.background_type === 'video'" class="pers-bg-media">
        <div class="pers-bg-section">
          <div class="pers-bg-section-label">适应模式</div>
          <div class="pers-fit-modes">
            <div
              v-for="fm in fitModes"
              :key="fm.value"
              class="pers-fit-item"
              :class="{ active: persSettings.background_fit === fm.value }"
              @click="persSettings.background_fit = fm.value; onPersChange()"
            >
              {{ fm.label }}
            </div>
          </div>
        </div>
        <div class="pers-bg-section">
          <div class="pers-bg-section-label">本地文件 (Background 文件夹)</div>
          <div v-if="bgFileLoading" class="pers-bg-loading">扫描中...</div>
          <div v-else-if="backgroundFiles.length === 0" class="pers-bg-empty">暂无文件，请将图片/视频放入 Background 文件夹</div>
          <div v-else class="pers-bg-files">
            <div
              v-for="f in backgroundFiles"
              :key="f.name"
              class="pers-bg-file"
              :class="{ active: persSettings.background_value === f.name && !persSettings.background_value.startsWith('http') }"
              @click="selectBgFile(f.name, f.is_video)"
            >
              <i :class="f.is_video ? 'bi bi-film' : 'bi bi-file-earmark-image'"></i>
              <span class="pers-bg-file-name">{{ f.name }}</span>
            </div>
          </div>
        </div>
        <div class="pers-bg-section">
          <div class="pers-bg-section-label">URL 链接</div>
          <div class="pers-bg-url-row">
            <Input
              v-model="bgUrlInput"
              placeholder="输入图片/视频 URL"
              @keyup.enter="selectBgUrl"
              class="pers-bg-url-input"
            />
            <Button variant="primary" size="sm" @click="selectBgUrl">应用</Button>
          </div>
        </div>
        <div v-if="persSettings.background_value" class="pers-bg-current">
          当前: {{ persSettings.background_value }}
        </div>
      </div>
    </Card>

    <!-- 主题模式 -->
    <Card>
      <div class="pers-card-title">主题模式</div>
      <div class="pers-effects">
        <div
          v-for="mode in [{value:'system',label:'跟随系统'},{value:'light',label:'浅色'},{value:'dark',label:'深色'}]"
          :key="mode.value"
          class="pers-effect-item"
          :class="{ active: persSettings.theme_mode === mode.value }"
          @click="persSettings.theme_mode = mode.value; onPersChange()"
        >
          {{ mode.label }}
        </div>
      </div>
    </Card>

    <!-- 背景遮罩 -->
    <Card>
      <div class="pers-card-title">背景遮罩</div>
      <div class="pers-row">
        <span>启用遮罩</span>
        <Switch v-model="persSettings.background_overlay" @change="onPersChange" />
      </div>
      <div v-if="persSettings.background_overlay" class="pers-slider-row">
        <span>遮罩透明度</span>
        <Slider v-model="persSettings.background_overlay_opacity" :min="0" :max="100" @input="onPersChange" show-value />
      </div>
    </Card>

    <!-- 音乐 -->
    <Card>
      <div class="pers-card-title">背景音乐</div>
      <div class="pers-music-modes">
        <div
          v-for="m in getMusicModes()"
          :key="m.value"
          class="pers-music-mode-item"
          :class="{ active: persSettings.music_mode === m.value }"
          @click="persSettings.music_mode = m.value; onPersChange()"
        >
          {{ m.label }}
        </div>
      </div>
      <div v-if="persSettings.music_mode === 'file'" class="pers-music-file">
        <div class="pers-bg-section-label">从 Background 文件夹选择音乐文件</div>
        <div v-if="musicFiles.length === 0" class="pers-bg-empty">暂无音频文件</div>
        <div v-else class="pers-bg-files">
          <div
            v-for="f in musicFiles"
            :key="f.name"
            class="pers-bg-file"
            :class="{ active: persSettings.music_value === f.name }"
            @click="selectMusicFile(f.name)"
          >
            <i class="bi bi-file-earmark-music"></i>
            <span class="pers-bg-file-name">{{ f.name }}</span>
          </div>
        </div>
      </div>
      <div v-if="persSettings.music_mode === 'url'" class="pers-music-url">
        <div class="pers-bg-url-row">
          <Input
            v-model="bgMusicUrlInput"
            placeholder="输入音频 URL"
            @keyup.enter="applyMusicUrl"
            class="pers-bg-url-input"
          />
          <Button variant="primary" size="sm" @click="applyMusicUrl">应用</Button>
        </div>
      </div>
    </Card>

    <!-- 自定义主页 -->
    <Card>
      <div class="pers-card-title">自定义主页</div>
      <div class="pers-music-modes">
        <div
          v-for="h in [{value:'default',label:'默认'},{value:'blank',label:'空白'},{value:'webpage',label:'网页'}]"
          :key="h.value"
          class="pers-music-mode-item"
          :class="{ active: persSettings.homepage_mode === h.value }"
          @click="persSettings.homepage_mode = h.value; onPersChange()"
        >
          {{ h.label }}
        </div>
      </div>
      <div v-if="persSettings.homepage_mode === 'webpage'" class="pers-music-url">
        <div class="pers-bg-url-row">
          <Input
            v-model="homepageUrlInput"
            placeholder="输入主页 URL (https://...)"
            @keyup.enter="applyHomepageUrl"
            class="pers-bg-url-input"
          />
          <Button variant="primary" size="sm" @click="applyHomepageUrl">应用</Button>
        </div>
      </div>
    </Card>
  </div>
</template>

<style scoped>
.personalization-page {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
}

.pers-card-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 16px;
}

.pers-colors {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.pers-color-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  padding: 8px;
  border-radius: 10px;
  transition: background 0.15s ease;
  min-width: 64px;
}

.pers-color-item:hover {
  background: var(--bg-hover);
}

.pers-color-item.active {
  background: rgba(255, 255, 255, 0.08);
  border-radius: 10px;
}

@media (prefers-color-scheme: light) {
  .pers-color-item.active {
    background: rgba(0, 0, 0, 0.04);
  }
}

.pers-color-swatch {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  flex-shrink: 0;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

.pers-color-swatch .bi {
  color: #fff;
  font-size: 18px;
  font-weight: 700;
  filter: drop-shadow(0 1px 2px rgba(0,0,0,0.5));
}

.pers-color-picker {
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  cursor: pointer;
  color: #fff;
  font-size: 16px;
}

.pers-color-input {
  position: absolute;
  inset: 0;
  opacity: 0;
  cursor: pointer;
  width: 100%;
  height: 100%;
}

.pers-color-name {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}

.pers-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 0;
  font-size: 14px;
  color: var(--text-primary);
}

.pers-row + .pers-row {
  border-top: 1px solid var(--border-color);
}

.pers-speed {
  display: flex;
  align-items: center;
  gap: 10px;
}

.pers-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--accent-primary);
  cursor: pointer;
  border: 2px solid #fff;
  box-shadow: 0 1px 4px rgba(0,0,0,0.2);
}

.pers-slider::-moz-range-thumb {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--accent-primary);
  cursor: pointer;
  border: 2px solid #fff;
  box-shadow: 0 1px 4px rgba(0,0,0,0.2);
}

.pers-speed-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  min-width: 36px;
  text-align: right;
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
}

.pers-effects {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.pers-effect-item {
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: all 0.15s ease;
}

.pers-effect-item:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.pers-effect-item.active {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}


  .pers-bg-types {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.pers-bg-type-item {
  padding: 8px 20px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: all 0.15s ease;
}

.pers-bg-type-item:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.pers-bg-type-item.active {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}

.pers-bg-solid {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 12px;
}

.pers-color-input-inline {
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 8px;
  padding: 0;
  cursor: pointer;
  background: none;
}

.pers-color-input-inline::-webkit-color-swatch-wrapper {
  padding: 0;
}

.pers-color-input-inline::-webkit-color-swatch {
  border: 1px solid var(--border-color);
  border-radius: 8px;
}

.pers-color-hex {
  font-size: 13px;
  font-family: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
  color: var(--text-muted);
}

.pers-bg-media {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.pers-bg-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.pers-bg-section-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-muted);
}

.pers-fit-modes {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.pers-fit-item {
  padding: 6px 14px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: all 0.15s ease;
}

.pers-fit-item:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.pers-fit-item.active {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}

.pers-bg-loading,
.pers-bg-empty {
  font-size: 12px;
  color: var(--text-muted);
  padding: 8px 0;
}

.pers-bg-files {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  max-height: 120px;
  overflow-y: auto;
}

.pers-bg-file {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 6px;
  font-size: 12px;
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: all 0.15s ease;
  max-width: 200px;
}

.pers-bg-file:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.pers-bg-file.active {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}

.pers-bg-file i {
  font-size: 14px;
  flex-shrink: 0;
}

.pers-bg-file-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pers-bg-url-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.pers-bg-url-input {
  flex: 1;
}

.pers-bg-url-btn {
  flex-shrink: 0;
  padding: 8px 16px;
  font-size: 13px;
}

.pers-bg-current {
  font-size: 12px;
  color: var(--text-muted);
  padding: 4px 0;
  word-break: break-all;
}

.pers-toggle.active {
  background: var(--accent-primary);
}

.pers-toggle-knob {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: #fff;
  position: absolute;
  top: 2px;
  left: 2px;
  transition: left 0.2s;
  box-shadow: 0 1px 3px rgba(0,0,0,0.3);
}

.pers-toggle.active .pers-toggle-knob {
  left: 22px;
}

.pers-slider-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 8px;
  font-size: 13px;
  color: var(--text-secondary);
}

.pers-music-modes {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.pers-music-mode-item {
  padding: 6px 14px;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: all 0.15s ease;
}

.pers-music-mode-item:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.pers-music-mode-item.active {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}

.pers-music-file {
  margin-top: 10px;
}

.pers-music-url {
  margin-top: 10px;
}

@media (prefers-color-scheme: light) {
  .pers-bg-type-item {
    background: rgba(0, 0, 0, 0.04);
  }
  .pers-bg-type-item:hover {
    background: rgba(0, 0, 0, 0.08);
  }
}

.setting-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-muted);
  font-size: 14px;
}
</style>