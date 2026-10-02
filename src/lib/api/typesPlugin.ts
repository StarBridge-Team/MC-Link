// 后端契约的类型镜像（插件系统 + 社区数据）。
//
// 详见 `typesCore.ts` 顶部的说明。

// ===== 插件系统（后端 plugin/**） =====

export type PluginKind = "adapter" | "detector" | "coupler";
export type PluginSource = "builtin" | "external";
export type PluginTrust = "official" | "verified" | "unsigned" | "blocked";

/** 权限稳定名（后端 plugin/permission.rs，共 14 项）。 */
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
  /** 已授予的权限（稳定名数组）。 */
  permissions: string[];
  /** 因信任等级上限被拒的权限。 */
  deniedByCeiling: string[];
  declaredPermissions: string[];
  permissionDescriptions: PermissionDescription[];
  priority: number;
  fallback: string[];
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
  port: number;
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
}

/** `plugin-event` 事件的三种负载形状。 */
export type PluginEvent =
  | { type: "connected"; pluginId: string }
  | { type: "disconnected"; pluginId: string; reason: string }
  | { type: "event"; pluginId: string; topic: string; data: unknown };

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
