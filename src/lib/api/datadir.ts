import { invoke } from "@tauri-apps/api/core";
import type { BackgroundFile } from "./types";

/** 获取背景文件列表（包含 data_dir/Background 与旧 exe_dir/Background） */
export async function getBackgroundFiles() {
  return invoke<BackgroundFile[]>("get_background_files");
}

/** 获取背景文件本地绝对路径 */
export async function getBackgroundFileUrl(filename: string) {
  return invoke<string>("get_background_file_url", { filename });
}

/**
 * 把一个网络背景地址交给后端：**下载到本地缓存**，并顺便提取主题种子色。
 *
 * # 为什么必须由后端做
 *
 * 若把 URL 直接交给浏览器，后端拿不到字节就**无法取色**；而随机图片 API 每次请求
 * 返回的图都不同，会出现"取色用 A 图、显示用 B 图"，配色与背景对不上。
 * 由后端先落成文件，取色与显示读的是**同一个文件**。
 *
 * 返回的 `path` 是本地绝对路径（前端再用 `convertFileSrc` 转成可加载 URL）；
 * `seed` 为 null 表示下载成功但取色失败（视频、或不支持解码的格式）——
 * 调用方回退到手选主题色即可，不该让整个操作失败。
 */
export interface RemoteBackground {
  path: string;
  seed: string | null;
  /** 本次下载失败、用的是上一次的缓存。 */
  from_cache: boolean;
}

export async function fetchRemoteBackground(url: string, isVideo: boolean) {
  return invoke<RemoteBackground>("fetch_remote_background", { url, isVideo });
}
