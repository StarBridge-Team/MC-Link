import { invoke } from "@tauri-apps/api/core";

/** 获取当前平台默认窗口效果 */
export async function getDefaultEffect() {
  return invoke<string>("get_default_effect");
}

/** 设置窗口效果（mica / acrylic / hud_window / transparent / none） */
export async function setWindowEffect(effect: string) {
  return invoke<void>("set_window_effect", { effect });
}

/** 设置窗口沉浸式深色模式（Windows 上决定 Mica/Acrylic 材质明暗） */
export async function setWindowDarkMode(dark: boolean) {
  return invoke<void>("set_window_dark_mode", { dark });
}
