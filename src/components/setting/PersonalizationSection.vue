<script setup lang="ts">
import { ref, onUnmounted } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { applyPersStyle, resolveBgFile, loadPersSettings, savePersSettings, getDefaultPers } from "../../composables/usePersonalization";
import type { Personalization } from "../../composables/usePersonalization";
import { getBackgroundFiles, getBackgroundFileUrl, setWindowEffect } from "../../lib/api/settings";

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
  { value: "mica", label: "Mica (Win11)" },
  { value: "acrylic", label: "亚克力 (Win10)" },
  { value: "hud_window", label: "HUD (macOS)" },
];

const bgTypes = [
  { value: "default", label: "默认", icon: "bi bi-app" },
  { value: "solid", label: "纯色", icon: "bi bi-square-fill" },
  { value: "image", label: "图片", icon: "bi bi-image" },
  { value: "video", label: "视频", icon: "bi bi-film" },
];

const fitModes = [
  { value: "scale-to-fill", label: "拉伸填充" },
  { value: "aspect-fit", label: "等比适应" },
  { value: "aspect-fill", label: "等比填充" },
  { value: "width-fix", label: "宽度固定" },
  { value: "height-fix", label: "高度固定" },
];

const themeModes = [
  { value: "system", label: "跟随系统", icon: "bi bi-circle-half" },
  { value: "light", label: "浅色", icon: "bi bi-sun" },
  { value: "dark", label: "深色", icon: "bi bi-moon-stars" },
];

