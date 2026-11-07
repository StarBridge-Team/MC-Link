import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  GameListResult,
  InstalledPlugin,
  MarketManifest,
  PluginEvent,
  PluginGateway,
  PluginListQuery,
  PluginListResult,
  PluginRuntimeStatus,
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
 * 查询共享 Node 运行时（Bun）的状态。
 *
 * 不发网络请求：界面据此决定要不要显示「安装运行时」以及 Node 插件能否运行。
 */
export async function getPluginRuntime() {
  return invoke<PluginRuntimeStatus>("plugin_runtime_status");
}

/**
 * 安装/修复共享运行时，返回实际安装的版本。
 *
 * 会下载约 90 MB 的产物（带 SHA256 校验与断点续传），因此可能耗时较久；
 * 失败时**必须**把错误展示出来——否则用户只会看到 Node 插件"加载失败"。
 *
 * 平时不需要手动调用：核心在拉起 Node 插件前会自动安装，并在启动时对
 * "已启用 Node 插件 + 运行时缺失"做一次后台预热。
 */
export async function installPluginRuntime() {
  return invoke<string>("plugin_runtime_install");
}

/**
 * 从本地文件安装插件包（`.mclink-plugin`，tar.gz）。
 *
 * `allowUnsigned` 为 `true` 表示用户已确认"我知道它没签名"；否则未签名包会被拒绝。
 * 通过后按未签名安装并**显式启用**（安装本身就是明确的用户动作）。
 */
export async function installPluginPackage(path: string, allowUnsigned = false) {
  return invoke<InstalledPlugin>("plugin_install_file", { path, allowUnsigned });
}

/**
 * 卸载插件。
 *
 * `removeData = false` 时保留 `<data_dir>/PluginData/<id>/`（下次装回来还能用）。
 */
export async function uninstallPlugin(pluginId: string, removeData = false) {
  return invoke<void>("plugin_uninstall", { pluginId, removeData });
}

/**
 * 拉取插件市场清单。
 *
 * 后端 fail-closed：拉不到/解析失败会**报错**而不是返回空清单，
 * 因此界面要把"加载失败"与"市场为空"区分开。
 */
export async function marketList() {
  return invoke<MarketManifest>("market_list");
}

/** 安装市场插件（下载 → 校验 → 解压 → 重扫），返回插件 id。 */
export async function marketInstall(pluginId: string) {
  return invoke<string>("market_install", { pluginId });
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
