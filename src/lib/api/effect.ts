import { invoke } from "@tauri-apps/api/core";

/** 获取当前平台默认窗口效果 */
export async function getDefaultEffect() {
  return invoke<string>("get_default_effect");
}

/**
 * 设置窗口效果（mica / acrylic / hud_window / transparent / none）。
 *
 * `tint` 是材质染色浓度（0–100，越高越不透明），`dark` 决定染色往黑还是往白走。
 * 两者都随调用一起传：材质是"半透明纯色 + 染色层"，缺了任何一个窗口都不会有变化。
 */
export async function setWindowEffect(effect: string, tint: number, dark: boolean) {
  return invoke<void>("set_window_effect", { effect, tint, dark });
}

/** 设置窗口沉浸式深色模式（Windows 上决定 Mica/Acrylic 材质明暗） */
export async function setWindowDarkMode(dark: boolean) {
  return invoke<void>("set_window_dark_mode", { dark });
}
