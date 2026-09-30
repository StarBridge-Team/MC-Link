import { invoke } from "@tauri-apps/api/core";
import type {
  CheckUpdateResult,
  DownloadUpdateResult,
  InstallModeInfo,
  InstallUpdateResult,
  UpdateAsset,
} from "./types";

/**
 * 检查是否有新版本。
 *
 * 返回值同时带上当前安装形态（`portable` / `installed`）与平台，
 * 界面不需要再单独问一次就能决定按钮文案。
 */
export async function checkUpdate() {
  return invoke<CheckUpdateResult>("check_update_command");
}

/**
 * 下载更新包到本地缓存（已缓存且校验通过时不会重复下载）。
 *
 * 下载进度通过 `update-progress` 事件推送，见 `useUpdater`。
 */
export async function downloadUpdate(asset: UpdateAsset) {
  return invoke<DownloadUpdateResult>("download_update_command", { asset });
}

/**
 * 安装更新并重启应用。
 *
 * - 便携版：把已下载的 exe 覆盖到当前程序路径
 * - 安装版：运行 NSIS 安装器，装完重新拉起应用
 *
 * 调用成功后应用会在约 0.6 秒内退出，因此**先展示提示再调用**。
 * 传 `null`（没有可用更新包）时不应调用本函数，应引导用户走 `manual_url`。
 */
export async function installUpdate(asset: UpdateAsset) {
  return invoke<InstallUpdateResult>("install_update_command", { asset });
}

/** 清理更新缓存（安装包与落地日志）。 */
export async function clearUpdateCache() {
  return invoke<void>("clear_update_cache_command");
}

/** 查询安装形态与自更新能力（用于"关于"页显示便携版/安装版与程序路径）。 */
export async function getInstallMode() {
  return invoke<InstallModeInfo>("get_install_mode_command");
}
