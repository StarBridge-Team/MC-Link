/**
 * 联机的**只读**能力（后端 `src-tauri/src/plugin/connect.rs`）。
 *
 * 建房 / 加入 / 断开已迁到 `src/lib/api/room.ts`：那些是会话级操作，
 * 需要 `sessionId` 与状态机。这里只剩无状态的扫描与缓存查询。
 */
import { invoke } from "@tauri-apps/api/core";
import type { LocalGame, LocalGameFound } from "./types";

/** 扫描本机游戏实例（检测器插件），返回发现到的游戏数组（可能为空）。 */
export async function scanLocalGames(gameId?: string) {
  return invoke<LocalGame[]>("connect_scan", { gameId });
}

/**
 * 取最近一次扫描结果，**不重新扫描**。
 *
 * 事件（`local-game-found`）是一次性广播、没有补发，首页挂载时用它兜底，
 * 否则监听器晚一步就会永远停在"正在寻找本地游戏…"。
 */
export async function listLocalGames() {
  return invoke<LocalGameFound[]>("connect_local_games");
}
