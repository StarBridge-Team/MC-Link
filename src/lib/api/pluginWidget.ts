/**
 * 挂件（widget 类插件）的 IPC 封装（后端 `plugin/commands_widget.rs`）。
 *
 * # 为什么是"声明式视图"而不是插件交 HTML
 *
 * 挂件跑在主窗口里。插件如果能交任意 HTML/JS，就等于拿到了 DOM 与会话，
 * 能绕过全部权限模型。因此后端只接受白名单节点树（`WidgetNode`），
 * 前端只用 Vue 渲染这几种类型，用户的点击以 `action` 回传。
 */
import { invoke } from "@tauri-apps/api/core";

import type { WidgetView } from "./types";

/**
 * 挂载一个挂件，返回视图。
 *
 * 失败语义：插件未连接是 `unavailable`，插件返回的视图非法是 `bad_request`——
 * 界面据此区分"插件没起来"与"插件写错了"。
 */
export async function mountWidget(pluginId: string, widgetId: string) {
  return invoke<WidgetView>("widget_mount", { pluginId, widgetId });
}

/**
 * 把用户在挂件上的动作回传，返回新视图；`null` 表示插件决定不刷新。
 */
export async function sendWidgetAction(
  pluginId: string,
  widgetId: string,
  action: string,
  payload?: unknown,
) {
  return invoke<WidgetView | null>("widget_action", {
    pluginId,
    widgetId,
    action,
    payload: payload ?? null,
  });
}

/** 卸载挂件（插件侧的清理失败不影响卸载）。 */
export async function unmountWidget(pluginId: string, widgetId: string) {
  return invoke<void>("widget_unmount", { pluginId, widgetId });
}
