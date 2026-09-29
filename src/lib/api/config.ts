import { invoke } from "@tauri-apps/api/core";
import type { PersonalizationSettings } from "./types";

/** 获取指定设置文件内容 */
export async function getSetting(section: string) {
  return invoke<string>("get_setting", { section });
}

/** 保存设置文件内容 */
export async function saveSetting(section: string, content: string) {
  return invoke<void>("save_setting", { section, content });
}

/** 获取个性化设置 */
export async function getPersonalization() {
  return invoke<PersonalizationSettings>("get_personalization");
}

/** 保存个性化设置 */
export async function savePersonalization(settings: PersonalizationSettings) {
  return invoke<void>("save_personalization", { settings });
}
