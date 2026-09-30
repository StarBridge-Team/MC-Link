//! M3 配色方案的 Tauri 命令实现
//!
//! 通过 `generate_m3_scheme` 命令，前端可以传入种子颜色（及变体 / 对比度），
//! 后端使用 `material-colors` crate 计算出：
//!   1. 完整的色调调色板（primary / secondary / tertiary / neutral / neutral_variant / error）
//!   2. 明 (light) 与 暗 (dark) 两套主题的角色色彩映射
//!   3. 符合 M3 规范的颜色角色分配（primary / container / surface / outline ...）
//! 同时为 Element Plus 的语义色（success / warning / error / info）生成配套的 M3 方案。
//!
//! 实现要点：
//!   - 调色板由 `CorePalette::of` 生成（HCT 色相 / 色度模型）；
//!   - 角色分配由 `DynamicScheme::by_variant` 生成，支持全部 9 种变体与对比度；
//!   - 结果统一转换为 hex 字符串，保证与前端镜像引擎结构一致、严格同步。

use std::collections::HashMap;

use material_colors::color::Argb;
use material_colors::dynamic_color::{DynamicScheme, Variant};
use material_colors::palette::{CorePalette, TonalPalette};
use material_colors::scheme::Scheme;
use serde::Serialize;

use super::{parse_seed, variant_from_str, variant_to_str, TONES};

/// 单个色调调色板（tone -> hex）
#[derive(Serialize)]
pub struct TonalRamp {
    pub tones: HashMap<i32, String>,
}

/// 完整的 M3 色调调色板集合
#[derive(Serialize)]
pub struct M3Palettes {
    pub primary: TonalRamp,
    pub secondary: TonalRamp,
    pub tertiary: TonalRamp,
    pub neutral: TonalRamp,
    pub neutral_variant: TonalRamp,
    pub error: TonalRamp,
}

/// 符合 M3 规范的颜色角色集合（全部为 hex 字符串）
#[derive(Serialize, Default)]
pub struct M3Roles {
    // primary
    pub primary: String,
    pub on_primary: String,
    pub primary_container: String,
    pub on_primary_container: String,
    pub inverse_primary: String,
    pub primary_fixed: String,
    pub on_primary_fixed: String,
    pub primary_fixed_dim: String,
    pub on_primary_fixed_variant: String,
    // secondary
    pub secondary: String,
    pub on_secondary: String,
    pub secondary_container: String,
    pub on_secondary_container: String,
    pub secondary_fixed: String,
    pub on_secondary_fixed: String,
    pub secondary_fixed_dim: String,
    pub on_secondary_fixed_variant: String,
    // tertiary
    pub tertiary: String,
    pub on_tertiary: String,
    pub tertiary_container: String,
    pub on_tertiary_container: String,
    pub tertiary_fixed: String,
    pub on_tertiary_fixed: String,
    pub tertiary_fixed_dim: String,
    pub on_tertiary_fixed_variant: String,
    // error
    pub error: String,
    pub on_error: String,
    pub error_container: String,
    pub on_error_container: String,
    // surface / background
    pub background: String,
    pub on_background: String,
    pub surface: String,
    pub on_surface: String,
    pub surface_variant: String,
    pub on_surface_variant: String,
    pub outline: String,
    pub outline_variant: String,
    pub shadow: String,
    pub scrim: String,
    pub inverse_surface: String,
    pub inverse_on_surface: String,
    pub surface_dim: String,
    pub surface_bright: String,
    pub surface_container_lowest: String,
    pub surface_container_low: String,
    pub surface_container: String,
    pub surface_container_high: String,
    pub surface_container_highest: String,
}

/// 单个语义色（如 success）在明 / 暗两套主题下的 M3 角色分配
#[derive(Serialize)]
pub struct M3SemanticColor {
    pub light: M3Roles,
    pub dark: M3Roles,
}

/// 为 Element Plus 语义色（success / warning / error / info）生成的 M3 方案
#[derive(Serialize)]
pub struct M3Semantic {
    pub success: M3SemanticColor,
    pub warning: M3SemanticColor,
    pub error: M3SemanticColor,
    pub info: M3SemanticColor,
}

/// 完整的 M3 配色方案返回结构（前后端通过此结构同步）
#[derive(Serialize)]
pub struct M3Scheme {
    pub seed: String,
    pub variant: String,
    pub contrast: f64,
    pub palettes: M3Palettes,
    pub light: M3Roles,
    pub dark: M3Roles,
    pub semantic: M3Semantic,
}

fn ramp(palette: &TonalPalette) -> TonalRamp {
    let mut tones = HashMap::new();
    for &t in TONES.iter() {
        tones.insert(t, palette.tone(t).to_hex_with_pound());
    }
    TonalRamp { tones }
}

fn build_palettes(argb: Argb) -> M3Palettes {
    // 色调调色板由 HCT 色相 / 色度模型生成（与 Material Theme Builder 一致）
    let core = CorePalette::of(argb);
    M3Palettes {
        primary: ramp(&core.primary),
        secondary: ramp(&core.secondary),
        tertiary: ramp(&core.tertiary),
        neutral: ramp(&core.neutral),
        neutral_variant: ramp(&core.neutral_variant),
        error: ramp(&core.error),
    }
}

