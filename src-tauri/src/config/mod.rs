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
    /// 窗口材质的染色浓度（0–100）：**越高越不透明**。
    ///
    /// 只对 **Acrylic** 真正生效（Windows 的 `apply_acrylic` 接受一个 color）。
    /// **Mica 调不动**：它的实现只设 `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_MAINWINDOW`，
    /// 没有任何浓度/染色参数，材质浓淡完全由系统合成器决定 —— 见 `effect.rs` 的说明。
    ///
    /// 注：刻意不叫「强度」：那个词说不清"越高越透明"还是"越高越不透明"。
    pub effect_tint: f64,
    pub background_type: String,
    pub background_value: String,
    pub background_fit: String,
    pub background_overlay: bool,
    pub background_overlay_opacity: f64,
    /// 页面背景的不透明度（0–100）：**越高越遮住底下的材质**。
    ///
    /// 这才是"看得见多少 Mica/Acrylic"的那个旋钮：材质本身（尤其 Mica）不可调，
    /// 想控制它露多少只能靠压在上面这层页面背景的 alpha。
    ///
    ///   100 → 完全遮住材质（看不到桌面）；
    ///   0   → 完全不遮，材质/桌面完全显现；
    ///   50  → 半透明，材质半显现。
    pub background_opacity: f64,
    /// 背景不透明度的深色档（`background_opacity` 是浅色档）。
    ///
    /// 分档的理由同遮罩：深色下界面的可读性对"背景透出多少"更敏感，
    /// 常用值往往与浅色不同，共用一个值会逼用户在一种模式下妥协。
    pub background_opacity_dark: f64,
    /// 图片背景的模糊强度（px），分深浅各一份——深色下背景需要更强的柔化才压得住。
    pub background_image_blur_light: f64,
    pub background_image_blur_dark: f64,
    /// 视频背景的模糊强度（px），分深浅各一份。
    pub background_video_blur_light: f64,
    pub background_video_blur_dark: f64,
    /// 种子色背景的模糊强度（px），分深浅各一份。
    ///
    /// "种子色背景"就是 `default` 背景这一层，它有两种形态（见 [`Self::effect_tint`]）：
    /// 有材质时透出系统材质，无材质时露出我们自建的配色背景层（调色板 surface）。
    /// 两种形态下模糊都落在同一处，所以共用这一对字段。
    pub seed_blur_light: f64,
    pub seed_blur_dark: f64,
    /// 遮罩强度在深色下的另一份取值（`background_overlay_opacity` 是浅色那份）。
    ///
    /// 拆开的原因同理：深色背景本来就更暗，同样的遮罩会更"糊"，通常需要更低的值。
    pub background_overlay_opacity_dark: f64,
    pub music_mode: String,
    pub music_value: String,
    pub homepage_mode: String,
    pub homepage_value: String,
    /// 配色种子是否取自背景图（"配色跟随背景图"）。
    ///
    /// 开启后 `theme_color` 退居为**回退值**：取色失败（非图片、SVG、图损坏）时仍用它。
    /// 这是刻意设计——取色失败就"没有配色"会让界面直接失去主题色，比用回手选色更糟。
    ///
    /// 默认 `false`：这是新增行为，不能因为升级就改变老用户既有的配色。
    #[serde(default)]
    pub theme_from_background: bool,
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
            // 0 = 不覆盖材质自身的染色（材质保持系统原样）。
            //
            // 不默认 100 是因为 Mica 根本不接受这个参数、Acrylic 接受：默认"不干预"
            // 才能保证老用户升级后材质观感不变；要更实心自己往上调（Acrylic 才有反应）。
            effect_tint: 0.0,
            background_type: "default".to_string(),
            background_value: String::new(),
            background_fit: "scale-to-fill".to_string(),
            background_overlay: false,
            background_overlay_opacity: 30.0,
            // 100 = 页面背景完全不透明 = 完全遮住材质，与加入本项之前的行为一致。
            background_opacity: 100.0,
            background_opacity_dark: 100.0,
            background_image_blur_light: 0.0,
            background_image_blur_dark: 0.0,
            background_video_blur_light: 0.0,
            background_video_blur_dark: 0.0,
            seed_blur_light: 0.0,
            seed_blur_dark: 0.0,
            background_overlay_opacity_dark: 30.0,
            music_mode: "none".to_string(),
            music_value: String::new(),
            homepage_mode: "default".to_string(),
            homepage_value: String::new(),
            // 保持与加入本项之前一致：默认跟随手选的 `theme_color`。
            theme_from_background: false,
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
