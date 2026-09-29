import { invoke } from "@tauri-apps/api/core";
import type { InitAppData, PrepareAppData, IpInfo } from "./types";

/** 获取应用版本 */
export async function getAppVersion() {
  return invoke<string>("get_app_version");
}

/** 获取 Tauri 版本 */
export async function getTauriVersion() {
  return invoke<string>("get_tauri_version");
}

/** 初始化应用（轻量） */
export async function initApp() {
  return invoke<InitAppData>("init_app");
}

/** 准备应用：首次启动从资产服务器下载字体/图标到本地 */
export async function prepareApp() {
  return invoke<PrepareAppData>("prepare_app");
}

/** 获取 IP 信息 */
export async function getIpInfo(host: string) {
  return invoke<IpInfo>("get_ip_info", { host });
}

/** 获取资产服务器地址 */
export async function getAssetsServerUrl() {
  return invoke<string>("get_assets_server_url");
}

/** 获取本地 Assets 资源路径（前端再用 convertFileSrc 转为 webview URL） */
export async function getAssetUrl(path: string) {
  return invoke<string>("get_asset_url", { path });
}
