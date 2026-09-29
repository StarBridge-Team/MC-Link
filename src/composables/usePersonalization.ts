// 个性化设置共享逻辑 - 消除 App.vue 和 SettingPage.vue 之间的重复
import { convertFileSrc } from "@tauri-apps/api/core";
import { getPersonalization, savePersonalization, getBackgroundFileUrl } from '../lib/api/settings';
import { setWindowDarkMode } from '../lib/api/effect';

export interface Personalization {
  theme_color: string;
  theme_mode: string;
  animation_enabled: boolean;
  animation_speed: number;
  transparent_effect: string;
  background_type: string;
  background_value: string;
  background_fit: string;
  background_overlay: boolean;
  background_overlay_opacity: number;
  music_mode: string;
  music_value: string;
  homepage_mode: string;
  homepage_value: string;
}

export function getDefaultPers(): Personalization {
  return {
    theme_color: "#0066cc",
    theme_mode: "system",
    animation_enabled: true,
    animation_speed: 1.0,
    transparent_effect: "none",
    background_type: "default",
    background_value: "",
    background_fit: "scale-to-fill",
    background_overlay: false,
    background_overlay_opacity: 30,
    music_mode: "none",
    music_value: "",
    homepage_mode: "default",
    homepage_value: "",
  };
}

// 记录最近一次应用的设置，用于"跟随系统"模式下响应系统主题切换
let lastAppliedPers: Personalization | null = null;
// 记录最近一次同步到窗口（DWM）的暗色状态，避免高频调用后端
let lastWindowDark: boolean | null = null;

function syncWindowDark(isDark: boolean) {
  if (lastWindowDark === isDark) return;
  lastWindowDark = isDark;
  setWindowDarkMode(isDark).catch(() => {});
}

if (typeof window !== "undefined" && window.matchMedia) {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => {
    if (lastAppliedPers?.theme_mode === "system") {
      document.documentElement.classList.toggle("dark", e.matches);
      syncWindowDark(e.matches);
    }
  });
}

