//! 窗口级显隐。
//!
//! # 结论：不做窗口动画（至少不用 `AnimateWindow`）
//!
//! 曾经在这里用 `AnimateWindow(AW_BLEND)` 做系统渲染的淡入淡出，**已移除**。实测后果：
//!
//! - `AnimateWindow` 是**同步阻塞**的，而 Tauri 的同步命令跑在主线程（窗口消息泵所在线程）。
//!   在消息处理中调用它会把整个消息泵卡住：主窗口无法最小化/关闭、托盘点击无响应。
//! - 它还会让窗口内容不再合成：房间窗口与托盘面板显示为一片透明/空白（只有材质）。
//! - 透明窗口在 Windows 上是 layered window，`AW_BLEND` 对它本就不保证成功。
//!
//! 换句话说：**没有既"系统渲染"又不阻塞消息泵、还能作用于透明 WebView 窗口的公开 API**。
//! 想给**窗口本身**加动画，只有三条路，都需要重新评估：
//!
//! 1. 窗口内用 CSS 过渡——只能演内容、演不了窗口：最小化是 DWM 直接对窗口抓帧，
//!    页面里怎么淡出都拦不住那一步（详见下节）；
//! 2. DWM 的过渡效果（`DWMWA_TRANSITIONS_FORCEDISABLED` 只能关不能定制）；
//! 3. 自绘分层窗口（把 WebView 内容抓帧后自己合成）——成本远超收益。
//!
//! # CSS 只能演内容，不能演窗口
//!
//! `App.vue` 里的进场淡入（`.win-enter-*`）是合法的：它在窗口**已经显示之后**播放，
//! 不延迟任何系统动作。反过来的"先淡出、再调 `minimize()`"是错的——淡出那段时间
//! 窗口还停在原地，用户看到的是"点了没反应"，200ms 之后才轮到真正的系统动画，
//! 于是一个动作被拆成两段。这套 `isExiting` + `EXIT_MS` 已从
//! `src/composables/useWindowControls.ts` 删除，两个窗口统一为即时执行。
//!
//! 因此这里保留 `show` / `hide` 两个入口（调用方不必知道实现），内部只做瞬时显隐。
//! 将来要接动画，改这两个函数即可，调用点无需改动。
//!
//! # 最小化/最大化动画：由系统给，但受系统开关控制（2026-11-01 实测）
//!
//! `SPI_GETANIMATION` / `ANIMATIONINFO.iMinAnimate` 就是「辅助功能 → 视觉效果 →
//! 动画效果」与「性能选项 → 在最大化和最小化时显示窗口动画」背后的那个开关
//! （同一份状态也写在 `HKCU\Control Panel\Desktop\WindowMetrics\MinAnimate`）。
//! 它关掉时**任何**程序都没有这段动画：`ShowWindow(SW_MINIMIZE)` 就是瞬间消失。
//! 这很容易被误判成"无边框窗口的毛病"——本仓库当年那套"先播 200ms CSS 淡出再
//! `minimize()`"就是为这个洞打的补丁。现在不补：那是用户的系统偏好，程序不该绕过。
//!
//! 无边框窗口**能**播放这段动画所需的样式，一个都不能少（实测主窗口：
//! `STYLE=0x34CF0000`、`EXSTYLE=0x00040110`）：
//!
//! - `WS_THICKFRAME`：`resizable: true` 时 tao 会给；只有算窗口尺寸用的
//!   `to_adjusted_window_styles()` 才剥掉它。Electron 的 `thickFrame` 文档把这条
//!   说得很直白——去掉它会"移除窗口阴影与窗口动画"；
//! - `WS_CAPTION` / `WS_MINIMIZEBOX`：tao 对 `decorations: false` 仍然保留；
//! - 不能是 `WS_EX_LAYERED`，也不能是 `WS_EX_TOOLWINDOW`（工具窗口没有任务栏，
//!   也就没有"缩进任务栏"的目标）。
//!
//! 所以**不要**为了让动画"看起来有"去改窗口样式或加透明通道：当前三个样式位都在、
//! 两个排除位都没有，形式是对的，缺的只是系统开关。
//!
//! 与之相对，`DWMWA_TRANSITIONS_FORCEDISABLED` 只能**关**过渡，没有"强制开"的用法；
//! 而且对透明无边框窗口的显示淡入是 no-op（外部实测，见 `docs/ARCHITECTURE.md`）。

use tauri::WebviewWindow;

/// 启动时检查一次"系统是否允许最小化/最大化动画"，关了就在日志里说清楚。
///
/// 只为解决一类反复出现的误判：用户看到最小化瞬间完成，以为是窗口实现的问题，
/// 实际是系统开关关着。留一行日志比每次重新排查便宜。
#[cfg(windows)]
pub fn log_system_animation_state() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, ANIMATIONINFO, SPI_GETANIMATION,
    };

    let mut info = ANIMATIONINFO {
        cbSize: std::mem::size_of::<ANIMATIONINFO>() as u32,
        iMinAnimate: 1,
    };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETANIMATION,
            info.cbSize,
            &mut info as *mut _ as *mut core::ffi::c_void,
            0,
        )
    };
    if ok != 0 && info.iMinAnimate == 0 {
        eprintln!(
            "[窗口] 系统已关闭「最小化/最大化时的窗口动画」（辅助功能 → 视觉效果）。\
             最小化不会有系统动画，与本程序无关"
        );
    }
}

#[cfg(not(windows))]
pub fn log_system_animation_state() {}

/// 显示窗口。
///
/// 先 `show` 再 `set_focus`：反过来的话，某些平台上 `set_focus` 会作用在仍隐藏的窗口上，
/// 表现为"窗口出现了但没有焦点"。
pub fn show(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

/// 隐藏窗口。
pub fn hide(window: &WebviewWindow) {
    let _ = window.hide();
}
