// 后端契约的类型镜像（插件系统 + 社区数据）。
//
// 详见 `typesCore.ts` 顶部的说明。

// ===== 插件系统（后端 plugin/**） =====

/**
 * 插件种类（B7 扩到 6 类）。
 *
 * 前三是联机链路核心；`bridge` 是纯协议兼容的桥接器（不携带引擎），
 * `widget` 承载「窗口内挂件 / 窗口外页面」两种形态，`utility` 是不参与链路的小工具。
 */
export type PluginKind =
  | "adapter"
  | "detector"
  | "coupler"
  | "bridge"
  | "widget"
  | "utility";
export type PluginSource = "builtin" | "external";
export type PluginTrust = "official" | "verified" | "unsigned" | "blocked";

/** 权限稳定名（后端 plugin/permission.rs，共 15 项）。 */
export type PluginPermission =
  | "net_listen_local"
  | "net_listen_public"
  | "net_connect_local"
  | "net_connect_any"
  | "net_udp"
  | "net_nat_mapping"
  | "fs_read_game_dirs"
  | "fs_plugin_data"
  | "proc_spawn_self"
  | "proc_spawn_game"
  | "proc_inspect"
  | "registry_read"
  | "game_scan"
  /** 与本机其它插件互相收发消息（双向闸门：双方都要声明）。 */
  | "peer_message"
  | "ui_notify";

export interface PermissionDescription {
  name: string;
  description: string;
  highRisk: boolean;
}

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  kind: PluginKind | string;
  author: string | null;
  description: string | null;
  games: string[];
  platforms: string[];
  methods: string[];
  tags: string[];
  source: PluginSource | string;
  trust: PluginTrust | string;
  enabled: boolean;
  runnable: boolean;
  /** 是否已连接（内置=true；外部插件要握手成功）。 */
  alive: boolean;
  /** 会话建立时间（毫秒时间戳）；未连接或内置插件为 null。 */
  connectedAt: number | null;
  /** 运行时轨道：`builtin` / `process` / `node`。 */
  runtime: string;
  /** 已授予的权限（稳定名数组）。 */
  permissions: string[];
  /** 因信任等级上限被拒的权限。 */
  deniedByCeiling: string[];
  declaredPermissions: string[];
  permissionDescriptions: PermissionDescription[];
  priority: number;
  fallback: string[];
}

// ===== 插件市场（后端 plugin/market.rs） =====

/** 市场中一个插件的条目（与后端 `MarketEntry` 同形）。 */
export interface MarketEntry {
  id: string;
  name: string;
  version: string;
  kind: string;
  author: string;
  description: string;
  platforms: string[];
  urls: string[];
  /** 必填：缺失时后端拒绝安装。 */
  sha256: string;
  size: number | null;
}

/** 市场清单（资源服务器 `plugins/market.json`）。 */
export interface MarketManifest {
  manifest_version: number;
  plugins: MarketEntry[];
}

/** 各维度的可选值与计数（分母是**其它**维度的筛选结果）。 */
export interface PluginFacets {
  kinds: Record<string, number>;
  methods: Record<string, number>;
  platforms: Record<string, number>;
  tags: Record<string, number>;
  games: Record<string, number>;
}

export interface PluginListResult {
  plugins: PluginInfo[];
  facets: PluginFacets;
  currentPlatform: string;
  total: number;
  matched: number;
  warnings: string[];
}

export interface PluginListQuery {
  query?: string;
  kinds?: string[];
  methods?: string[];
  platforms?: string[];
  tags?: string[];
  gameId?: string;
  enabledOnly?: boolean;
}

export interface GameInfo {
  id: string;
  name: string;
  aliases: string[];
  /** 游戏监听端口（后端 `Option<u16>`，没有默认端口时为 null）。 */
  port: number | null;
  transport: string;
  traits: string[];
  processNames: string[];
  requiresCoupler: boolean;
  preferredAdapter: string | null;
  fallbackAdapters: string[];
  methods: string[];
}

export interface GameListResult {
  games: GameInfo[];
  facets: { methods: Record<string, number> };
  total: number;
  matched: number;
}