const homepageModes = [
  { value: "default", label: "默认" },
  { value: "blank", label: "空白" },
  { value: "webpage", label: "网页" },
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
    // 回填 URL 输入框，避免已保存的 URL 设置在界面上显示为空
    const d = persSettings.value;
    if (d.background_value?.startsWith('http')) bgUrlInput.value = d.background_value;
    if (d.music_mode === 'url' && d.music_value) bgMusicUrlInput.value = d.music_value;
    if (d.homepage_mode === 'webpage' && d.homepage_value) homepageUrlInput.value = d.homepage_value;
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
    const allFiles = await getBackgroundFiles();
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

function selectThemeMode(mode: string) {
  persSettings.value.theme_mode = mode;
  onPersChange();
}

function setEffect(effect: string) {
  persSettings.value.transparent_effect = effect;
  setWindowEffect(effect).catch(() => {});
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
}

function applyMusicUrl() {
  const url = bgMusicUrlInput.value.trim();
  if (!url) return;
  persSettings.value.music_value = url;
  persSettings.value.music_mode = 'url';
  onPersChange();
}

function selectBgFile(name: string, isVideo: boolean) {
  persSettings.value.background_value = name;
  persSettings.value.background_type = isVideo ? 'video' : 'image';
  bgUrlInput.value = '';
  getBackgroundFileUrl(name)
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
  <div v-if="persLoading" class="loading-text">
    <el-icon class="is-loading"><i class="bi bi-arrow-repeat" /></el-icon>
    <span>加载中...</span>
  </div>
  <div v-else class="pers-grid">
    <!-- 外观：主题色 + 主题模式 -->
    <section class="pcard pcard--wide">
      <header class="pcard__head">
        <span class="pcard__icon"><i class="bi bi-palette"></i></span>
        <div class="pcard__titles">
          <h3 class="pcard__title">外观</h3>
          <p class="pcard__desc">主题色与明暗模式，改动即时生效并自动保存</p>
        </div>
      </header>
      <div class="pcard__body">
        <div class="color-grid">
          <div
            v-for="preset in themePresets"
            :key="preset.color"
            class="color-item"
            :class="{ 'is-active': persSettings.theme_color === preset.color }"
            @click="selectTheme(preset.color)"
          >
            <div class="color-item__swatch" :style="{ background: preset.color }">
              <i v-if="persSettings.theme_color === preset.color" class="bi bi-check-lg"></i>
            </div>
            <span class="color-item__name">{{ preset.name }}</span>
          </div>
          <div class="color-item color-item--custom">
            <div class="color-item__swatch color-item__picker" :style="{ background: persSettings.theme_color }">
              <el-color-picker
                v-model="persSettings.theme_color"
                size="small"
                class="color-item__input"
                @change="onPersChange"
              />
              <i class="bi bi-eyedropper"></i>
            </div>
            <span class="color-item__name">自定义</span>
          </div>
        </div>
        <div class="pcard__divider"></div>
        <div class="mode-tiles">
          <button
            v-for="mode in themeModes"
            :key="mode.value"
            type="button"
            class="mode-tile"
            :class="{ 'is-active': persSettings.theme_mode === mode.value }"
            @click="selectThemeMode(mode.value)"
          >
            <i :class="mode.icon"></i>
            <span>{{ mode.label }}</span>
          </button>
        </div>
      </div>
    </section>

    <!-- 动画 -->
    <section class="pcard">
      <header class="pcard__head">
        <span class="pcard__icon"><i class="bi bi-magic"></i></span>
        <div class="pcard__titles">
          <h3 class="pcard__title">动画</h3>
          <p class="pcard__desc">界面过渡与动效</p>
        </div>
        <el-switch v-model="persSettings.animation_enabled" @change="onPersChange" />
      </header>
      <div v-if="persSettings.animation_enabled" class="pcard__body">
        <div class="field-row">
          <span class="field-row__title">动画速度</span>
          <div class="speed-control">
            <el-slider
              v-model="persSettings.animation_speed"
              :min="0.25"
              :max="2"
              :step="0.25"
              class="speed-slider"
              @input="onPersChange"
            />
            <span class="speed-control__label">{{ persSettings.animation_speed }}x</span>
          </div>
        </div>
      </div>
    </section>

    <!-- 透明效果 -->
    <section class="pcard">
      <header class="pcard__head">
        <span class="pcard__icon"><i class="bi bi-droplet-half"></i></span>
        <div class="pcard__titles">
          <h3 class="pcard__title">透明效果</h3>
          <p class="pcard__desc">窗口材质，依赖操作系统支持</p>
        </div>
      </header>
      <div class="pcard__body">
        <el-radio-group
          v-model="persSettings.transparent_effect"
          class="chip-group"
          @change="setEffect(persSettings.transparent_effect)"
        >
          <el-radio-button
            v-for="effect in effects"
            :key="effect.value"
            :value="effect.value"
          >
            {{ effect.label }}
          </el-radio-button>
        </el-radio-group>
      </div>
    </section>

    <!-- 背景 -->
    <section class="pcard pcard--wide">
      <header class="pcard__head">
        <span class="pcard__icon"><i class="bi bi-image"></i></span>
        <div class="pcard__titles">
          <h3 class="pcard__title">背景</h3>
          <p class="pcard__desc">纯色、本地图片/视频或网络资源</p>
        </div>
      </header>
      <div class="pcard__body">
        <el-radio-group
          v-model="persSettings.background_type"
          class="chip-group"
          @change="onPersChange"
        >
          <el-radio-button
            v-for="bt in bgTypes"
            :key="bt.value"
            :value="bt.value"
          >
            <i :class="bt.icon" class="chip-icon"></i>{{ bt.label }}
          </el-radio-button>
        </el-radio-group>

        <div v-if="persSettings.background_type === 'solid'" class="bg-solid">
          <el-color-picker
            v-model="persSettings.background_value"
            size="default"
            @change="onPersChange"
          />
          <span class="color-hex">{{ persSettings.background_value || '#18191a' }}</span>
        </div>

        <div v-if="persSettings.background_type === 'image' || persSettings.background_type === 'video'" class="bg-media">
          <div class="bg-section">
            <div class="section-subtitle">适应模式</div>
            <el-radio-group
              v-model="persSettings.background_fit"
              class="chip-group chip-group--sm"
              @change="onPersChange"
            >
              <el-radio-button
                v-for="fm in fitModes"
                :key="fm.value"
                :value="fm.value"
                size="small"
              >
                {{ fm.label }}
              </el-radio-button>
            </el-radio-group>
          </div>
          <div class="bg-section">
            <div class="section-subtitle">本地文件 (Background 文件夹)</div>
            <div v-if="bgFileLoading" class="bg-empty">扫描中...</div>
            <div v-else-if="backgroundFiles.length === 0" class="bg-empty">暂无文件，请将图片/视频放入 Background 文件夹</div>
            <div v-else class="bg-files">
              <div
                v-for="f in backgroundFiles"
                :key="f.name"
                :class="['bg-file', { 'is-active': persSettings.background_value === f.name && !persSettings.background_value.startsWith('http') }]"
                @click="selectBgFile(f.name, f.is_video)"
              >
                <i :class="f.is_video ? 'bi bi-film' : 'bi bi-file-earmark-image'"></i>
                <span class="bg-file__name">{{ f.name }}</span>
              </div>
            </div>
          </div>
          <div class="bg-section">
            <div class="section-subtitle">URL 链接</div>
            <div class="bg-url-row">
              <el-input
                v-model="bgUrlInput"
                placeholder="输入图片/视频 URL"
                @keyup.enter="selectBgUrl"
              />
              <el-button type="primary" @click="selectBgUrl">应用</el-button>
            </div>
          </div>
          <div v-if="persSettings.background_value" class="bg-current">
            当前: {{ persSettings.background_value }}
          </div>
        </div>
      </div>
    </section>

    <!-- 背景遮罩 -->
    <section class="pcard">
      <header class="pcard__head">
        <span class="pcard__icon"><i class="bi bi-layers-half"></i></span>
        <div class="pcard__titles">
          <h3 class="pcard__title">背景遮罩</h3>
          <p class="pcard__desc">在背景上叠加暗色遮罩以提升可读性</p>
        </div>
        <el-switch v-model="persSettings.background_overlay" @change="onPersChange" />
      </header>
      <div v-if="persSettings.background_overlay" class="pcard__body">
        <div class="field-row">
          <span class="field-row__title">遮罩透明度</span>
          <div class="speed-control">
            <el-slider
              v-model="persSettings.background_overlay_opacity"
              :min="0"
              :max="100"
              class="speed-slider"
              @input="onPersChange"
            />
            <span class="speed-control__label">{{ persSettings.background_overlay_opacity }}%</span>
          </div>
        </div>
      </div>
    </section>

    <!-- 背景音乐 -->
    <section class="pcard">
      <header class="pcard__head">
        <span class="pcard__icon"><i class="bi bi-music-note-beamed"></i></span>
        <div class="pcard__titles">
          <h3 class="pcard__title">背景音乐</h3>
          <p class="pcard__desc">循环播放本地或网络音频</p>
        </div>
      </header>
      <div class="pcard__body">
        <el-radio-group
          v-model="persSettings.music_mode"
          class="chip-group"
          @change="onPersChange"
        >
          <el-radio-button
            v-for="m in getMusicModes()"
            :key="m.value"
            :value="m.value"
          >
            {{ m.label }}
          </el-radio-button>
        </el-radio-group>
        <div v-if="persSettings.music_mode === 'file'" class="bg-section">
          <div class="section-subtitle">从 Background 文件夹选择音乐文件</div>
          <div v-if="musicFiles.length === 0" class="bg-empty">暂无音频文件</div>
          <div v-else class="bg-files">
            <div
              v-for="f in musicFiles"
              :key="f.name"
              :class="['bg-file', { 'is-active': persSettings.music_value === f.name }]"
              @click="selectMusicFile(f.name)"
            >
              <i class="bi bi-file-earmark-music"></i>
              <span class="bg-file__name">{{ f.name }}</span>
            </div>
          </div>
        </div>
        <div v-if="persSettings.music_mode === 'url'" class="bg-section">
          <div class="bg-url-row">
            <el-input
              v-model="bgMusicUrlInput"
              placeholder="输入音频 URL"
              @keyup.enter="applyMusicUrl"
            />
            <el-button type="primary" @click="applyMusicUrl">应用</el-button>
          </div>
        </div>
      </div>
    </section>

    <!-- 自定义主页 -->
    <section class="pcard pcard--wide">
      <header class="pcard__head">
        <span class="pcard__icon"><i class="bi bi-house-gear"></i></span>
        <div class="pcard__titles">
          <h3 class="pcard__title">自定义主页</h3>
          <p class="pcard__desc">启动时显示的首页内容</p>
        </div>
      </header>
      <div class="pcard__body">
        <el-radio-group
          v-model="persSettings.homepage_mode"
          class="chip-group"
          @change="onPersChange"
        >
          <el-radio-button
            v-for="h in homepageModes"
            :key="h.value"
            :value="h.value"
          >
            {{ h.label }}
          </el-radio-button>
        </el-radio-group>
        <div v-if="persSettings.homepage_mode === 'webpage'" class="bg-section">
          <div class="bg-url-row">
            <el-input
              v-model="homepageUrlInput"
              placeholder="输入主页 URL (https://...)"
              @keyup.enter="applyHomepageUrl"
            />
            <el-button type="primary" @click="applyHomepageUrl">应用</el-button>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.pers-grid {
  flex: 1;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-5);
  align-content: start;
  overflow-y: auto;
  min-height: 0;
  padding-bottom: var(--sp-5);
}

@media (max-width: 900px) {
  .pers-grid {
    grid-template-columns: 1fr;
  }
}

/* 卡片 */
.pcard {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 16px;
  padding: var(--sp-6);
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  transition: box-shadow var(--motion-base) var(--ease),
              border-color var(--motion-base) var(--ease),
              transform var(--motion-base) var(--ease);
}

.pcard:hover {
  border-color: color-mix(in srgb, var(--accent-primary) 35%, var(--border-color));
  box-shadow: 0 4px 20px color-mix(in srgb, var(--accent-primary) 8%, transparent);
}

.pcard--wide {
  grid-column: 1 / -1;
}

.pcard__head {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}

.pcard__icon {
  width: 38px;
  height: 38px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
  color: var(--accent-primary);
  font-size: var(--fs-lg);
}

.pcard__titles {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.pcard__title {
  margin: 0;
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  line-height: 1.3;
}

.pcard__desc {
  margin: 0;
  font-size: var(--fs-xs);
  color: var(--text-muted);
  line-height: 1.4;
}

.pcard__body {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}

.pcard__divider {
  height: 1px;
  background: var(--border-color);
}

.section-subtitle {
  font-size: var(--fs-sm);
  font-weight: var(--fw-medium);
  color: var(--text-secondary);
  margin-bottom: var(--sp-2);
}

/* 主题色网格 */
.color-grid {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-3);
}
.color-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-1);
  cursor: pointer;
  padding: var(--sp-2) var(--sp-3);
  border-radius: 12px;
  transition: background var(--motion-base) var(--ease),
              transform var(--motion-base) var(--ease);
  min-width: 64px;
}
.color-item:hover {
  background: var(--bg-soft-hover);
  transform: translateY(-2px);
}
.color-item.is-active {
  background: var(--status-info-bg);
}
.color-item__swatch {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  flex-shrink: 0;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border-color);
  box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.15);
}
.color-item.is-active .color-item__swatch {
  outline: 2px solid var(--accent-primary);
  outline-offset: 2px;
}
.color-item__swatch .bi {
  color: #fff;
  font-size: var(--fs-lg);
  font-weight: var(--fw-bold);
  filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.5));
}
.color-item__picker {
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: #fff;
  font-size: var(--fs-lg);
}
.color-item__input {
  position: absolute;
  inset: 0;
  opacity: 0;
  cursor: pointer;
}

