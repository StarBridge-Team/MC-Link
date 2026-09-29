import { invoke } from "@tauri-apps/api/core";
import type { AccountResponse, DesktopInitResult, DesktopPollResult, MeResult } from "./types";

/** 桌面登录：初始化 */
export async function desktopLoginInit(appId: string) {
  return invoke<DesktopInitResult>("desktop_login_init", { appId });
}

/** 桌面登录：轮询 */
export async function desktopLoginPoll(appId: string, session: string) {
  return invoke<DesktopPollResult>("desktop_login_poll", { appId, session });
}

/** 获取当前用户信息 */
export async function accountGetMe(token: string) {
  return invoke<MeResult>("account_get_me", { token });
}

/** 获取头像（后端代理，避免跨域） */
export async function accountGetAvatar(token: string, avatarPath: string) {
  return invoke<string>("account_get_avatar", { token, avatarPath });
}

/** 验证 Token */
export async function accountVerify(token: string) {
  return invoke<AccountResponse>("account_verify", { token });
}

/** Ping 账号服务器 */
export async function accountPing() {
  return invoke<boolean>("account_ping");
}
