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
    {
        "hud_window".to_string()
    }
    #[cfg(windows)]
    {
        "mica".to_string()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        "none".to_string()
    }
}

/// 启动时按**已保存的设置**预应用窗口效果。
///
/// 必须使用持久化的值而不是平台默认值：否则用户设置的 acrylic / none 会在启动瞬间
/// 先被默认的 mica 覆盖一次，界面出现可见闪烁（此前 `useAppInit.ts` 里那句
/// "setup_window_effects 启动时会预应用 mica" 的注释就是在描述这个症状）。
pub fn setup_window_effects(app: &tauri::App, effect: &str, tint: f64, dark: bool) {
    if let Some(window) = app.get_webview_window("main") {
        apply_effect(&window, effect, tint, dark);
    }
}

/// 按名称应用窗口效果。**效果名 → 平台实现的唯一映射处**。
///
/// 启动预应用（[`setup_window_effects`]）与 `set_window_effect` 命令都走这里，
/// 避免两处各写一份映射而漂移。
///
/// 取值集合（`mica` / `acrylic` / `hud_window` / `none`）是持久化设置的跨层契约，
/// 改名会让老用户配置失效，故保持不变；新增取值需同步前端。
///
/// `tint`（0–100）是材质染色浓度：**越高材质越实**。
///
/// 注意它的作用范围很窄，**只有 Acrylic 会真的用上**：
/// Windows 的 `apply_acrylic` 接收一个 color，而 `apply_mica` 只设
/// `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_MAINWINDOW` —— Mica 的浓淡完全由系统合成器决定，
/// 这个值会被静默忽略（macOS 的 hud_window 同理不受它控制）。
///
/// 想控制"看得见多少材质"应该用页面背景的不透明度
/// （`config::PersonalizationSettings::background_opacity`），那是压在上层的一层，
/// 对任何材质都有效。
///
/// `dark` 决定染色往黑还是往白走（材质本身在 Windows 上是纯色半透明，
/// 染色色相只影响"半透明的那一半"叠上来的观感）。
pub fn apply_effect(window: &tauri::WebviewWindow, effect: &str, tint: f64, dark: bool) {
    let config = effects_for(effect, tint, dark);

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
fn effects_for(effect: &str, tint: f64, dark: bool) -> Option<WindowEffectsConfig> {
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
        radius: None,
        // `color` 就是材质染色层的颜色。此前留空靠官方默认（不透明纯色），
        // 于是材质永远是一块实心面板、完全调不了浓度；这里改由「材质浓度」显式给 alpha。
        color: tint_color(tint, dark),
        // interactive 取官方默认值：用 ..Default::default() 而不是逐字段列出，
        // 官方新增字段时不必跟着改
        ..Default::default()
    })
}

/// 材质浓度（0–100）→ 材质染色颜色。
///
/// 0 返回 `None` = 不覆盖官方参数（材质保持系统原样）；
/// 其余线性映射到 alpha。深色模式往黑走、浅色往白走。
///
/// **只有 Acrylic 会用到它**：`window_vibrancy::apply_mica` 只设
/// `DWMWA_SYSTEMBACKDROP_TYPE`，根本不读 color，所以 Mica 下这个值会被静默忽略
/// （见 [`apply_effect`] 的说明）。
fn tint_color(tint: f64, dark: bool) -> Option<tauri::window::Color> {
    let alpha = (clamp_tint(tint) / 100.0 * 255.0).round() as u8;
    if alpha == 0 {
        return None;
    }
    let base = if dark { 0u8 } else { 255u8 };
    Some(tauri::window::Color(base, base, base, alpha))
}

