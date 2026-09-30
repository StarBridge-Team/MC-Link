import { invoke } from "@tauri-apps/api/core";
import type { SetupState } from "./types";

/**
 * 首次启动引导（OOBE）与本地化设置。
 *
 * 可选语言/地区清单以后端返回为准（`src-tauri/src/setup.rs`），界面不要硬编码。
 */

/** 读取引导状态：是否已完成 + 生效/系统检测到的语言与地区 + 可选值清单。 */
export async function getSetupState() {
  return invoke<SetupState>("get_setup_state_command");
}

/**
 * 完成首次引导：保存语言与地区并标记为已完成。
 *
 * 传入不支持的语言/地区会报错，且**不会**写入半个选择。
 */
export async function completeSetup(language: string, region: string) {
  return invoke<SetupState>("complete_setup_command", { language, region });
}

/** 修改语言/地区（引导之后使用，不会改动"已完成"标记）。 */
export async function updateSetup(language: string, region: string) {
  return invoke<SetupState>("update_setup_command", { language, region });
}

/** 重置引导状态，下次启动重新走一遍 OOBE（调试引导界面时常用）。 */
export async function resetSetup() {
  return invoke<SetupState>("reset_setup_command");
}
