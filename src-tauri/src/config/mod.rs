pub mod check;
pub mod push;
pub mod read;
pub mod write;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
// 容器级 serde 默认：缺失字段回落到 `Default::default()` 的取值。
// 新增字段时**必须**靠它兜底——`config/read.rs` 是直接 `from_str` 反序列化，
// 没有"合并默认值"的步骤，缺字段会被判为"文件损坏"从而把用户设置整份隔离重置。
#[serde(default)]
pub struct PersonalizationSettings {
    pub theme_color: String,
    pub theme_mode: String,
    /// 配色变体（M3E `ThemeVariant` 取值：tonal-spot / vibrant / expressive / neutral / …）。
    pub theme_variant: String,
    /// 配色对比度（M3E `ContrastLevel` 取值：standard / medium / high）。
    pub theme_contrast: String,
    pub animation_enabled: bool,
    pub animation_speed: f64,
    pub transparent_effect: String,
    pub background_type: String,
    pub background_value: String,
    pub background_fit: String,
    pub background_overlay: bool,
    pub background_overlay_opacity: f64,
    pub music_mode: String,
    pub music_value: String,
    pub homepage_mode: String,
    pub homepage_value: String,
}

impl Default for PersonalizationSettings {
    fn default() -> Self {
        Self {
            theme_color: "#0066cc".to_string(),
            theme_mode: "system".to_string(),
            theme_variant: "tonal-spot".to_string(),
            theme_contrast: "standard".to_string(),
            animation_enabled: true,
            animation_speed: 1.0,
            // 平台默认效果（Windows 为 mica、macOS 为 hud_window）：
            // 让"从未设置"与"后端默认"一致，避免启动瞬间先应用默认值再被用户值覆盖而闪烁
            transparent_effect: crate::effect::get_default_effect(),
            background_type: "default".to_string(),
            background_value: String::new(),
            background_fit: "scale-to-fill".to_string(),
            background_overlay: false,
            background_overlay_opacity: 30.0,
            music_mode: "none".to_string(),
            music_value: String::new(),
            homepage_mode: "default".to_string(),
            homepage_value: String::new(),
        }
    }
}

#[derive(Serialize)]
pub struct InitAppData {
    pub personalization: PersonalizationSettings,
    pub default_effect: String,
    pub app_version: String,
    pub tauri_version: String,
}

#[derive(Serialize)]
pub struct PrepareAppData {
    pub personalization: PersonalizationSettings,
    pub default_effect: String,
    pub app_version: String,
    pub tauri_version: String,
    /// 资源清单版本（诊断用）。
    pub asset_version: String,
    /// 是否全部远程资源就绪（就绪才可注入对应 CSS）。
    pub assets_ready: bool,
    /// 每个资源的就绪状态，前端据此决定注入哪些 CSS、缺哪些。
    pub assets: Vec<crate::assets::pull::AssetState>,
    /// 失败原因，已去掉内部细节，可直接展示给用户。
    pub asset_failures: Vec<String>,
    /// 是否处于离线降级（未能拉到远程清单，仅用本地缓存判定）。
    pub assets_offline: bool,
}

#[derive(Serialize)]
pub struct IpInfo {
    pub region: String,
    pub isp: String,
}

/// 分区名 → 设置目录下的文件路径。
///
/// 读写共用同一份过滤规则（此前 `read.rs` 与 `write.rs` 各写一份，容易漂移）。
pub(crate) fn section_path(
    data_dir: &std::path::Path,
    section: &str,
) -> Result<std::path::PathBuf, String> {
    let file = format!(
        "{}.yml",
        section
            .replace(' ', "_")
            .replace('/', "_")
            .replace('\\', "_")
    );
    Ok(crate::datadir::setting_dir(data_dir)?.join(file))
}

pub const CONFIG_VERSION: u32 = 1;
