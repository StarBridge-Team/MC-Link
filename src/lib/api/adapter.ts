import { invoke } from "@tauri-apps/api/core";
import type { AdapterStatus } from "./types";

/** 下载适配器 */
export async function downloadAdapter() {
  return invoke<string>("download_adapter");
}

/** 启动时初始化适配器 */
export async function adapterStartupInit() {
  return invoke<string>("adapter_startup_init");
}

/** 获取适配器状态 */
export async function getAdapterStatus() {
  return invoke<AdapterStatus>("get_adapter_status");
}

/** 获取陶瓦联机状态 */
export async function getTerracottaState() {
  return invoke<Record<string, unknown>>("get_terracotta_state");
}

/** 启动陶瓦联机（房主） */
export async function startTerracottaHost(params: {
  roomCode: string;
  playerName: string;
}) {
  return invoke<Record<string, unknown>>("start_terracotta_host", params);
}

/** 加入陶瓦联机（访客） */
export async function startTerracottaGuest(params: {
  roomCode: string;
  playerName: string;
}) {
  return invoke<Record<string, unknown>>("start_terracotta_guest", params);
}
