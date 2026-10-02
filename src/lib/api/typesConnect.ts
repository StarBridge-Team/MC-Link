// 联机页（前端 ConnectView）与后端 `plugin/connect.rs` 的契约镜像。
//
// 字段名必须与 `src-tauri/src/plugin/connect.rs` 的 serde 输出逐字一致。

/** 适配器自声明的一个加入字段（由 `adapter.init` 返回）。 */
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
}

/** 可用于联机的适配器（按当前游戏路由得出）。 */
export interface ConnectAdapter {
  pluginId: string;
  name: string;
  /** 信任级别：official | verified | unsigned | blocked */
  trust: string;
  ready: boolean;
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