/// 收敛到 0–100；非法值（NaN / 无穷）按 0 处理。
fn clamp_tint(tint: f64) -> f64 {
    if !tint.is_finite() {
        return 0.0;
    }
    tint.clamp(0.0, 100.0)
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

/// 设置窗口沉浸式深色模式（`DWMWA_USE_IMMERSIVE_DARK_MODE`）。
///
/// Mica/Acrylic 背景的明暗由该属性决定，而非页面 CSS；
/// 不设置时跟随系统主题，导致"应用深色 + 系统浅色"时材质仍为浅色。
///
/// # 为什么改用 `windows-sys` 而不是手写 `#[link(name = "dwmapi")]`
///
/// Tauri 没有等价 API（官方 `set_effects` 只接受"用哪个材质"，不接受
/// "材质不变、只改亮暗"），所以这一步必须自己调 DwmApi。但**手写 `extern "system"`
/// 声明**要自己维护签名与常量值（例如 20 这个魔法数字在旧 SDK 里是 19），
/// 一旦签名写错就是 UB。`windows-sys` 由 Microsoft 官方维护、与 Windows SDK 同步，
/// 签名与常量都由它给出；它本来就在依赖树里（Tauri 自身依赖 0.61.2），不新增编译单元。
#[cfg(windows)]
pub(crate) fn set_immersive_dark_mode(window: &tauri::WebviewWindow, dark: bool) {
    use raw_window_handle::HasWindowHandle;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as windows_sys::Win32::Foundation::HWND;

    // `DWMWA_USE_IMMERSIVE_DARK_MODE` 期望一个 Win32 BOOL（即 i32：0 = 关，非 0 = 开）。
    // windows-sys 在该属性上没有单独的标量类型，直接用 i32 并让 `cbAttribute` 与之一致。
    let value: i32 = if dark { 1 } else { 0 };
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &value as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&value) as u32,
        )
    };
    // 返回 HRESULT：失败不 panic，但留痕（材质明暗不对时这是唯一线索）
    if result < 0 {
        eprintln!("[窗口效果] 设置沉浸式深色模式失败: HRESULT=0x{result:08X}");
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
            effects_for("mica", 0.0, false).unwrap().effects,
            vec![Effect::Mica]
        );
        assert_eq!(
            effects_for("acrylic", 0.0, false).unwrap().effects,
            vec![Effect::Acrylic]
        );
        assert_eq!(
            effects_for("hud_window", 0.0, false).unwrap().effects,
            vec![Effect::HudWindow]
        );
    }

    #[test]
    fn none_and_unknown_names_clear_the_effect() {
        // 关键行为：切到"无效果"必须显式清空，否则旧材质残留
        assert!(effects_for("none", 50.0, false).is_none());
        assert!(effects_for("transparent", 50.0, false).is_none());
        assert!(effects_for("", 50.0, false).is_none());
        assert!(
            effects_for("mica_dark", 50.0, false).is_none(),
            "未知取值一律清空而非猜测"
        );
    }

    #[test]
    fn macos_material_state_is_active() {
        // 与迁移前的手写实现保持一致（setState: NSVisualEffectStateActive）
        assert_eq!(
            effects_for("hud_window", 0.0, false).unwrap().state,
            Some(EffectState::Active)
        );
    }

    #[test]
    fn zero_tint_leaves_color_to_official_default() {
        // 0 = 不覆盖官方默认：老用户升级后观感必须与加本项之前完全一致
        assert_eq!(effects_for("mica", 0.0, false).unwrap().color, None);
        // 非法值同样按 0 处理，不能算出一个诡异的 alpha 把窗口糊住
        assert_eq!(effects_for("mica", f64::NAN, false).unwrap().color, None);
        assert_eq!(effects_for("mica", -10.0, false).unwrap().color, None);
    }

    #[test]
    fn tint_scales_alpha_linearly() {
        assert_eq!(tint_color(100.0, false), Some(tauri::window::Color(255, 255, 255, 255)));
        assert_eq!(tint_color(50.0, false), Some(tauri::window::Color(255, 255, 255, 128)));
        // 超过 100 收敛到上限而不是溢出（u8 溢出会让 alpha 变成很小的值）
        assert_eq!(tint_color(999.0, false), tint_color(100.0, false));
    }

    #[test]
    fn tint_darkens_or_lightens_by_theme() {
        // 深色往黑走、浅色往白走：材质是半透明纯色，base 决定透出来那半边的基调
        assert_eq!(tint_color(100.0, true), Some(tauri::window::Color(0, 0, 0, 255)));
        assert_eq!(tint_color(100.0, false), Some(tauri::window::Color(255, 255, 255, 255)));
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
