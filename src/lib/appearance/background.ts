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
// 覆盖层的透明度由 `background_overlay` + `background_overlay_opacity`
// （深色另有 `_dark` 档）决定，只在非 default 背景上生效。
//
// 另有两组"分深浅两档"的调节，见 `blurLayer` / `applyBackground`：
//   - 模糊：图片、视频、种子色（材质）各一对，只作用于当前背景的载体；
//   - 页面不透明度：只对 default 与 solid 有意义（媒体背景的载体是媒体本身）。
//
// `dark` 由调用方传入而不是在这里自己判断：它同时决定模糊与遮罩用哪一档，
// 散成多个判断点迟早会漂移。

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

/**
 * 转义要放进 CSS `url("...")` 的地址。
 *
 * 地址可能是**用户填的远程 URL**：里面的 `"` 或 `)` 会提前闭合 `url()`，
 * 破坏整条声明（背景消失，甚至影响后续 CSS 变量）。这里按 CSS 字符串字面量规则
 * 转义反斜杠、双引号与换行，并把 `)` 一并转义（同 `CSS.escape` 的思路）。
 */
export function cssUrlEscape(url: string): string {
  return url
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\)/g, "\\)")
    .replace(/[\r\n]/g, "");
}

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

/**
 * 按当前明暗取出该类背景的模糊强度（px）。
 *
 * 模糊是逐类目 + 分深浅的：图片、视频、种子色（材质）各有一对浅色/深色值。
 * 深色档单独存的原因是深色背景本身更暗，同样的模糊半径观感差别很大，
 * 强行共用一个值会逼用户在自己常用的那一种下做妥协。
 *
 * 越界与非法值在这里收敛，而不是指望滑块不越界——配置文件的来源不止滑块。
 */
export function blurFor(
  s: PersonalizationSettings,
  kind: "image" | "video" | "seed",
  dark: boolean,
): number {
  const key = `${kind === "seed" ? "seed" : `background_${kind}`}_blur_${
    dark ? "dark" : "light"
  }` as keyof PersonalizationSettings;
  return clampBlur(s[key] as number);
}

function clampBlur(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(BLUR_MAX, Math.max(0, value));
}

/** 模糊强度上限（px）。再高就只剩一片色块，滑块的调节区间到此为止。 */
export const BLUR_MAX = 40;

/**
 * 判断当前背景的**载体**（决定用哪一类模糊、以及透明度作用于哪一层）。
 *
 * `solid` 没有载体：「纯色」本身就是最终画面，糊不糊都是同一块颜色，
 * 所以不给模糊（返回 null），但透明度仍然对它有意义。
 */
export type BackgroundCarrier = "image" | "video" | "material" | "solid" | "none";

export function carrierOf(s: PersonalizationSettings): BackgroundCarrier {
  switch (s.background_type) {
    case "image":
      return s.background_value ? "image" : "none";
    case "video":
      return s.background_value ? "video" : "none";
    case "solid":
      return "solid";
    case "default":
    default:
      // default 这一层不管有没有窗口材质都存在，只是底下垫的东西不同：
      //   有材质（Mica/Acrylic）→ 底下是系统材质；
      //   选「无」            → 底下什么都没有，看到的是桌面。
      // 两种情况都由同一个「背景不透明度」决定它有多实，所以归为同一载体。
      return "material";
  }
}

/**
 * 当前的模糊强度（px）与它作用在哪一层。
 *
 * 返回 `null` 表示"本次不该有模糊"（无载体、强度为 0、或该层根本不支持）。
 */
export function blurLayer(
  s: PersonalizationSettings,
  dark: boolean,
): { target: "media" | "material"; px: number } | null {
  const carrier = carrierOf(s);
  if (carrier === "image" || carrier === "video") {
    const px = blurFor(s, carrier, dark);
    return px > 0 ? { target: "media", px } : null;
  }
  if (carrier === "material") {
    // 「种子色背景」= default 背景这一层。有材质时它透出的是系统材质，所以模糊落在
    // `.shell` 自己身上（backdrop-filter 只能作用于元素自身，且其背后不能有不透明父级）；
    // 无材质时 `.shell` 上就是我们自建的配色背景层，同样的 `filter` 语义也成立。
    const px = blurFor(s, "seed", dark);
    return px > 0 ? { target: "material", px } : null;
  }
  return null;
}

