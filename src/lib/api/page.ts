import { invoke } from "@tauri-apps/api/core";
import type { PageManifest } from "./types";

/** 获取远程页面清单（带本地缓存） */
export async function getPageManifest() {
  return invoke<PageManifest>("get_page_manifest");
}

/** 获取单个页面 HTML 内容（带本地缓存） */
export async function getPageContent(name: string) {
  return invoke<string>("get_page_content", { name });
}

/** 清理页面本地缓存 */
export async function clearPageCache() {
  return invoke<void>("clear_page_cache_command");
}
