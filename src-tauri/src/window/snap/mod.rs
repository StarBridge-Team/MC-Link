//! Windows 11 贴靠布局（Snap Layouts）。
//!
//! # 问题
//!
//! 无边框窗口的"最大化"是自绘的 HTML 按钮，Windows 根本看不见它：贴靠布局只在
//! 命中测试（`WM_NCHITTEST`）对最大化按钮返回 `HTMAXBUTTON` 时才会弹出。
//!
//! # 为什么不能改主窗口的窗口过程
//!
//! 鼠标落在 WebView2 上时，收到 `WM_NCHITTEST` 的是 **WebView2 自己的子窗口**，
//! 主窗口收不到（输入路由由 Chromium 接管）。所以在主窗口上子类化/挂钩消息是无效的。
//!
//! # 这里的做法
//!
//! 在按钮矩形上盖一个**不绘制的原生子窗口**（[`overlay`]），它的窗口过程由我们
//! 自己注册，对 `WM_NCHITTEST` 返回 `HTMAXBUTTON`。系统据此把这片区域当成顶层的
//! 最大化按钮：悬停弹出贴靠布局，点击的 `WM_NCLBUTTON*` 也发回给它。
//! 它自己不画像素，按钮的外观仍然来自前端。
//!
//! # 位置从哪来
//!
//! 前端用 `ResizeObserver` + resize/scroll 事件跟随 `id="snap-maximize"` 的元素，
//! 把 CSS 逻辑像素矩形上报过来（`src/composables/useSnapTracking.ts`）。
//! 校验、去重、逻辑→物理像素换算都在 [`tracker`]。
//!
//! # 平台
//!
//! 只有 Windows 有贴靠布局；其它平台 `update` 是空操作，前端不需要判断平台。

pub mod tracker;

#[cfg(windows)]
mod overlay;

pub use tracker::{SnapBounds, SnapChange, SnapTracker};

/// 应用一次前端上报：`None` 表示按钮不存在（隐藏覆盖层）。
///
/// 调用方必须保证在主线程（见 [`overlay::apply`] 的说明）。
pub fn update(window: &tauri::WebviewWindow, bounds: Option<SnapBounds>) -> Result<(), String> {
    #[cfg(windows)]
    {
        overlay::apply(window, bounds)
    }

    #[cfg(not(windows))]
    {
        let _ = (window, bounds);
        Ok(())
    }
}
