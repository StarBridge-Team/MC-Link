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

use tauri::WebviewWindow;

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
