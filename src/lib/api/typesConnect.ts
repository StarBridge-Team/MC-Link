// 联机页（前端 ConnectView 等）与后端 `plugin/connect.rs` 的契约镜像。
//
// 字段名必须与 `src-tauri/src/plugin/connect.rs` 的 serde 输出逐字一致。

/** 检测器扫到的本机游戏实例（与 `detector.scan` 返回逐字对应）。 */
export interface LocalGame {
  id: string;
  /** 进程 MOTD / 标识。 */
  process: string;
  name: string;
  port: number;
}

/** 适配器自声明的一个字段（由 `adapter.init` 的 `host_fields` / `join_fields` 返回）。 */
export interface JoinField {
  key: string;
  label?: string;
  type: "text" | "password";
  required?: boolean;
  /** 字段校验正则（后端约定，前端仅作输入提示）。 */
  pattern?: string;
  placeholder?: string;
  /** 该字段是否可由 `mclink://join/<code>` 深链自动填入（如房间码）。 */
  autofillFromInvite?: boolean;
  /** 字段相位：`primary`（首步收集）或 `password`（首步提交后再问）。 */
  phase?: "primary" | "password";
  /** 设为 true 表示由适配器生成并回传（房主无需输入）。 */
  generated?: boolean;
  /** 附加说明（如"由适配器自动生成"）。 */
  note?: string;
}

/** 可用于联机的适配器（按当前游戏路由得出）。 */
export interface ConnectAdapter {
  pluginId: string;
  name: string;
  /** 信任级别：official | verified | unsigned | blocked */
  trust: string;
  ready: boolean;
  /** 房主侧需要的字段（generated 的由适配器生成，不收集输入）。 */
  hostFields: JoinField[];
  /** 访客侧需要的字段。 */
  joinFields: JoinField[];
}

/** 后端推送的 `connect-event` 负载。 */
export interface ConnectEvent {
  stage: "progress" | "connected" | "error" | "stopped";
  message: string;
  role?: "host" | "guest";
  room_code?: string;
  nat_type?: string;
  peer_addr?: string;
  elapsed_ms?: number;
}

/** `connect_status` 返回的原始快照（各适配器自报，字段不固定）。 */
export type ConnectStatus = Record<string, unknown>;