/** 背景不透明度（0–100）→ 0–1 的 alpha。 */
function opacityAlpha(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.min(100, Math.max(0, value)) / 100;
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
 *
 * `dark` 由调用方传入（不要再自己判断一遍）：模糊强度与遮罩的深浅两档都靠它选择。
 */
export function applyBackground(
  s: PersonalizationSettings,
  resolvedUrl: string,
  resolvedMusicUrl: string,
  dark: boolean,
): void {
  const root = document.documentElement;
  const style = root.style;

  style.removeProperty("--app-bg");
  style.removeProperty("--app-bg-image");
  style.removeProperty("--app-bg-size");
  style.removeProperty("--app-scrim");
  style.removeProperty("--app-opacity");
  style.removeProperty("--app-media-blur");
  style.removeProperty("--app-material-blur");

  const video = videoEl();
  const carrier = carrierOf(s);

  // 页面实体背景：**只有 solid 才写死 `--app-bg`**，其余一律留空让它回落。
  //
  // 这里刻意什么都不写的关键原因（踩过）：`.shell` 的背景是
  //   color-mix(in srgb, var(--app-bg, var(--surface)) <alpha>, transparent)
  // 而 `transparent` 混任何比例仍然是 `transparent` —— **alpha 会被完全吃掉**。
  // 早先选 Mica 时这里写 `--app-bg: transparent`，于是"背景不透明度"拖动毫无反应；
  // 只有让它们回落到 `--surface`（一个真实的颜色）alpha 才有东西可调。
  //
  //   solid        → 用户选的颜色（alpha 调它自己的透明度）；
  //   default      → 回落 `--surface`：有材质时它就是"盖在材质上的那层雾"，
  //                  不透明度 100 完全遮住材质、0 完全露出；
  //   image/video  → 也回落 `--surface`，但它被 `.shell__bg`/`.shell__video` 盖住，
  //                  而本项对这两类不生效（见下方 range），所以看不到它的影响。
  if (s.background_type === "solid") {
    style.setProperty("--app-bg", s.background_value || "#000000");
  }

  if (s.background_type === "image" && resolvedUrl) {
    // 转义后再拼进 `url("...")`：`resolvedUrl` 对 `background_type === "image"` 时
    // 可能是**用户填的远程地址**，里面的 `"` 或 `)` 会直接破坏 CSS 值
    // （轻则背景不显示，重则把后续声明一起吞掉）。
    style.setProperty("--app-bg-image", `url("${cssUrlEscape(resolvedUrl)}")`);
    style.setProperty("--app-bg-size", fitToCssSize(s.background_fit));
  }
  // 视频走 <video> 元素（CSS 背景不支持视频），故这里只处理图片。

  // 「页面背景不透明度」：**这就是"看得见多少窗口材质"的那个旋钮**。
  //
  // 为什么必须由它来管、而不是去调材质：材质本身（尤其 Mica）不可调 ——
  // `window_vibrancy::apply_mica` 只设 `DWMWA_SYSTEMBACKDROP_TYPE`，没有任何浓度参数。
  // 想控制它露多少，唯一可行的就是压在上面这层页面背景的 alpha：
  //   100 → 完全遮住（看不见材质）；0 → 完全不遮（材质全显现）；50 → 半显现。
  //
  // 这里**只写 alpha**，颜色交给 CSS 的 `color-mix(in srgb, var(--app-bg, var(--surface)) α, transparent)`
  // 去配 —— 因为 `--surface` 是由种子色动态算出来的 40+ 个角色之一，
  // 在 JS 里硬编码 RGB 只对默认种子色成立，用户换个主题色就配错色了。
  //
  // 作用范围：
  //   default → **本项的主要用途**：盖在窗口材质上的那层雾，100 完全遮住、0 完全露出。
  //            前提是上面没有把 `--app-bg` 写成 `transparent`（见上方注释里的坑）；
  //   solid   → 让**用户选的那个颜色**变透明；
  //   image / video → 不参与：媒体下方还有一层 surface，调它的 alpha 等于"给媒体加一层雾"
  //     而不是变透，那两类要的是模糊。
  if (carrier === "solid" || carrier === "material") {
    const opacity = dark ? s.background_opacity_dark : s.background_opacity;
    style.setProperty("--app-opacity", String(opacityAlpha(opacity)));
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

  // 模糊：图片/视频层糊媒体层（`.shell__bg` / `.shell__video`），
  // 种子色背景糊材质层（`.shell`，见 `blurLayer` 的说明）。
  const blur = blurLayer(s, dark);
  if (blur) {
    style.setProperty(
      blur.target === "media" ? "--app-media-blur" : "--app-material-blur",
      `${blur.px}px`,
    );
  }

  if (s.background_overlay && s.background_type !== "default") {
    // 遮罩分深浅两档：深色背景本来更暗，同一个数值会更"糊"，通常要更低。
    const overlayOpacity = dark
      ? s.background_overlay_opacity_dark
      : s.background_overlay_opacity;
    style.setProperty(
      "--app-scrim",
      `rgba(0, 0, 0, ${clampPercent(overlayOpacity) / 100})`,
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