/** 回环网关状态。 */
export interface PluginGateway {
  running: boolean;
  port: number | null;
  endpoint?: string;
  externalPlugins?: number;
  loopbackOnly?: boolean;
  protocolVersion?: number;
  subprotocol?: string;
  /** 插件间通信（peer 信道）是否可用。 */
  peerApi?: boolean;
  /** peer 调用的最大嵌套深度。 */
  peerMaxDepth?: number;
  /** 单次 peer 调用的默认超时（毫秒）。 */
  peerTimeoutMs?: number;
}

/** `plugin-event` 事件负载。 */
export type PluginEvent =
  | { type: "connected"; pluginId: string }
  | { type: "disconnected"; pluginId: string; reason: string }
  | { type: "event"; pluginId: string; topic: string; data: unknown }
  | {
      /** 共享运行时的后台安装进度（只在真有 Node 插件时出现）。 */
      type: "runtime";
      state: "downloading" | "ready" | "failed";
      version: string | null;
      message: string | null;
    };

// ===== 插件包安装（后端 plugin/install.rs） =====

/** `plugin_install_file` 的返回值。 */
export interface InstalledPlugin {
  id: string;
  name: string;
  version: string;
  /** `verified` / `unsigned`。 */
  trust: string;
  /** 非致命提示：未签名、覆盖安装等。 */
  warnings: string[];
  /** 解压后的实际体积（字节）。 */
  bytes: number;
}

// ===== 挂件（后端 plugin/widget.rs） =====

/** 允许的节点类型（白名单；未知类型由后端丢弃）。 */
export type WidgetNodeType =
  | "text"
  | "heading"
  | "badge"
  | "button"
  | "row"
  | "column"
  | "divider"
  | "progress"
  | "spacer";

/** 声明式节点：插件只能描述"显示什么"，不能注入 HTML/脚本。 */
export interface WidgetNode {
  type: WidgetNodeType;
  id?: string;
  text?: string;
  label?: string;
  value?: string;
  placeholder?: string;
  /** `primary` / `danger` 之类的语义色调。 */
  tone?: string;
  /** 按钮点击时回传给插件的动作名。 */
  action?: string;
  disabled?: boolean;
  /** 0–100。 */
  percent?: number;
  children?: WidgetNode[];
}

/** `widget_mount` 的返回。 */
export interface WidgetView {
  title: string;
  subtitle: string;
  nodes: WidgetNode[];
}

// ===== 共享运行时（后端 plugin/runtime.rs） =====

/**
 * Node 插件共用的 Bun 运行时状态。
 *
 * `source` 为 `dev-override` 时说明当前用的是开发期覆盖（`MC_LINK_BUN`），
 * 不代表用户机器上真的装好了运行时。
 */
export interface PluginRuntimeStatus {
  /** 固定 `node`。 */
  kind: string;
  /** 是否已就绪（能直接拉起 Node 插件）。 */
  installed: boolean;
  /** 实际可用版本；未安装为 `null`。 */
  version: string | null;
  /** 可执行文件路径；未安装为 `null`。 */
  path: string | null;
  /** 核心锁定的版本（用户不应自己升级运行时）。 */
  pinnedVersion: string;
  /** `installed` / `dev-override` / `missing`。 */
  source: "installed" | "dev-override" | "missing" | string;
}

// ===== 社区数据（后端 community.rs） =====

export interface CommunityContributor {
  login: string;
  avatar_url: string;
  html_url: string;
  contributions: number;
}

export interface CommunityLabel {
  name: string;
  color: string;
}

export interface CommunityIssueUser {
  login: string;
  avatar_url: string;
  html_url: string;
}

export interface CommunityIssue {
  number: number;
  title: string;
  state: string;
  html_url: string;
  created_at: string;
  updated_at: string;
  comments: number;
  labels: CommunityLabel[];
  user: CommunityIssueUser | null;
}

export interface Community {
  repo: string;
  repo_url: string;
  contributors: CommunityContributor[];
  issues: CommunityIssue[];
  /** 是否返回了过期缓存（离线回退）。 */
  stale: boolean;
  error: string | null;
}
