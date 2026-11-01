/**
 * 房间状态（每窗口一份投影）。
 *
 * # 权威在哪
 *
 * 权威在 Rust：本 store 只是把快照与事件收敛成可渲染的状态。
 * 因此**没有任何本地"事实"**——`sessions` 全部由 `snapshot` / 事件写入，
 * 窗口重载后 `refresh()`（**全量对账**）就能恢复，不依赖内存里残留的东西。
 *
 * # 为什么 apply 要判 rev
 *
 * 同一个变更会有两条到达路径：命令的返回值、随后广播的事件（`emit` 会发给所有
 * 窗口，包括发起方）。两条都应用会重复；`rev` 单调，判一下即可幂等。
 *
 * # 终态不可回退
 *
 * `closed` 是终态：后端一旦把会话判死，迟到的快照（更小的 `rev`，或更活跃的
 * `phase`）都不许把它拽回"在线"。`refresh()` 还会把后端**已经回收**的会话裁掉，
 * 否则主窗口会留下永远关不掉的"幽灵房间"。
 */
import { computed, ref } from "vue";
import { defineStore } from "pinia";

import {
  focusRoomWindow,
  openRoomWindow,
  roomJoin,
  roomLeave,
  roomList,
  roomRefresh,
  roomSnapshot,
  roomStartHost,
} from "../lib/api/room";
import type { RoomProgressEvent, RoomSnapshot } from "../ipc/contracts/room";

export interface RoomProgress {
  stage: string;
  message: string;
  seq: number;
}

export const useRoomsStore = defineStore("rooms", () => {
  const sessions = ref<Record<string, RoomSnapshot>>({});
  const progress = ref<Record<string, RoomProgress>>({});
  /** 已被后端回收的会话：迟到的快照不许再把它们复活。 */
  const retired = ref<Record<string, boolean>>({});

  /** 全部会话（含已结束），按创建时间升序。 */
  const list = computed(() =>
    Object.values(sessions.value).sort((a, b) => a.createdAt - b.createdAt),
  );

  /** 进行中的会话。主窗口的房间列表用这个。 */
  const active = computed(() => list.value.filter((s) => s.phase !== "closed"));

  /** 应用一个快照。返回是否真的更新了（`rev` 未前进时返回 false）。 */
  function apply(snapshot: RoomSnapshot): boolean {
    // 后端已经回收的会话：任何迟到快照都不得复活它。
    if (retired.value[snapshot.sessionId]) return false;
    const current = sessions.value[snapshot.sessionId];
    if (current && snapshot.rev <= current.rev) return false;
    // 终态不可回退：已断开的会话不接受"更活跃"的快照。
    if (current && current.phase === "closed" && snapshot.phase !== "closed") return false;
    sessions.value = { ...sessions.value, [snapshot.sessionId]: snapshot };
    return true;
  }

  /** 应用进度。`seq` 断档或过期一律丢弃。 */
  function applyProgress(event: RoomProgressEvent): void {
    const current = progress.value[event.sessionId];
    if (current && event.seq <= current.seq) return;
    progress.value = {
      ...progress.value,
      [event.sessionId]: { stage: event.stage, message: event.message, seq: event.seq },
    };
  }

  function get(sessionId: string): RoomSnapshot | null {
    return sessions.value[sessionId] ?? null;
  }

  function progressOf(sessionId: string): RoomProgress | null {
    return progress.value[sessionId] ?? null;
  }

  /**
   * 从后端拉一次全量，并**全量对账**。
   *
   * 后端 `room_list` 只返回进行中的会话：服务端已经回收的会话必须在这里被裁掉，
   * 否则 store 里会留下"幽灵房间"（`phase` 停在旧值，点「聚焦」还会去开一个
   * 根本不存在的房间窗口）。
   */
  async function refresh(): Promise<RoomSnapshot[]> {
    // 先记下 await 之前的会话集合：await 期间新建的会话不算"已消失"，不能误删。
    const before = new Set(Object.keys(sessions.value));
    const snapshots = await roomList();
    const live = new Set(snapshots.map((s) => s.sessionId));
    for (const snapshot of snapshots) apply(snapshot);

    const nextSessions = { ...sessions.value };
    const nextProgress = { ...progress.value };
    const nextRetired = { ...retired.value };
    let pruned = false;
    for (const id of before) {
      if (live.has(id)) continue;
      const current = nextSessions[id];
      if (!current) continue;
      // 标记为已结束（后端已回收，迟到快照不许复活它），并从列表与进度里删掉。
      nextRetired[id] = true;
      delete nextSessions[id];
      delete nextProgress[id];
      pruned = true;
    }
    if (pruned) {
      sessions.value = nextSessions;
      progress.value = nextProgress;
      retired.value = nextRetired;
    }
    return snapshots;
  }

  /** 拉单个会话（房间窗口的权威基线）。 */
  async function pull(sessionId: string): Promise<RoomSnapshot> {
    const snapshot = await roomSnapshot(sessionId);
    apply(snapshot);
    return snapshot;
  }

  /** 向适配器要一次实时状态（成员会陆续加入，房间窗口低频调用）。 */
  async function sync(sessionId: string): Promise<RoomSnapshot> {
    const snapshot = await roomRefresh(sessionId);
    apply(snapshot);
    return snapshot;
  }

  async function openWindow(sessionId: string, roomCode: string): Promise<void> {
    try {
      await openRoomWindow(sessionId, roomCode);
    } catch (e) {
      console.warn("[room] 打开房间窗口失败", e);
    }
  }

  async function startHost(
    adapterId: string,
    gameId: string | undefined,
    fields: Record<string, unknown>,
    playerName: string,
  ): Promise<RoomSnapshot> {
    const snapshot = await roomStartHost(adapterId, gameId, fields, playerName);
    apply(snapshot);
    await openWindow(snapshot.sessionId, snapshot.roomCode);
    return snapshot;
  }

  async function join(
    adapterId: string,
    gameId: string | undefined,
    fields: Record<string, unknown>,
    playerName: string,
  ): Promise<RoomSnapshot> {
    const snapshot = await roomJoin(adapterId, gameId, fields, playerName);
    apply(snapshot);
    await openWindow(snapshot.sessionId, snapshot.roomCode);
    return snapshot;
  }

  async function leave(sessionId: string): Promise<RoomSnapshot> {
    const snapshot = await roomLeave(sessionId);
    apply(snapshot);
    return snapshot;
  }

  /** 聚焦房间窗口；窗口已被关掉就重新开一个。 */
  async function focus(sessionId: string): Promise<void> {
    try {
      await focusRoomWindow(sessionId);
    } catch {
      const snapshot = get(sessionId);
      if (snapshot) await openWindow(snapshot.sessionId, snapshot.roomCode);
    }
  }

  return {
    sessions,
    progress,
    list,
    active,
    apply,
    applyProgress,
    get,
    progressOf,
    refresh,
    pull,
    sync,
    openWindow,
    startHost,
    join,
    leave,
    focus,
  };
});
