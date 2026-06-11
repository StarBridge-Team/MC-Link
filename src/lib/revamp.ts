// MC Link revamp 适配器 API - 通过 Tauri invoke 调用后端
import { invoke } from "@tauri-apps/api/core";

export const REVAMP_CENTRAL = "103.36.221.57:8000";

export interface RevampNode {
  ip: string;
  port: string;
  name: string;
  last_update: string;
  contributor: string;
}

export interface RevampRoom {
  room_id: string;
  password: string;
  node_ip: string;
  node_port: string;
  creator_name: string;
  players: string[];
}

export interface RevampVersion {
  version: string;
  download_url: string;
  update_info: string;
  release_date: string;
}

/** 检测中央服务器是否在线 */
export async function pingCentral(): Promise<boolean> {
  return invoke<boolean>("revamp_ping");
}

/** 获取节点列表 */
export async function getNodes(): Promise<RevampNode[]> {
  return invoke<RevampNode[]>("revamp_get_nodes");
}

/** 获取房间列表 */
export async function getRooms(): Promise<RevampRoom[]> {
  return invoke<RevampRoom[]>("revamp_get_rooms");
}

/** 检测房间是否存在 */
export async function roomExists(roomId: string): Promise<boolean> {
  return invoke<boolean>("revamp_room_exists", { roomId });
}

/** 创建房间 */
export async function createRoom(params: {
  room_id: string;
  password: string;
  node_ip: string;
  node_port: string;
  creator_name: string;
}): Promise<{ status: string; message: string; room_id: string }> {
  return invoke("revamp_create_room", {
    roomId: params.room_id,
    password: params.password,
    nodeIp: params.node_ip,
    nodePort: params.node_port,
    creatorName: params.creator_name,
  });
}

/** 加入房间 */
export async function joinRoom(params: {
  room_id: string;
  password: string;
  player_name: string;
}): Promise<{ status: string; node_ip: string; node_port: string; room_id: string }> {
  return invoke("revamp_join_room", {
    roomId: params.room_id,
    password: params.password,
    playerName: params.player_name,
  });
}

/** 离开房间 */
export async function leaveRoom(params: {
  room_id: string;
  player_name: string;
}): Promise<{ status: string; message: string }> {
  return invoke("revamp_leave_room", {
    roomId: params.room_id,
    playerName: params.player_name,
  });
}

/** 获取版本信息 */
export async function getVersion(): Promise<RevampVersion> {
  return invoke<RevampVersion>("revamp_get_version");
}

export interface AccountResult {
  status: string;
  message: string;
  token: string;
  username: string;
}

/** 注册账号 */
export async function registerAccount(username: string, password: string): Promise<AccountResult> {
  return invoke<AccountResult>("revamp_register", { username, password });
}

/** 登录 */
export async function loginAccount(username: string, password: string): Promise<AccountResult> {
  return invoke<AccountResult>("revamp_login", { username, password });
}

/** 房主启动 revamp 中继 */
export async function startRevampHost(params: {
  localPort: number;
  nodeIp: string;
  nodePort: string;
  roomId: string;
  password: string;
  playerName: string;
}): Promise<string> {
  return invoke<string>("revamp_start_host", params);
}

/** 访客加入 revamp 房间 */
export async function joinRevampRoom(params: {
  roomId: string;
  password: string;
  playerName: string;
}): Promise<{ status: string; node_ip: string; node_port: string; room_id: string }> {
  return invoke("revamp_join_room_cmd", {
    roomId: params.roomId,
    password: params.password,
    playerName: params.playerName,
  });
}
