/**
 * 小组件窗口专用的命令。
 *
 * 只放**只属于小组件**的能力：显示主窗口 / 退出应用 / 隐藏自身都走
 * `lib/api/window.ts`，不在这里再包一层——同一个命令两个出口，改签名时必漏一个。
 */
import { call } from "../../ipc/client";

/** 显示主窗口，并请求它进入「联机」页；`process` 用于预选要联机的游戏。 */
export function appShowConnect(process?: string) {
  return call<void>("app_show_connect", { process });
}
