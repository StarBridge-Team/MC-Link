import { invoke } from "@tauri-apps/api/core";
import type { CheckUpdateResult, DownloadUpdateResult, UpdateInfo } from "./types";

/** 检查是否有新版本 */
export async function checkUpdate() {
  return invoke<CheckUpdateResult>("check_update_command");
}

/** 下载指定版本更新包 */
export async function downloadUpdate(info: UpdateInfo) {
  return invoke<DownloadUpdateResult>("download_update_command", { info });
}

/** 清理更新本地缓存 */
export async function clearUpdateCache() {
  return invoke<void>("clear_update_cache_command");
}