.color-item__input :deep(.el-color-picker__trigger) {
  width: 100%;
  height: 100%;
  border: none;
  padding: 0;
  background: transparent;
}

.color-item__name {
  font-size: var(--fs-xs);
  color: var(--text-muted);
  white-space: nowrap;
}

/* 主题模式选块 */
.mode-tiles {
  display: flex;
  gap: var(--sp-3);
  flex-wrap: wrap;
}

.mode-tile {
  flex: 1;
  min-width: 120px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-4);
  border-radius: 12px;
  border: 1px solid var(--border-color);
  background: var(--bg-soft);
  color: var(--text-secondary);
  cursor: pointer;
  font-size: var(--fs-sm);
  font-family: inherit;
  transition: background var(--motion-base) var(--ease),
              color var(--motion-base) var(--ease),
              border-color var(--motion-base) var(--ease);
}

.mode-tile i {
  font-size: 22px;
}

.mode-tile:hover {
  background: var(--bg-soft-hover);
  color: var(--text-primary);
}

.mode-tile.is-active {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
  font-weight: var(--fw-semibold);
}

/* 字段行 */
.field-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-4);
}

.field-row__title {
  font-size: var(--fs-base);
  font-weight: var(--fw-medium);
  color: var(--text-primary);
}

/* 滑块 */
.speed-control {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  flex: 1;
  max-width: 320px;
  min-width: 180px;
}

