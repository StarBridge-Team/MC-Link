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

/**
 * 读取 Assets 目录内某个文本资源的内容。
 *
 * 用于 CSS：前端需要拿到原文把其中的相对 `url()` 重写成绝对地址，
 * 因为 asset 协议下相对路径会被解析到 asset 根而 404（详见 Rust 侧注释）。
 */
export async function readAssetText(path: string) {
  return invoke<string>("read_asset_text", { path });
}

/** 获取本地 Assets 资源路径（前端再用 convertFileSrc 转为 webview URL） */
export async function getAssetUrl(path: string) {
  return invoke<string>("get_asset_url", { path });
}

/**
 * 重新执行「应用打开时的动作」（目前只有「扫描局域网内已开启的游戏」）。
 *
 * 结果不经返回值，而是通过 `local-game-status` / `local-game-found` 事件推送——
 * 与启动时那次扫描走同一条路径，所以调用方不必自己拼扫描结果。
 */
export async function runOpenActions() {
  return invoke<void>("run_open_actions");
}
