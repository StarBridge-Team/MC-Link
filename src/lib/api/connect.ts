import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** P2P 连接阶段（与后端 ConnectionStage 对应） */
export type P2PStage = "idle" | "connecting" | "connected" | "disconnected";

/** 后端事件 stage 字段 */
export type P2PEventStage = "progress" | "connected" | "error" | "stopped";

/** 后端推送的 p2p-event payload */
export interface P2PEvent {
  stage: P2PEventStage;
  message: string;
  natType?: string | null;
  peerAddr?: string | null;
  successLayer?: string | null;
  elapsedMs?: number | null;
  error?: string | null;
  status: P2PStatus;
}

/** 后端 ConnectionStatus 快照 */
export interface P2PStatus {
  stage: P2PStage;
  mode?: string | null;
  code?: string | null;
  startedAtMs?: number | null;
  peerAddr?: string | null;
  localNat?: string | null;
  peerNat?: string | null;
  successLayer?: string | null;
  elapsedMs?: number | null;
  lastError?: string | null;
}

/** 启动参数（camelCase 与后端 serde rename_all 对应） */
export interface StartP2PArgs {
  /** "create" | "join" */
  mode: "create" | "join";
  /** 邀请码 / 房间码 */
  code: string;
  /** 信令服务器地址（host[:port]），留空使用默认 */
  signalingAddr?: string;
  /** 打洞 STUN 地址（host[:port]），留空使用默认 */
  stunAddr?: string;
  /** NAT 检测 STUN 地址列表，留空使用默认 */
  natStunServers?: string[];
  /** 应用类型，默认 "GameTcp" */
  appType?: string;
  /** 本地监听端口（预留桥接，本期可不传） */
  listenPort?: number;
}


/** 启动 P2P 连接（后台任务，立即返回任务 id） */
export async function startP2PConnection(args: StartP2PArgs): Promise<string> {
  return invoke<string>("start_p2p_connection", { args });
}

/** 停止当前 P2P 连接 */
export async function stopP2PConnection(): Promise<void> {
  return invoke<void>("stop_p2p_connection");
}

/** 查询当前连接状态 */
export async function getP2PStatus(): Promise<P2PStatus> {
  return invoke<P2PStatus>("get_p2p_status");
}

/**
 * 订阅 p2p-event，返回取消订阅函数。
 * 回调在收到事件时同步触发。
 */
export async function onP2PEvent(
  cb: (evt: P2PEvent) => void
): Promise<UnlistenFn> {
  return await listen<P2PEvent>("p2p-event", (e) => {
    cb(e.payload);
  });
}
