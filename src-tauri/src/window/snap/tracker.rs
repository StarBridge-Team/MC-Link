//! 最大化按钮矩形的校验、去重与像素换算。
//!
//! 前端（`src/composables/useSnapTracking.ts`）跟随 `#snap-maximize` 元素，把它的
//! **CSS 逻辑像素**矩形上报到这里；本文只做三件纯逻辑的事：
//!
//! 1. 挡住"元素还没布局完"上报的 0×0 / NaN（否则会创建一个 0 尺寸窗口）；
//! 2. 丢掉与上次完全相同的矩形——拖动窗口时 `ResizeObserver` 会高频触发，
//!    而每次应用都是一次 `SetWindowPos`，会白白让 DWM 重新合成；
//! 3. 逻辑像素 → 物理像素（`SetWindowPos` 只认物理像素）。

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Deserialize;

/// 最大化按钮矩形：**CSS 逻辑像素**，左上角相对 WebView 客户区。
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl SnapBounds {
    /// 是否值得应用。
    ///
    /// 下界取 8px：比任何真实按钮都小，能过滤掉"元素刚插入、样式还没落"的 0×0；
    /// 上界只是防御性上限，避免一条异常上报把覆盖层做成整窗大小。
    pub fn is_usable(&self) -> bool {
        let within = |v: f64, min: f64, max: f64| v.is_finite() && v >= min && v <= max;
        within(self.width, 8.0, 2_000.0)
            && within(self.height, 8.0, 200.0)
            && within(self.x, -50.0, 20_000.0)
            && within(self.y, -50.0, 200.0)
    }

    /// 逻辑像素 → 物理像素（四舍五入）。
    ///
    /// `scale` 取 `WebviewWindow::scale_factor()`：它随显示器 DPI 变化，
    /// 跨屏拖动后新一次上报会用新比例重新换算。
    pub fn to_physical(&self, scale: f64) -> (i32, i32, i32, i32) {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let round = |v: f64| (v * scale).round() as i32;
        (
            round(self.x),
            round(self.y),
            round(self.width),
            round(self.height),
        )
    }
}

/// 一次上报的处置结果。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapChange {
    /// 与上次相同：什么都不用做（最常见的情况）。
    Unchanged,
    /// 按钮不在了或上报无效：隐藏覆盖层。
    Hide,
    /// 新的矩形：创建或移动覆盖层。
    Set(SnapBounds),
}

/// 每个窗口"上一次真正应用过的矩形"。
#[derive(Default)]
pub struct SnapTracker {
    /// 值里的 `f64` 是应用当时的缩放比例。
    ///
    /// **必须一起比较**：把窗口拖到另一块不同 DPI 的显示器上时，CSS 矩形一点没变
    /// （`getBoundingClientRect` 返回的是逻辑像素），但物理位置变了。只看矩形就会
    /// 把这次上报判成"没变化"，覆盖层于是留在旧显示器的坐标上——按钮再也点不到。
    last: Mutex<HashMap<String, (SnapBounds, f64)>>,
}

impl SnapTracker {
    /// 记录一次上报并给出要不要动覆盖层。`scale` 是当前的 `scale_factor`。
    pub fn update(&self, label: &str, bounds: Option<SnapBounds>, scale: f64) -> SnapChange {
        let mut last = match self.last.lock() {
            Ok(guard) => guard,
            // 锁中毒说明上一次更新 panic 了；隐藏覆盖层比拿旧矩形继续用安全。
            Err(_) => return SnapChange::Hide,
        };

        let Some(bounds) = bounds.filter(SnapBounds::is_usable) else {
            last.remove(label);
            return SnapChange::Hide;
        };

        if last.get(label) == Some(&(bounds, scale)) {
            return SnapChange::Unchanged;
        }
        last.insert(label.to_string(), (bounds, scale));
        SnapChange::Set(bounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f64, y: f64) -> SnapBounds {
        SnapBounds {
            x,
            y,
            width: 34.0,
            height: 30.0,
        }
    }

    #[test]
    fn identical_bounds_are_dropped() {
        let tracker = SnapTracker::default();
        assert_eq!(
            tracker.update("main", Some(bounds(100.0, 10.0)), 1.0),
            SnapChange::Set(bounds(100.0, 10.0))
        );
        assert_eq!(
            tracker.update("main", Some(bounds(100.0, 10.0)), 1.0),
            SnapChange::Unchanged
        );
        assert_eq!(
            tracker.update("main", Some(bounds(120.0, 10.0)), 1.0),
            SnapChange::Set(bounds(120.0, 10.0))
        );
    }

    #[test]
    fn dpi_change_reapplies_even_with_identical_bounds() {
        // 拖到另一块不同缩放的显示器：CSS 矩形一模一样，但物理位置变了。
        let tracker = SnapTracker::default();
        tracker.update("main", Some(bounds(100.0, 10.0)), 1.0);
        assert_eq!(
            tracker.update("main", Some(bounds(100.0, 10.0)), 1.5),
            SnapChange::Set(bounds(100.0, 10.0))
        );
    }

    #[test]
    fn zero_sized_bounds_hide_instead_of_applying() {
        let tracker = SnapTracker::default();
        let zero = SnapBounds {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        assert_eq!(tracker.update("main", Some(zero), 1.0), SnapChange::Hide);
    }

    #[test]
    fn none_hides_and_forgets() {
        let tracker = SnapTracker::default();
        tracker.update("main", Some(bounds(1.0, 2.0)), 1.0);
        assert_eq!(tracker.update("main", None, 1.0), SnapChange::Hide);
        // 忘掉之后同一个矩形要重新应用，否则窗口重建后覆盖层不会被摆回去。
        assert_eq!(
            tracker.update("main", Some(bounds(1.0, 2.0)), 1.0),
            SnapChange::Set(bounds(1.0, 2.0))
        );
    }

    #[test]
    fn windows_do_not_share_state() {
        let tracker = SnapTracker::default();
        tracker.update("main", Some(bounds(1.0, 1.0)), 1.0);
        assert_eq!(
            tracker.update("room-1", Some(bounds(1.0, 1.0)), 1.0),
            SnapChange::Set(bounds(1.0, 1.0))
        );
    }

    #[test]
    fn physical_rounding_handles_fractional_scaling() {
        let b = SnapBounds {
            x: 10.4,
            y: 10.5,
            width: 34.0,
            height: 30.0,
        };
        assert_eq!(b.to_physical(1.25), (13, 13, 43, 38));
        assert_eq!(b.to_physical(0.0), (10, 11, 34, 30));
    }
}
