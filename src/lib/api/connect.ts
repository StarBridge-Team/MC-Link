import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ConnectAdapter, ConnectEvent, ConnectStatus } from "./types";

/**
 * 联机页的 IPC 封装（后端 `src-tauri/src/plugin/connect.rs`）。
 *
 * 字段形态由适配器自己声明（`join_fields`），因此这里的 `fields` 是
 * `Record<string, string>`——键来自适配器，前端不预设房间码/密码之类的形态。
 */

/** 列出可用于联机的适配器（按当前游戏路由），含各自声明的 join 字段。 */
export async function listConnectAdapters(gameId?: string) {
  return invoke<{ adapters: ConnectAdapter[] }>("connect_adapters", { gameId });
}

/** 以房主身份创建房间。 */
export async function startConnectHost(
  adapterId: string,
  gameId: string | undefined,
  fields: Record<string, string>,
  playerName: string,
) {
  return invoke<string>("connect_start_host", { adapterId, gameId, fields, playerName });
}

/** 以访客身份加入房间。 */
export async function joinConnect(
  adapterId: string,
  gameId: string | undefined,
  fields: Record<string, string>,
  playerName: string,
) {
  return invoke<string>("connect_join", { adapterId, gameId, fields, playerName });
}

/** 查询当前连接状态（适配器自报）。 */
export async function getConnectStatus(adapterId: string, gameId: string | undefined) {
  return invoke<ConnectStatus>("connect_status", { adapterId, gameId });
}

/** 断开当前房间 / 主机。 */
export async function stopConnect(adapterId: string, gameId: string | undefined) {
  return invoke<void>("connect_stop", { adapterId, gameId });
}

/** 订阅 `connect-event`（进度 / 连接 / 错误 / 断开）。 */
export async function onConnectEvent(cb: (e: ConnectEvent) => void): Promise<UnlistenFn> {
  return listen<ConnectEvent>("connect-event", (e) => cb(e.payload));
}
