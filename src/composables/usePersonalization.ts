// 个性化设置共享逻辑 - 消除 App.vue 和 SettingPage.vue 之间的重复
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";

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

export function applyPersStyle(s: Personalization, resolvedBgUrl?: string) {
  const root = document.documentElement;
  // 主题色
  root.style.setProperty('--accent-primary', s.theme_color);
  root.style.setProperty('--accent-hover', s.theme_color + '33');
  root.style.setProperty('--shadow-accent', s.theme_color + '40');
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
  // 背景
  const isFullBg = s.background_type !== 'default' && s.background_type !== 'transparent';
  if (isFullBg) {
    root.style.setProperty('--window-margin', '0');
    root.style.setProperty('--window-radius', '0');
  } else {
    root.style.removeProperty('--window-margin');
    root.style.removeProperty('--window-radius');
  }
  switch (s.background_type) {
    case 'default':
      root.style.removeProperty('--window-container-bg');
      root.style.removeProperty('--titlebar-bg');
      root.style.removeProperty('--window-wrapper-bg');
      root.style.removeProperty('--window-wrapper-bg-image');
      root.style.removeProperty('--window-wrapper-bg-fit');
      break;
    case 'transparent':
      root.style.setProperty('--window-container-bg', 'transparent');
      root.style.setProperty('--titlebar-bg', 'transparent');
      root.style.removeProperty('--window-wrapper-bg');
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
  if (s.transparent_effect === 'none' && s.background_type === 'default') {
    root.style.setProperty('--window-wrapper-bg', 'var(--bg-window)');
    root.style.setProperty('--titlebar-bg', 'var(--bg-window)');
  } else if (s.transparent_effect === 'none') {
    root.style.removeProperty('--window-wrapper-bg');
    root.style.setProperty('--titlebar-bg', 'var(--bg-window)');
  } else {
    root.style.removeProperty('--window-wrapper-bg');
    root.style.removeProperty('--titlebar-bg');
    if (s.transparent_effect === 'transparent') {
      root.style.setProperty('--transparent-overlay', 'rgba(0,0,0,0.4)');
    } else {
      root.style.removeProperty('--transparent-overlay');
    }
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
    invoke<string>("get_background_file_url", { filename: s.music_value })
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
    const filePath = await invoke<string>("get_background_file_url", { filename });
    return convertFileSrc(filePath);
  } catch {
    return "";
  }
}

export async function loadPersSettings(): Promise<Personalization> {
  const data = await invoke<Personalization>("get_personalization");
  // 兼容旧配置
  if (!data.background_fit) data.background_fit = "scale-to-fill";
  if (!data.theme_mode) data.theme_mode = "system";
  if (data.background_overlay === undefined) data.background_overlay = false;
  if (!data.background_overlay_opacity) data.background_overlay_opacity = 30;
  if (!data.music_mode) data.music_mode = "none";
  if (!data.music_value) data.music_value = "";
  if (!data.homepage_mode) data.homepage_mode = "default";
  if (!data.homepage_value) data.homepage_value = "";
  return data;
}

export async function savePersSettings(settings: Personalization): Promise<void> {
  await invoke("save_personalization", { settings });
}