/// 将 crate 的 `Scheme`（M3 角色集合）转换为 hex 字符串结构
fn roles_from_scheme(s: &Scheme) -> M3Roles {
    M3Roles {
        primary: s.primary.to_hex_with_pound(),
        on_primary: s.on_primary.to_hex_with_pound(),
        primary_container: s.primary_container.to_hex_with_pound(),
        on_primary_container: s.on_primary_container.to_hex_with_pound(),
        inverse_primary: s.inverse_primary.to_hex_with_pound(),
        primary_fixed: s.primary_fixed.to_hex_with_pound(),
        on_primary_fixed: s.on_primary_fixed.to_hex_with_pound(),
        primary_fixed_dim: s.primary_fixed_dim.to_hex_with_pound(),
        on_primary_fixed_variant: s.on_primary_fixed_variant.to_hex_with_pound(),

        secondary: s.secondary.to_hex_with_pound(),
        on_secondary: s.on_secondary.to_hex_with_pound(),
        secondary_container: s.secondary_container.to_hex_with_pound(),
        on_secondary_container: s.on_secondary_container.to_hex_with_pound(),
        secondary_fixed: s.secondary_fixed.to_hex_with_pound(),
        on_secondary_fixed: s.on_secondary_fixed.to_hex_with_pound(),
        secondary_fixed_dim: s.secondary_fixed_dim.to_hex_with_pound(),
        on_secondary_fixed_variant: s.on_secondary_fixed_variant.to_hex_with_pound(),

        tertiary: s.tertiary.to_hex_with_pound(),
        on_tertiary: s.on_tertiary.to_hex_with_pound(),
        tertiary_container: s.tertiary_container.to_hex_with_pound(),
        on_tertiary_container: s.on_tertiary_container.to_hex_with_pound(),
        tertiary_fixed: s.tertiary_fixed.to_hex_with_pound(),
        on_tertiary_fixed: s.on_tertiary_fixed.to_hex_with_pound(),
        tertiary_fixed_dim: s.tertiary_fixed_dim.to_hex_with_pound(),
        on_tertiary_fixed_variant: s.on_tertiary_fixed_variant.to_hex_with_pound(),

        error: s.error.to_hex_with_pound(),
        on_error: s.on_error.to_hex_with_pound(),
        error_container: s.error_container.to_hex_with_pound(),
        on_error_container: s.on_error_container.to_hex_with_pound(),

        background: s.background.to_hex_with_pound(),
        on_background: s.on_background.to_hex_with_pound(),
        surface: s.surface.to_hex_with_pound(),
        on_surface: s.on_surface.to_hex_with_pound(),
        surface_variant: s.surface_variant.to_hex_with_pound(),
        on_surface_variant: s.on_surface_variant.to_hex_with_pound(),
        outline: s.outline.to_hex_with_pound(),
        outline_variant: s.outline_variant.to_hex_with_pound(),
        shadow: s.shadow.to_hex_with_pound(),
        scrim: s.scrim.to_hex_with_pound(),
        inverse_surface: s.inverse_surface.to_hex_with_pound(),
        inverse_on_surface: s.inverse_on_surface.to_hex_with_pound(),
        surface_dim: s.surface_dim.to_hex_with_pound(),
        surface_bright: s.surface_bright.to_hex_with_pound(),
        surface_container_lowest: s.surface_container_lowest.to_hex_with_pound(),
        surface_container_low: s.surface_container_low.to_hex_with_pound(),
        surface_container: s.surface_container.to_hex_with_pound(),
        surface_container_high: s.surface_container_high.to_hex_with_pound(),
        surface_container_highest: s.surface_container_highest.to_hex_with_pound(),
    }
}

/// 生成指定种子色在明 / 暗两套主题下的角色分配（支持变体与对比度）
fn scheme_roles(source: Argb, variant: &Variant, contrast: f64) -> (M3Roles, M3Roles) {
    let light: Scheme =
        DynamicScheme::by_variant(source, variant, false, Some(contrast)).into();
    let dark: Scheme = DynamicScheme::by_variant(source, variant, true, Some(contrast)).into();
    (roles_from_scheme(&light), roles_from_scheme(&dark))
}

/// 生成完整的 M3 配色方案（接口 / 配置同步的核心命令）
#[tauri::command]
pub fn generate_m3_scheme(
    seed: String,
    variant: Option<String>,
    contrast: Option<f64>,
) -> Result<M3Scheme, String> {
    let argb = parse_seed(&seed).ok_or_else(|| format!("无效的种子颜色: {seed}"))?;
    let variant: Variant = variant_from_str(&variant.unwrap_or_else(|| "tonal_spot".to_string()));
    let contrast = contrast.unwrap_or(0.0).clamp(-1.0, 1.0);

    let (light, dark) = scheme_roles(argb, &variant, contrast);

    // 语义色采用固定的 Material 基准种子，确保 success/warning/error/info
    // 在任意主题色下都能保持一致的语义可读性，并遵循 M3 角色分配。
    let green = Argb::from_u32(0xff2e7d32);
    let orange = Argb::from_u32(0xff_ed_6c_02);
    let red = Argb::from_u32(0xff_d3_2f_2f);

    let semantic = |source: Argb| {
        let (light, dark) = scheme_roles(source, &variant, contrast);
        M3SemanticColor { light, dark }
    };

    Ok(M3Scheme {
        seed: seed.clone(),
        variant: variant_to_str(&variant).to_string(),
        contrast,
        palettes: build_palettes(argb),
        light,
        dark,
        semantic: M3Semantic {
            success: semantic(green),
            warning: semantic(orange),
            error: semantic(red),
            info: semantic(argb),
        },
    })
}