export function applyPersStyle(s: Personalization, resolvedBgUrl?: string) {
  const root = document.documentElement;
  lastAppliedPers = { ...s };
  // 主题色：仅设置基础色。其余派生色（hover/阴影/EP 浅色阶 light-3..9、dark-2）
  // 全部由 App.vue 中基于 --accent-primary 的 color-mix 推导。
  // 这样任何合法 CSS 颜色（3位/6位/8位 hex、rgb()、命名色）都能正确生效，
  // 避免此前 JS 对 6 位 hex 做字符串拼接/解析，在非法格式时整组派生色失效的问题。
  root.style.setProperty('--accent-primary', s.theme_color);
  root.style.setProperty('--el-color-primary', s.theme_color);
  // 动画
  if (s.animation_enabled) {
    root.classList.remove('no-animations');
    root.style.setProperty('--anim-speed', String(s.animation_speed));
  } else {
    root.classList.add('no-animations');
    root.style.setProperty('--anim-speed', '1');
  }
  // 主题模式
  root.setAttribute('data-theme-mode', s.theme_mode);
  // 同步 Element Plus 暗色模式 class
  const isDark = s.theme_mode === 'dark'
    || (s.theme_mode === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  root.classList.toggle('dark', isDark);
  // 同步窗口 DWM 暗色模式（Mica/Acrylic 材质明暗跟随应用主题而非系统）
  syncWindowDark(isDark);
  // 背景
  const isFullBg = s.background_type !== 'default';
  // 移除窗口边框，全屏覆盖
  root.style.setProperty('--window-margin', '0');
  root.style.setProperty('--window-radius', '0');
  switch (s.background_type) {
    case 'default':
      root.style.setProperty('--window-wrapper-bg', 'transparent');
      root.style.setProperty('--titlebar-bg', 'transparent');
      root.style.setProperty('--window-container-bg', 'transparent');
      root.style.removeProperty('--window-wrapper-bg-image');
      root.style.removeProperty('--window-wrapper-bg-fit');
      break;
    case 'solid':
      root.style.setProperty('--window-wrapper-bg', s.background_value || '#18191a');
      root.style.setProperty('--window-container-bg', 'transparent');
      root.style.setProperty('--titlebar-bg', 'transparent');
      root.style.removeProperty('--window-wrapper-bg-image');
      root.style.removeProperty('--window-wrapper-bg-fit');
      break;
    case 'image': {
      root.style.setProperty('--window-container-bg', 'transparent');
      root.style.setProperty('--titlebar-bg', 'transparent');
      root.style.removeProperty('--window-wrapper-bg');
      if (s.background_value) {
        const isUrl = s.background_value.startsWith('http://') || s.background_value.startsWith('https://');
        const bgPath = isUrl ? s.background_value : resolvedBgUrl || '';
        if (bgPath) {
          root.style.setProperty('--window-wrapper-bg-image', `url("${bgPath}")`);
          root.style.setProperty('--window-wrapper-bg-fit', getFitCss(s.background_fit));
        } else {
          root.style.removeProperty('--window-wrapper-bg-image');
          root.style.removeProperty('--window-wrapper-bg-fit');
        }
      } else {
        root.style.removeProperty('--window-wrapper-bg-image');
        root.style.removeProperty('--window-wrapper-bg-fit');
      }
      break;
    }
    case 'video':
      root.style.setProperty('--window-container-bg', 'transparent');
      root.style.setProperty('--titlebar-bg', 'transparent');
      root.style.removeProperty('--window-wrapper-bg');
      root.style.removeProperty('--window-wrapper-bg-image');
      root.style.removeProperty('--window-wrapper-bg-fit');
      break;
  }
  // 透明效果
  // 注意：背景（wrapper/titlebar）已在上方 switch 中统一设置为透明顶栏，
  // 这里不再给顶栏单独设置实色背景，避免顶栏出现一条与内容区割裂的色块分割；
  // 也不再清除纯色背景的 --window-wrapper-bg（旧逻辑会导致纯色背景失效）
  if (s.transparent_effect !== 'none') {
    // 系统材质模式：清除自定义窗口背景，让 Mica/Acrylic/vibrancy 透出
    root.style.removeProperty('--window-wrapper-bg');
    root.style.removeProperty('--titlebar-bg');
    if (s.transparent_effect === 'transparent') {
      root.style.setProperty('--transparent-overlay', 'rgba(0,0,0,0.4)');
    } else {
      root.style.removeProperty('--transparent-overlay');
    }
  } else {
    root.style.removeProperty('--transparent-overlay');
  }
  // 背景遮罩
  root.style.removeProperty('--bg-overlay');
  if (s.background_overlay && isFullBg) {
    root.style.setProperty('--bg-overlay', `rgba(0,0,0,${s.background_overlay_opacity / 100})`);
  }
  // 同步背景视频状态
  const vidEl = document.getElementById('bg-video') as HTMLVideoElement | null;
  if (s.background_type === 'video' && s.background_value) {
    const isUrl = s.background_value.startsWith('http://') || s.background_value.startsWith('https://');
    const videoSrc = isUrl ? s.background_value : resolvedBgUrl || '';
    if (vidEl && videoSrc) {
      if (vidEl.getAttribute('data-src') !== videoSrc) {
        vidEl.setAttribute('data-src', videoSrc);
        vidEl.src = videoSrc;
        vidEl.play().catch(() => {});
      }
      vidEl.style.display = 'block';
    }
  } else if (vidEl) {
    vidEl.style.display = 'none';
  }
  // 同步音乐
  syncBgMusic(s);
}

function syncBgMusic(s: Personalization) {
  const audioEl = document.getElementById('bg-music') as HTMLAudioElement | null;
  const vidEl = document.getElementById('bg-video') as HTMLVideoElement | null;
  if (vidEl) vidEl.muted = true;
  if (!audioEl) return;
  if (s.music_mode === 'video' && s.background_type === 'video') {
    if (vidEl) vidEl.muted = false;
    audioEl.pause();
    audioEl.src = '';
    audioEl.style.display = 'none';
    return;
  }
  if (s.music_mode === 'file' && s.music_value) {
    getBackgroundFileUrl(s.music_value)
      .then(path => {
        audioEl.src = convertFileSrc(path);
        audioEl.style.display = '';
        audioEl.loop = true;
        audioEl.play().catch(() => {});
      })
      .catch(() => {});
  } else if (s.music_mode === 'url' && s.music_value) {
    audioEl.src = s.music_value;
    audioEl.style.display = '';
    audioEl.loop = true;
    audioEl.play().catch(() => {});
  } else {
    audioEl.pause();
    audioEl.src = '';
    audioEl.style.display = 'none';
  }
}

export function getFitCss(fit: string): string {
  switch (fit) {
    case 'aspect-fit': return 'contain';
    case 'aspect-fill': return 'cover';
    case 'width-fix': return '100% auto';
    case 'height-fix': return 'auto 100%';
    default: return '100% 100%';
  }
}

export async function resolveBgFile(filename: string): Promise<string> {
  try {
    const filePath = await getBackgroundFileUrl(filename);
    return convertFileSrc(filePath);
  } catch {
    return "";
  }
}

export async function loadPersSettings(): Promise<Personalization> {
  const data = await getPersonalization();
  // 兼容旧配置
  if (!data.background_fit) data.background_fit = "scale-to-fill";
  if (!data.theme_mode) data.theme_mode = "system";
  if (!data.theme_color) data.theme_color = "#0066cc";
  if (data.animation_enabled === undefined || data.animation_enabled === null) data.animation_enabled = true;
  if (!data.animation_speed) data.animation_speed = 1.0;
  if (data.background_overlay === undefined) data.background_overlay = false;
  if (data.background_overlay_opacity === undefined || data.background_overlay_opacity === null) data.background_overlay_opacity = 30;
  if (!data.music_mode) data.music_mode = "none";
  if (!data.music_value) data.music_value = "";
  if (!data.homepage_mode) data.homepage_mode = "default";
  if (!data.homepage_value) data.homepage_value = "";
  return data;
}

export async function savePersSettings(settings: Personalization): Promise<void> {
  await savePersonalization(settings);
}