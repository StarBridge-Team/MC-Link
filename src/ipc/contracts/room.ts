/**
 * 房间契约：与 `src-tauri/src/domain/room/model.rs` 的 serde 输出逐字对齐。
 *
 * 这里**不重复** `typesConnect.ts` 里的适配器/字段类型，只引用它们——
 * 同形类型定义两份是旧审计里的 D10，重写时刻意合并。
 */
import type { ConnectAdapter, JoinField, LocalGameFound } from "../../lib/api/types";

/** 会话阶段。与 Rust `Phase` 的 kebab-case 序列化一致。 */
export type RoomPhase = "starting" | "linking" | "online" | "stopping" | "closed";

export type RoomRole = "host" | "guest";

export interface RoomMember {
  name: string;
  kind: string;
  machineId: string;
  isSelf: boolean;
}

/** 会话全量快照。`rev` 单调递增，用于去重与判序。 */
export interface RoomSnapshot {
  sessionId: string;
  rev: number;
  role: RoomRole;
  adapterId: string;
  gameId: string | null;
  phase: RoomPhase;
  roomCode: string;
  members: RoomMember[];
  port: number | null;
  message: string;
  createdAt: number;
  updatedAt: number;
}

/** `mclink://room/state` 的负载。 */
export interface RoomStateEvent {
  sessionId: string;
  rev: number;
  snapshot: RoomSnapshot;
}

/** `mclink://room/progress` 的负载。`seq` 与 `rev` 独立，进度不推全量快照。 */
export interface RoomProgressEvent {
  sessionId: string;
  seq: number;
  stage: string;
  message: string;
  role: RoomRole;
}

/** `room_prepare` 的返回。 */
export interface RoomPrepare {
  adapters: ConnectAdapter[];
  /** 最近一次扫描的缓存结果（无端口）；带端口的实时扫描仍走 `connect_scan`。 */
  games: LocalGameFound[];
}

export type { ConnectAdapter, JoinField, LocalGameFound };
