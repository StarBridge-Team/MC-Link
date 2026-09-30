pub mod push;
pub mod read;
pub mod write;
pub mod check;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct PersonalizationSettings {
    pub theme_color: String,
    pub theme_mode: String,
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
    pub bootstrap_icons_ready: bool,
    pub fonts_ready: bool,
    pub icon_ready: bool,
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
        section.replace(' ', "_").replace('/', "_").replace('\\', "_")
    );
    Ok(crate::datadir::setting_dir(data_dir)?.join(file))
}

pub const CONFIG_VERSION: u32 = 1;
