// MC Link 插件 SDK 的类型声明。
//
// 与 `src/index.js` 一一对应；改实现时同步改这里，否则作者拿到的类型会撒谎。

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

/** 核心下发的上下文（`core.context.update` 的载荷）。 */
export interface PluginContext {
  /** `host` / `guest` / `idle`。 */
  role: string;
  game: { id?: string; name?: string; port?: number; transport?: string; [k: string]: unknown } | null;
  room: { code?: string; host?: string; capacity?: number; password_protected?: boolean } | null;
  members?: Array<{ name: string; kind: string; machine_id: string; is_self: boolean }>;
  peers: PeerInfo[];
}

/** `core.peer.list` 的元素 / 上下文里的 `peers`。 */
export interface PeerInfo {
  plugin_id: string;
  kind: string;
  games: string[];
  endpoints: string[];
  version?: string;
  alive?: boolean;
}

/** 处理器拿到的上下文：绑定本次调用。 */
export interface HandlerContext {
  readonly pluginId: string;
  /** 本次调用来自哪枚插件（顶层调用为 `null`）。 */
  readonly from: string | null;
  /** 本次调用所处的 peer 嵌套深度；继续转发时 SDK 会自动带回去。 */
  readonly depth: number;
  /** 插件可写的数据目录（**不要**写插件安装目录，那会破坏整包验签）。 */
  readonly dataDir: string | null;
  readonly context: PluginContext;
  emit(topic: string, data?: unknown): Promise<void>;
  log(level: "debug" | "info" | "warn" | "error", message: string, extra?: object): Promise<void>;
  peerList(): Promise<PeerInfo[]>;
  peerCall(to: string, method: string, params?: unknown, options?: PeerCallOptions): Promise<unknown>;
  peerPublish(topic: string, data?: unknown, to?: string): Promise<unknown>;
}

export interface PeerCallOptions {
  /** 目标未连接时是否请求核心拉起（最长等 20 秒）。 */
  launch?: boolean;
  /** 超时，默认 10000，核心夹取 100–30000。 */
  timeoutMs?: number;
  /** 一般不要手填：SDK 会用当前调用的深度。 */
  depth?: number;
}

/** 带错误码的插件侧异常。 */
export declare class PluginError extends Error {
  constructor(code: string, message: string);
  readonly code: string;
}

export interface ConnectOptions {
  /** 核心写下的会话文件路径。 */
  sessionPath: string;
  /** 测试时替换 WebSocket 实现。 */
  WebSocketImpl?: typeof WebSocket;
}

export declare class Plugin {
  /** 按 `--mclink-session <file>` 建立连接。 */
  static connectFromArgs(argv?: string[]): Promise<Plugin>;
  /** 直接按会话文件建立连接。 */
  static connect(options: ConnectOptions): Promise<Plugin>;

  readonly pluginId: string;
  /** 插件可写的数据目录；核心未提供时为 `null`。 */
  readonly dataDir: string | null;
  readonly context: PluginContext;
  readonly closed: boolean;

  /** 注册能力方法（`adapter.init` / `detector.scan` …）。 */
  handle<T = unknown, R = unknown>(
    method: string,
    handler: (params: T, ctx: HandlerContext) => R | Promise<R>,
  ): this;

  /** 注册插件间方法（对端 `core.peer.call { method }` 时命中）。 */
  onPeer<T = unknown, R = unknown>(
    method: string,
    handler: (params: T, ctx: HandlerContext) => R | Promise<R>,
  ): this;

  onContext(handler: (context: PluginContext) => void): this;
  onEvent(topic: string, handler: (data: unknown) => void): this;
  onPeerEvent(handler: (data: { from: string; topic: string; data: unknown }, ctx: HandlerContext) => void): this;
  onShutdown(handler: (reason: string) => void): this;

  emit(topic: string, data?: unknown): Promise<void>;
  log(level: "debug" | "info" | "warn" | "error", message: string, extra?: object): Promise<void>;
  peerList(): Promise<PeerInfo[]>;
  peerCall(to: string, method: string, params?: unknown, options?: PeerCallOptions): Promise<unknown>;
  peerPublish(topic: string, data?: unknown, to?: string): Promise<unknown>;

  /** 处理入站消息直到断开。 */
  run(): Promise<void>;
  /** 主动断开。 */
  close(reason?: string): void;
}

export declare const PROTOCOL_VERSION: 1;
export declare const PEER_CALL: "peer.call";
export declare const PEER_EVENT: "peer.event";
export declare const ErrorCode: {
  readonly BAD_REQUEST: "bad_request";
  readonly UNAUTHORIZED: "unauthorized";
  readonly FORBIDDEN: "forbidden";
  readonly NOT_SUPPORTED: "not_supported";
  readonly TIMEOUT: "timeout";
  readonly RATE_LIMITED: "rate_limited";
  readonly INTERNAL: "internal";
  readonly UNAVAILABLE: "unavailable";
};
export declare const CoreMethod: Record<string, string>;
export declare const EventTopic: Record<string, string>;

export declare function toHex(bytes: Uint8Array): string;
export declare function fromHex(text: string): Uint8Array;
export declare function toBase64(bytes: Uint8Array): string;
export declare function fromBase64(text: string): Uint8Array;
