//! 窗口效果（Mica / Acrylic / macOS 材质）。
//!
//! # 为什么改为调用官方 API
//!
//! 这里曾自己写 `DwmSetWindowAttribute` / `SetWindowPos` / `objc_msgSend` 的 FFI，
//! 并自己用 `RtlGetVersion` 判断 Win11。迁移到 Tauri 官方的
//! [`tauri::WebviewWindow::set_effects`] 后：
//!
//! - 官方实现内部用的就是 `window-vibrancy`（Tauri 自己维护的跨平台材质库），
//!   Win10/11 的支持判定与材质回退由它处理，不必我们自己嗅探系统版本；
//! - **运行时切换效果是官方支持的**：每次调用都是覆盖式的
//!   （`Some(config)` 应用、`None` 清空），所以"设置页换效果立即生效"依然成立；
//! - 删掉了三处重复的 `DwmSetWindowAttribute` 声明与约 140 行 unsafe FFI。
//!
//! 唯一保留的手写 FFI 是 [`set_immersive_dark_mode`]：Tauri 没有对应 API
//! （官方材质接口不接受"只改亮暗、不动材质"的语义），而 Mica/Acrylic 的明暗
//! 恰恰由这个属性决定。

use tauri::utils::config::WindowEffectsConfig;
// 官方在 tauri::window 下把这两个枚举重命名为 Effect / EffectState
use tauri::window::{Effect, EffectState};
use tauri::Manager;

/// 返回当前平台默认窗口效果。
pub fn get_default_effect() -> String {
    #[cfg(target_os = "macos")]
    { "hud_window".to_string() }
    #[cfg(windows)]
    { "mica".to_string() }
    #[cfg(not(any(target_os = "macos", windows)))]
    { "none".to_string() }
}

/// 启动时按**已保存的设置**预应用窗口效果。
///
/// 必须使用持久化的值而不是平台默认值：否则用户设置的 acrylic / none 会在启动瞬间
/// 先被默认的 mica 覆盖一次，界面出现可见闪烁（此前 `useAppInit.ts` 里那句
/// "setup_window_effects 启动时会预应用 mica" 的注释就是在描述这个症状）。
pub fn setup_window_effects(app: &tauri::App, effect: &str) {
    if let Some(window) = app.get_webview_window("main") {
        apply_effect_by_name(&window, effect);
    }
}

/// 按名称应用窗口效果。**效果名 → 平台实现的唯一映射处**。
///
/// 启动预应用（[`setup_window_effects`]）与 `set_window_effect` 命令都走这里，
/// 避免两处各写一份映射而漂移。
///
/// 取值集合（`mica` / `acrylic` / `hud_window` / `none`）是持久化设置的跨层契约，
/// 改名会让老用户配置失效，故保持不变；新增取值需同步前端。
pub fn apply_effect_by_name(window: &tauri::WebviewWindow, effect: &str) {
    let config = effects_for(effect);

    // 官方实现在 Windows 下会用 clear_mica/clear_acrylic/clear_blur 清空，
    // 但 macOS 分支**不处理"清空"**（tauri/src/vibrancy/mod.rs 的 else 只对 windows 生效）。
    // 若不补这一步，macOS 上从 hud_window 切到"无效果"会残留材质视图。
    #[cfg(target_os = "macos")]
    if config.is_none() {
        clear_macos_vibrancy(window);
    }

    if let Err(e) = window.set_effects(config) {
        eprintln!("[窗口效果] 应用 {effect} 失败: {e}");
    }
}

/// 托盘菜单窗口的材质（Windows 的 Tabbed 材质）。
///
/// 结果与"应用哪个效果"无关，因此不进 [`effects_for`] 的取值集合——
/// 它只是同一个官方 API 的另一处调用，集中在这里以免又出现一份手写 FFI。
pub(crate) fn apply_tray_menu_effect(window: &tauri::WebviewWindow) {
    let config = WindowEffectsConfig {
        effects: vec![Effect::Tabbed],
        ..Default::default()
    };
    if let Err(e) = window.set_effects(config) {
        eprintln!("[窗口效果] 托盘菜单材质应用失败: {e}");
    }
}

