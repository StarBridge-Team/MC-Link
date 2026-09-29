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
            transparent_effect: "none".to_string(),
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

pub const CONFIG_VERSION: u32 = 1;
