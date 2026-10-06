import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConnectAdapter,
  ConnectEvent,
  ConnectStatus,
  LocalGame,
  LocalGameFound,
} from "./types";

/**
 * 联机页的 IPC 封装（后端 `src-tauri/src/plugin/connect.rs`）。
 *
 * 字段形态由适配器自己声明（`host_fields` / `join_fields`），因此这里的 `fields`
 * 是普通对象——键来自适配器，前端不预设房间码/网络名/密码之类的形态。
 */

/** 列出可用于联机的适配器（按当前游戏路由），含各自声明的 host/join 字段。 */
export async function listConnectAdapters(gameId?: string) {
  return invoke<{ adapters: ConnectAdapter[] }>("connect_adapters", { gameId });
}

/** 扫描本机游戏实例（检测器插件），返回发现到的游戏数组（可能为空）。 */
export async function scanLocalGames(gameId?: string) {
  return invoke<LocalGame[]>("connect_scan", { gameId });
}

/**
 * 取最近一次扫描结果，**不重新扫描**。
 *
 * 供首页在挂载时兜底：扫描结果是一次性广播（`local-game-found`），没有补发，
 * 监听器晚一步就再也拿不到，界面会一直停在"正在寻找本地游戏…"。
 *
 * 返回的是核心加工后的展示结构（`LocalGameFound`），**不是** `detector.scan` 的
 * 原始插件返回值（`LocalGame`）——后端 `connect_local_games` 直接回传
 * `PluginManager::local_games()`。
 */
export async function listLocalGames() {
  return invoke<LocalGameFound[]>("connect_local_games");
}

/** 以房主身份创建房间。 */
export async function startConnectHost(
  adapterId: string,
  gameId: string | undefined,
  fields: Record<string, unknown>,
  playerName: string,
) {
  return invoke<string>("connect_start_host", { adapterId, gameId, fields, playerName });
}

/** 以访客身份加入房间。 */
export async function joinConnect(
  adapterId: string,
  gameId: string | undefined,
  fields: Record<string, unknown>,
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
