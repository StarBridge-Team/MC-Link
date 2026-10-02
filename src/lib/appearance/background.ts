// 背景与背景音乐的 DOM 副作用。
//
// 这里**只做"把状态写进 DOM"**，不碰持久化、不碰 IPC 之外的判断逻辑，
// 因此可以脱离后端单独验证。状态来源是 `useSettings`。
//
// 背景分三类互斥来源（后端字段 `background_type`）：
//   default —— 透明，让窗口材质（Mica/Acrylic/vibrancy）透出来；
//   solid   —— 纯色；
//   image   —— 本地文件或网络 URL，按 `background_fit` 适配；
//   video   —— 同 image，但用 <video> 播放。
//
// 覆盖层的透明度由 `background_overlay` + `background_overlay_opacity` 决定，
// 只在非 default 背景上生效（default 背景下面没有内容可遮）。

import { convertFileSrc } from "@tauri-apps/api/core";
import { getBackgroundFileUrl } from "../api/datadir";
import type { PersonalizationSettings } from "../api/types";

/** `background_fit` → CSS `background-size`。 */
export function fitToCssSize(fit: string): string {
  switch (fit) {
    case "aspect-fit":
      return "contain";
    case "aspect-fill":
      return "cover";
    case "width-fix":
      return "100% auto";
    case "height-fix":
      return "auto 100%";
    default:
      return "100% 100%";
  }
}

/** 是否是一个可以直接交给浏览器的 http(s) 地址。 */
export function isRemoteUrl(value: string): boolean {
  return /^https?:\/\//i.test(value.trim());
}

/** 常见视频扩展名 / 视频站点，用于在只填了 URL 时推断类型。 */
const VIDEO_URL_HINT = /\.(mp4|webm|ogg|avi|mov|mkv|flv)(\?|#|$)/i;

export function looksLikeVideoUrl(url: string): boolean {
  return VIDEO_URL_HINT.test(url);
}

/** 把 `background_value` / `music_value`（文件名或 URL）解析成可加载的地址。 */
export async function resolveMediaUrl(value: string): Promise<string> {
  if (!value) return "";
  if (isRemoteUrl(value)) return value;
  try {
    return convertFileSrc(await getBackgroundFileUrl(value));
  } catch {
    return "";
  }
}

function videoEl(): HTMLVideoElement | null {
  return document.getElementById("bg-video") as HTMLVideoElement | null;
}

function audioEl(): HTMLAudioElement | null {
  return document.getElementById("bg-music") as HTMLAudioElement | null;
}

/**
 * 应用背景与背景音乐。
 *
 * `resolvedUrl` / `resolvedMusicUrl` 是本轮已经解析好的媒体地址（本地文件需先经
 * IPC 取绝对路径），由调用方（`useSettings`）负责解析，避免这里重复请求。
 */
export function applyBackground(
  s: PersonalizationSettings,
  resolvedUrl: string,
  resolvedMusicUrl: string,
): void {
  const root = document.documentElement;
  const style = root.style;

  style.removeProperty("--app-bg");
  style.removeProperty("--app-bg-image");
  style.removeProperty("--app-bg-size");
  style.removeProperty("--app-scrim");

  const video = videoEl();

  // 窗口材质是否真的开启（none / 空 / transparent 都视为未开启）。
  const effectActive =
    !!s.transparent_effect &&
    s.transparent_effect !== "none" &&
    s.transparent_effect !== "transparent";

  // 页面实体背景：
  // - default 且开启了窗口材质 → 保持透明，让 Mica/Acrylic 透出来；
  // - 其余（default 无材质 / solid / image / video）→ 都以 surface 打底，
  //   避免"无窗口材质"时整窗透出桌面；solid 会被下方颜色覆盖，
  //   image/video 在图片/视频未铺满处露出 surface。
  if (!(s.background_type === "default" && effectActive)) {
    style.setProperty("--app-bg", "var(--surface)");
  }

  switch (s.background_type) {
    case "solid":
      style.setProperty("--app-bg", s.background_value || "#000000");
      break;
    case "image":
    case "video": {
      if (resolvedUrl) {
        // 视频走 <video> 元素（CSS 背景不支持视频），所以这里只处理图片。
        if (s.background_type === "image") {
          style.setProperty("--app-bg-image", `url("${resolvedUrl}")`);
          style.setProperty("--app-bg-size", fitToCssSize(s.background_fit));
        }
      }
      break;
    }
    // default 不再单独处理：实体背景已在上方决定。
  }

  if (video) {
    const wanted =
      s.background_type === "video" && resolvedUrl ? resolvedUrl : "";
    if (wanted) {
      if (video.getAttribute("data-src") !== wanted) {
        video.setAttribute("data-src", wanted);
        video.src = wanted;
        void video.play().catch(() => undefined);
      }
      video.style.display = "block";
    } else {
      video.pause();
      video.removeAttribute("data-src");
      video.removeAttribute("src");
      video.style.display = "none";
    }
  }

  if (s.background_overlay && s.background_type !== "default") {
    style.setProperty(
      "--app-scrim",
      `rgba(0, 0, 0, ${clampPercent(s.background_overlay_opacity) / 100})`,
    );
  }

  applyMusic(s, resolvedMusicUrl);
}

/**
 * 应用背景音乐。
 *
 * `music_mode === "video"` 表示"用背景视频自带的声音"，此时视频不静音、
 * 音频元素停用；其余情况视频一律静音，避免两条音轨打架。
 */
function applyMusic(s: PersonalizationSettings, resolvedMusicUrl: string): void {
  const video = videoEl();
  const audio = audioEl();
  const useVideoAudio = s.music_mode === "video" && s.background_type === "video";

  if (video) video.muted = !useVideoAudio;
  if (!audio) return;

  const source =
    s.music_mode === "video" || s.music_mode === "none" ? "" : resolvedMusicUrl;

  if (source) {
    if (audio.getAttribute("data-src") !== source) {
      audio.setAttribute("data-src", source);
      audio.src = source;
      void audio.play().catch(() => undefined);
    }
    audio.loop = true;
    audio.style.display = "";
  } else {
    audio.pause();
    audio.removeAttribute("data-src");
    audio.removeAttribute("src");
    audio.style.display = "none";
  }
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, value));
}