.speed-slider {
  flex: 1;
  min-width: 120px;
}

.speed-control__label {
  font-size: var(--fs-base);
  font-weight: var(--fw-semibold);
  color: var(--text-primary);
  min-width: 44px;
  text-align: right;
  font-family: var(--font-mono);
}

/* chip group */
.chip-group {
  flex-wrap: wrap;
}

.chip-icon {
  margin-right: 6px;
}

/* 背景相关 */
.bg-media {
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}
.bg-section {
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}
.bg-empty {
  font-size: var(--fs-sm);
  color: var(--text-muted);
  padding: var(--sp-2) 0;
}
.bg-files {
  display: flex;
  flex-wrap: wrap;
  gap: var(--sp-2);
  max-height: 140px;
  overflow-y: auto;
}
.bg-file {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-3);
  border-radius: 10px;
  font-size: var(--fs-sm);
  color: var(--text-secondary);
  background: var(--bg-soft);
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: background var(--motion-base) var(--ease),
              color var(--motion-base) var(--ease),
              border-color var(--motion-base) var(--ease);
  max-width: 220px;
}
.bg-file:hover {
  background: var(--bg-soft-hover);
  color: var(--text-primary);
}
.bg-file.is-active {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}
.bg-file i {
  font-size: var(--fs-md);
  flex-shrink: 0;
}
.bg-file__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bg-url-row {
  display: flex;
  gap: var(--sp-2);
  align-items: center;
  max-width: 560px;
}
.bg-current {
  font-size: var(--fs-sm);
  color: var(--text-muted);
  word-break: break-all;
}

.bg-solid {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}

.color-hex {
  font-size: var(--fs-base);
  font-family: var(--font-mono);
  color: var(--text-muted);
}

.loading-text {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  color: var(--text-muted);
}
</style>
