/**
 * 房间状态（每窗口一份投影）。
 *
 * # 权威在哪
 *
 * 权威在 Rust：本 store 只是把快照与事件收敛成可渲染的状态。
 * 因此**没有任何本地"事实"**——`sessions` 全部由 `snapshot` / 事件写入，
 * 窗口重载后 `refresh()` 就能恢复，不依赖内存里残留的东西。
 *
 * # 为什么 apply 要判 rev
 *
 * 同一个变更会有两条到达路径：命令的返回值、随后广播的事件（`emit` 会发给所有
 * 窗口，包括发起方）。两条都应用会重复；`rev` 单调，判一下即可幂等。
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

  /** 全部会话（含已结束），按创建时间升序。 */
  const list = computed(() =>
    Object.values(sessions.value).sort((a, b) => a.createdAt - b.createdAt),
  );

  /** 进行中的会话。主窗口的房间列表用这个。 */
  const active = computed(() => list.value.filter((s) => s.phase !== "closed"));

  /** 应用一个快照。返回是否真的更新了（`rev` 未前进时返回 false）。 */
  function apply(snapshot: RoomSnapshot): boolean {
    const current = sessions.value[snapshot.sessionId];
    if (current && snapshot.rev <= current.rev) return false;
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

  /** 从后端拉一次全量。窗口挂载、重载、聚焦纠偏都走它。 */
  async function refresh(): Promise<RoomSnapshot[]> {
    const snapshots = await roomList();
    for (const snapshot of snapshots) apply(snapshot);
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