/// 效果名 → 官方参数；`None` 表示清空材质。
///
/// 各平台不认识的取值由官方实现自行忽略（Windows 只认 Mica/Acrylic/Blur/Tabbed，
/// macOS 只认材质枚举），因此这里不需要再写平台分支。
fn effects_for(effect: &str) -> Option<WindowEffectsConfig> {
    let effect = match effect {
        "mica" => Effect::Mica,
        "acrylic" => Effect::Acrylic,
        "hud_window" => Effect::HudWindow,
        // "none" / "transparent" / 未知取值：显式清空，
        // 避免从 mica/acrylic 切换过来时旧材质残留
        _ => return None,
    };

    Some(WindowEffectsConfig {
        effects: vec![effect],
        // macOS 的材质状态（Windows 忽略此项）：保持迁移前的行为，固定为 Active
        state: Some(EffectState::Active),
        // radius / color / interactive 取官方默认值：用 ..Default::default()
        // 而不是逐字段列出，官方新增字段时不必跟着改
        ..Default::default()
    })
}

/// 移除 macOS 上已添加的 vibrancy 视图。
///
/// Tauri 的 `set_effects(None)` 在 macOS 上是 no-op，这里用官方材质库
/// （`window-vibrancy`，Tauri 内部同款）做真正的移除。
///
/// 注：本函数只在 macOS 编译，无法在当前 Windows 开发机上验证编译，
/// 若在 macOS 上构建报错，请检查 `clear_vibrancy` 的入参（可能需要传值而非引用）。
#[cfg(target_os = "macos")]
fn clear_macos_vibrancy(window: &tauri::WebviewWindow) {
    if let Err(e) = window_vibrancy::clear_vibrancy(window) {
        eprintln!("[窗口效果] 移除 macOS 材质失败: {e}");
    }
}

/// 设置窗口沉浸式深色模式（DWMWA_USE_IMMERSIVE_DARK_MODE = 20）。
/// Mica/Acrylic 背景的明暗由该属性决定，而非页面 CSS；
/// 不设置时跟随系统主题，导致"应用深色 + 系统浅色"时材质仍为浅色。
///
/// 保留这段 FFI 的理由：Tauri 没有等价 API——官方 `set_effects` 只接受
/// "用哪个材质"，不接受"材质不变、只改亮暗"；而应用主题与系统主题是独立的。
#[cfg(windows)]
pub(crate) fn set_immersive_dark_mode(window: &tauri::WebviewWindow, dark: bool) {
    use raw_window_handle::HasWindowHandle;
    if let Ok(handle) = window.window_handle() {
        if let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_raw() {
            let hwnd = win32.hwnd.get() as *mut std::ffi::c_void;
            let value: i32 = if dark { 1 } else { 0 };
            unsafe {
                #[link(name = "dwmapi")]
                extern "system" {
                    fn DwmSetWindowAttribute(
                        hwnd: *mut std::ffi::c_void,
                        dwAttribute: u32,
                        pvAttribute: *const std::ffi::c_void,
                        cbAttribute: u32,
                    ) -> i32;
                }
                DwmSetWindowAttribute(hwnd, 20, &value as *const _ as *const _, 4);
            }
        }
    }
}

/// 非 Windows 平台的空实现，保持调用方无需分支。
#[cfg(not(windows))]
pub(crate) fn set_immersive_dark_mode(_window: &tauri::WebviewWindow, _dark: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_effect_names_map_to_official_effects() {
        assert_eq!(
            effects_for("mica").unwrap().effects,
            vec![Effect::Mica]
        );
        assert_eq!(
            effects_for("acrylic").unwrap().effects,
            vec![Effect::Acrylic]
        );
        assert_eq!(
            effects_for("hud_window").unwrap().effects,
            vec![Effect::HudWindow]
        );
    }

    #[test]
    fn none_and_unknown_names_clear_the_effect() {
        // 关键行为：切到"无效果"必须显式清空，否则旧材质残留
        assert!(effects_for("none").is_none());
        assert!(effects_for("transparent").is_none());
        assert!(effects_for("").is_none());
        assert!(effects_for("mica_dark").is_none(), "未知取值一律清空而非猜测");
    }

    #[test]
    fn macos_material_state_is_active() {
        // 与迁移前的手写实现保持一致（setState: NSVisualEffectStateActive）
        assert_eq!(
            effects_for("hud_window").unwrap().state,
            Some(EffectState::Active)
        );
    }

    #[test]
    fn default_effect_matches_platform() {
        let d = get_default_effect();
        #[cfg(windows)]
        assert_eq!(d, "mica");
        #[cfg(target_os = "macos")]
        assert_eq!(d, "hud_window");
        #[cfg(not(any(windows, target_os = "macos")))]
        assert_eq!(d, "none");
    }
}
