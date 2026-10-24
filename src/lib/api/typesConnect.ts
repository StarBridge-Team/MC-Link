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

/**
 * 核心编排扫描后回传首页的发现结果。
 *
 * 与 {@link LocalGame} **不是同一个东西**：那个是 `detector.scan` 的**插件返回值**
 * （带 `id`/`port`），这个是核心加工后的**前端展示结构**（带扫描器与推荐适配器名）。
 * 权威定义见 `src-tauri/src/plugin/protocol.rs::LocalGameFound`。
 *
 * 此前首页在 `HomeView.vue` 里本地声明了一份同形接口，字段与这里不同（三处定义互不一致），
 * 类型系统因此形同虚设——统一收口到此处。
 */
export interface LocalGameFound {
  /** 游戏进程可执行文件名。 */
  process: string;
  /** 游戏展示名。 */
  game_name: string;
  /** 命中的扫描器（detector）插件展示名。 */
  scanner: string;
  /** 推荐的适配器（adapter）插件展示名；无则空串。 */
  adapter: string;
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

// `ConnectEvent` / `ConnectStatus` / `RoomMember` 已随多房间重构移除：
// 会话状态改由 `src/ipc/contracts/room.ts` 的 `RoomSnapshot` 承载
// （`connect-event` 事件与 `connect_status` 命令都不再存在）。
