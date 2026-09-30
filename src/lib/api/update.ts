import { invoke } from "@tauri-apps/api/core";
import type {
  CheckUpdateResult,
  DownloadUpdateResult,
  InstallUpdateResult,
  RuntimeInfo,
  UpdateAsset,
} from "./types";

/**
 * 检查是否有新版本。
 *
 * 返回值同时带上当前安装形态（`portable` / `installed`）、构建渠道与平台，
 * 界面不需要再单独问一次就能决定按钮文案。
 *
 * 注意 `build_channel` 与 `update_allowed`：开发构建会直接返回"无更新"
 * （连网络都不会发），自行构建的版本只会给出新版本信息与手动下载链接。
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
 *
 * 只有 `update_allowed === true` 且 `latest.asset` 非空时才应调用：
 * 开发构建与自行构建的版本会被后端拒绝（这是防呆，不是错误用法）。
 */
export async function installUpdate(asset: UpdateAsset) {
  return invoke<InstallUpdateResult>("install_update_command", { asset });
}

/** 清理更新缓存（安装包与落地日志）。 */
export async function clearUpdateCache() {
  return invoke<void>("clear_update_cache_command");
}

/**
 * 查询当前运行环境（安装形态 + 构建渠道 + 自更新许可），不联网。
 *
 * "关于"页首次渲染即可调用，用它显示"便携版 / 安装版"与"官方版本 / 自行构建"。
 */
export async function getRuntimeInfo() {
  return invoke<RuntimeInfo>("get_runtime_info_command");
}
