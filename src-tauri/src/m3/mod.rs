//! M3 (Material Design 3) 动态配色引擎
//!
//! 使用成熟稳定的 `material-colors` crate（v0.3.3）实现：
//! - 从种子颜色生成完整色调调色板（Tonal Palette）
//! - 明 / 暗主题切换所需的色彩映射
//! - 符合 M3 规范的颜色角色（Color Role）分配
//! - 支持官方全部 9 种变体（TonalSpot / Vibrant / Monochrome ...）与对比度调节
//!
//! 参考：<https://crates.io/crates/material-colors>

pub mod commands;
pub mod extract;

use material_colors::color::Argb;
use material_colors::dynamic_color::Variant;

/// M3 标准的 13 个色阶（与 Material Theme Builder 一致）
pub const TONES: [i32; 13] = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99, 100];

/// 解析种子颜色，支持 `#rgb` / `#rrggbb` / `#aarrggbb` / `rrggbb` 形式
pub fn parse_seed(seed: &str) -> Option<Argb> {
    let s = seed.trim().trim_start_matches('#');
    let rgb = match s.len() {
        3 => {
            let mut out = String::with_capacity(6);
            for c in s.chars() {
                out.push(c);
                out.push(c);
            }
            out
        }
        6 => s.to_string(),
        // 8 位视为 AARRGGBB，仅取 RGB 部分。
        //
        // 不能用 `s[2..]`：`s.len()` 是**字节**长度，若下标 2 落在多字节字符中间
        // （例如 "€abcde" = 3 + 5 字节）会直接 panic。`get` 取不到就按非法处理。
        8 => match s.get(2..) {
            Some(rest) => rest.to_string(),
            None => return None,
        },
        _ => return None,
    };
    if rgb.len() != 6 || !rgb.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(&rgb, 16).ok()?;
    Some(Argb::from_u32(0xff00_0000 | value))
}

/// 将变体名称映射为 crate 的 `Variant` 枚举
pub fn variant_from_str(s: &str) -> Variant {
    match s.to_ascii_lowercase().as_str() {
        "monochrome" => Variant::Monochrome,
        "neutral" => Variant::Neutral,
        "vibrant" => Variant::Vibrant,
        "expressive" => Variant::Expressive,
        "fidelity" => Variant::Fidelity,
        "content" => Variant::Content,
        "rainbow" => Variant::Rainbow,
        "fruitsalad" | "fruit_salad" => Variant::FruitSalad,
        _ => Variant::TonalSpot,
    }
}

/// 将 `Variant` 枚举序列化回字符串（用于接口回传，便于前端配置同步）
pub fn variant_to_str(v: &Variant) -> &'static str {
    match v {
        Variant::Monochrome => "monochrome",
        Variant::Neutral => "neutral",
        Variant::TonalSpot => "tonal_spot",
        Variant::Vibrant => "vibrant",
        Variant::Expressive => "expressive",
        Variant::Fidelity => "fidelity",
        Variant::Content => "content",
        Variant::Rainbow => "rainbow",
        Variant::FruitSalad => "fruit_salad",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_seed_accepts_documented_forms() {
        assert!(parse_seed("#abc").is_some(), "3 位简写应展开为 6 位");
        assert!(parse_seed("#6750A4").is_some());
        assert!(parse_seed("6750A4").is_some());
        assert!(parse_seed("#FF6750A4").is_some(), "8 位按 AARRGGBB 取 RGB");
    }

    /// 回归：8 字节的**多字节**输入曾经会 panic。
    ///
    /// `s.len()` 是字节长度，`s[2..]` 一旦让下标 2 落在字符中间就直接 panic
    /// （例如 "€abcde" = 3 + 5 = 8 字节，或 "🎨🎨" = 4 + 4 = 8 字节）。
    /// 现在应当返回 `None`，而不是把命令线程打挂。
    #[test]
    fn parse_seed_never_panics_on_multibyte_input() {
        assert!(parse_seed("€abcde").is_none());
        assert!(parse_seed("🎨🎨").is_none());
        assert!(parse_seed("中中中").is_none());
        assert!(parse_seed("").is_none());
        assert!(parse_seed("#").is_none());
    }
}
