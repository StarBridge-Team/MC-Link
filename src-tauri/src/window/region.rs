//! 圆角裁剪：把窗口的形状裁成圆角矩形。**只有 Windows 能做到。**
//!
//! # Windows：为什么必须裁窗口，而不是只写 CSS
//!
//! 窗口是**矩形**的。CSS 的 `border-radius` 只裁剪 DOM 的绘制，裁不掉窗口本身的底色——
//! 去掉材质后那层底色就是 WebView2 的白，于是圆角外面会露出四个白角 / 一条白边。
//! `SetWindowRgn` 直接把窗口形状改成圆角矩形，白底连同一并消失。
//!
//! # macOS / Linux：没有窗口形状裁剪能力
//!
//! 这两个平台没有 `SetWindowRgn` 的等价物（tao / GTK 未暴露该能力），
//! 窗口**始终是直角矩形**，圆角只来自前端 CSS。
//!
//! # 半径是参数，不是常量
//!
//! 圆角必须与**该窗口自己的**前端 `border-radius` 一致，所以半径由调用方给。
//! 目前**只有托盘面板**调用本函数（`window/widget.rs` 里的两处），传 30（对齐 `WidgetApp.vue`）。
//!
//! # 调用时机：只能挂在 `Resized` 上
//!
//! region 记的是**调用当时的像素矩形**，窗口尺寸一变就失效（变大后右下角露出没裁到的底色）。
//! 更关键的是 `set_size` / `maximize` 都是异步投递的，紧接着调本函数时 `inner_size()`
//! 往往还是旧值——"设完尺寸立刻裁"必然按旧尺寸裁。因此只能等 `Resized` 事件到了再算。

/// 把窗口裁成圆角矩形。`radius` 是**逻辑像素**，内部按窗口缩放比例换算。
#[cfg(windows)]
pub(crate) fn apply_round_region(window: &tauri::WebviewWindow, radius: f64) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let radius = (radius * scale).round() as i32;

    // 椭圆宽高是**直径**；右/下边界是开区间，+1 才覆盖到最后一个像素。
    let region = unsafe {
        CreateRoundRectRgn(
            0,
            0,
            size.width as i32 + 1,
            size.height as i32 + 1,
            radius * 2,
            radius * 2,
        )
    };
    if region.is_null() {
        eprintln!("[托盘] CreateRoundRectRgn 失败，窗口未裁剪");
        return;
    }
    // 成功后 region 归系统所有，**不要**再 DeleteObject。
    let applied = unsafe { SetWindowRgn(win32.hwnd.get() as _, region, 1) };
    if applied == 0 {
        eprintln!("[托盘] SetWindowRgn 失败（窗口未裁剪，圆角处会露底色）");
    }
}

/// macOS / Linux：**什么都不做**，窗口保持直角。
///
/// # 为什么这里不能"假装"能裁形状
///
/// 圆角在这两个平台上**只来自前端 CSS 的 `border-radius`**（托盘面板是 30px，
/// 对应 `WidgetApp.vue`）。CSS 只改变**绘制**，不改变**命中测试**，因此：
///
/// - 窗口本身仍是矩形 surface，四个角的 30px 透明区域**照样接收鼠标事件**
///   ——点空白处不会"穿透"到下面的应用；
/// - tao / GTK 没有暴露窗口形状裁剪能力（`SetWindowRgn` 是 Win32 专有），
///   所以在这两个平台上**没有**可用的替代手段，不要在这里臆造一个。
///
/// # 误命中的实际影响有限
///
/// 托盘面板在 `WindowEvent::Focused(false)`（失焦）时会自动收起（见 `window/widget.rs`），
/// 所以"点到圆角外的空白"最多是这一次点击被面板吃掉、面板没立刻关掉；
/// 用户再点别处失焦即收起，不会造成状态错乱或数据丢失。
///
/// 换句话说：**这是已知且可接受的平台差异**，不要为此重复踩坑。
#[cfg(not(windows))]
pub(crate) fn apply_round_region(_window: &tauri::WebviewWindow, _radius: f64) {
    // 平台没有窗口形状裁剪能力，CSS 的 border-radius 已经是能做到的全部。
}
