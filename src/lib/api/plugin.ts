import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  GameListResult,
  PluginEvent,
  PluginGateway,
  PluginListQuery,
  PluginListResult,
  RoutePlan,
} from "./types";

/**
 * 插件系统的 IPC 封装（后端 `src-tauri/src/plugin/commands.rs`）。
 *
 * # 未知取值绝不让加载失败
 *
 * `platforms` / `methods` / `tags` 里的未知取值由后端丢弃并留痕（返回 `warnings`），
 * 因此界面可以放心地把用户输入直接传下去。
 *
 * # 筛选结果的计数口径
 *
 * `facets` 里每个维度的计数以**其它**维度筛选结果为分母，避免用户组合出空结果。
 */

/** 列出插件（支持多维筛选），返回清单 + 各维度可选值与计数。 */
export async function listPlugins(query: PluginListQuery = {}) {
  return invoke<PluginListResult>("plugin_list", {
    query: query.query,
    kinds: query.kinds,
    methods: query.methods,
    platforms: query.platforms,
    tags: query.tags,
    gameId: query.gameId,
    enabledOnly: query.enabledOnly,
  });
}

/** 回环网关状态（谁在跑、端口、协议版本）。 */
export async function getPluginGateway() {
  return invoke<PluginGateway>("plugin_gateway");
}

/** 列出游戏（`methods` 由插件声明派生，同一事实只写一处）。 */
export async function listGames(query: string | undefined, methods?: string[]) {
  return invoke<GameListResult>("game_list", { query, methods });
}

/** 为某个能力（adapter / detector / coupler）生成路由计划。 */
export async function pluginRoutePlan(kind: string, gameId?: string) {
  return invoke<RoutePlan>("plugin_route_plan", { kind, gameId });
}

/** 启用 / 停用插件。 */
export async function setPluginEnabled(pluginId: string, enabled: boolean) {
  return invoke<void>("plugin_set_enabled", { pluginId, enabled });
}

/**
 * 设置插件权限。`granted` 传 `undefined` 表示恢复为该插件的默认（声明）权限。
 */
export async function setPluginGrants(pluginId: string, granted?: string[]) {
  return invoke<void>("plugin_set_grants", { pluginId, granted });
}

/** 拉黑 / 解除拉黑插件（拉黑即拒绝加载）。 */
export async function setPluginBlocked(pluginId: string, blocked: boolean) {
  return invoke<void>("plugin_set_blocked", { pluginId, blocked });
}

/** 重新扫描插件目录，返回本次的告警列表。 */
export async function reloadPlugins() {
  return invoke<string[]>("plugin_reload");
}

/**
 * 订阅 `plugin-event`（外部插件连接/断开/自定义事件），返回取消订阅函数。
 *
 * 事件只用于"提示 + 刷新状态"，不承载业务逻辑：界面任何时刻都可以丢弃它，
 * 重新拉一次 `plugin_list` 就能得到一致的状态。
 */
export async function onPluginEvent(
  cb: (event: PluginEvent) => void,
): Promise<UnlistenFn> {
  return listen<PluginEvent>("plugin-event", (e) => cb(e.payload));
}
