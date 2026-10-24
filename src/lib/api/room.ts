/**
 * 房间相关命令（后端 `src-tauri/src/commands/room.rs`）。
 *
 * 命令按**用户意图**切分，不按资源切分：一次调用完成一件用户能感知的事，
 * 前端不需要把多个原子命令拼成一次操作。
 */
import { call } from "../../ipc/client";
import type { RoomPrepare, RoomSnapshot } from "../../ipc/contracts/room";

/** 联机页初始化：适配器（含字段声明）+ 最近一次扫描缓存。 */
export function roomPrepare(gameId?: string) {
  return call<RoomPrepare>("room_prepare", { gameId });
}

/** 建房。立即返回首个快照（`phase: starting`），后续进度走事件。 */
export function roomStartHost(
  adapterId: string,
  gameId: string | undefined,
  fields: Record<string, unknown>,
  playerName: string,
) {
  return call<RoomSnapshot>("room_start_host", { adapterId, gameId, fields, playerName });
}

/** 加入房间。 */
export function roomJoin(
  adapterId: string,
  gameId: string | undefined,
  fields: Record<string, unknown>,
  playerName: string,
) {
  return call<RoomSnapshot>("room_join", { adapterId, gameId, fields, playerName });
}

/** 断开并结束会话，返回终态快照。 */
export function roomLeave(sessionId: string) {
  return call<RoomSnapshot>("room_leave", { sessionId });
}

/** 拉单个会话的权威快照（窗口挂载时用它做基线）。 */
export function roomSnapshot(sessionId: string) {
  return call<RoomSnapshot>("room_snapshot", { sessionId });
}

/** 向适配器要一次实时状态并合并。只在真的变化时才会推事件。 */
export function roomRefresh(sessionId: string) {
  return call<RoomSnapshot>("room_refresh", { sessionId });
}

/** 拉所有进行中的会话。 */
export function roomList() {
  return call<RoomSnapshot[]>("room_list");
}

/** 打开（或唤出）房间窗口，返回窗口 label。 */
export function openRoomWindow(sessionId: string, roomCode: string) {
  return call<string>("room_window_open", { sessionId, roomCode });
}

/** 聚焦已存在的房间窗口。 */
export function focusRoomWindow(sessionId: string) {
  return call<void>("room_window_focus", { sessionId });
}

/** 只关窗口，不断链路。 */
export function closeRoomWindow(sessionId: string) {
  return call<void>("room_window_close", { sessionId });
}
