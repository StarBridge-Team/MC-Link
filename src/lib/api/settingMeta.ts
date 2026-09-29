import { invoke } from "@tauri-apps/api/core";
import type { SettingMeta, SettingManifest } from "./types";

/** 拉取指定分区的元配置（带本地缓存） */
export function getSettingMeta(section: string) {
  return invoke<SettingMeta>("get_setting_meta", { section });
}

/** 拉取设置项清单（带本地缓存） */
export function getSettingManifest() {
  return invoke<SettingManifest>("get_setting_manifest");
}

/** 清理元配置缓存 */
export function clearSettingMetaCache() {
  return invoke<void>("clear_setting_meta_cache_command");
}
