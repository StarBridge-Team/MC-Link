//! 首次启动引导（OOBE）与本地化设置。
//!
//! # 为什么放在后端
//!
//! 语言与地区是**用户数据**（丢了要重选），而且必须在任何界面渲染之前就已知：
//! 前端要先知道"是否已完成引导"才能决定首个界面，也要先知道语言才能翻译。
//! 因此它存在 `Setting/setup.yml`，读写经 [`crate::persist`] 与 [`crate::mgr`] 的写锁，
//! 与其它用户数据遵循同一套约定。
//!
//! # 「用户的选择」与「系统检测值」是两件事
//!
//! 文件里只存**用户的选择**；用户没选过时字段为空，由系统语言推导出预选值。
//! 分开表达的好处：系统语言变化不会悄悄覆盖用户已经做出的选择，
//! 而"从未选择"也不必靠某个魔法默认值来暗示。
//!
//! # 新增语言的步骤
//!
//! 1. 在 [`SUPPORTED_LANGUAGES`] 里加语言标识；
//! 2. 在 `src/i18n/locales/` 下加同名文件并注册到 `src/i18n/index.ts`。
//!
//! 两处必须同时改：只加一边会出现"能选但没翻译"或"有翻译但选不到"。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::section_path;
use crate::persist;

/// 已提供翻译的语言。
pub const SUPPORTED_LANGUAGES: &[&str] = &["zh-CN", "en-US"];

/// 可选的地区（ISO 3166-1 alpha-2）。
///
/// [`REGION_OTHER`] 是显式的"未列出"，避免用户所在国家不在列表里就无路可选。
pub const SUPPORTED_REGIONS: &[&str] = &[
    "CN", "HK", "TW", "JP", "KR", "SG", "US", "GB", "DE", "FR", "AU", "CA", "BR", REGION_OTHER,
];

/// 未选择/未列出时的地区占位。
pub const REGION_OTHER: &str = "OTHER";

/// `Setting/setup.yml` 的内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SetupSettings {
    /// 是否已完成首次引导。
    #[serde(default)]
    pub completed: bool,
    /// 用户选择的界面语言；空表示"还没选"。
    #[serde(default)]
    pub language: String,
    /// 用户选择的地区；空表示"还没选"。
    #[serde(default)]
    pub region: String,
}

/// 面向界面的引导状态。
#[derive(Debug, Clone, Serialize)]
pub struct SetupState {
    /// 是否已完成引导；为 false 时前端应展示 OOBE。
    pub completed: bool,
    /// 当前**生效**的语言（用户选过就是用户选的，否则是系统检测值）。
    pub language: String,
    /// 当前生效的地区。
    pub region: String,
    /// 系统检测到的语言，供引导预选。
    pub detected_language: String,
    /// 系统检测到的地区，供引导预选。
    pub detected_region: String,
    /// 可选语言。界面**不要**自己硬编码这份清单。
    pub supported_languages: Vec<String>,
    /// 可选地区。
    pub supported_regions: Vec<String>,
}

/// 引导设置的落盘路径（`Setting/setup.yml`）。
pub(crate) fn setup_path(data_dir: &Path) -> Result<PathBuf, String> {
    section_path(data_dir, "setup")
}

/// 读取用户设置（文件缺失或损坏时由 persist 兜底为默认值）。
pub(crate) fn read(data_dir: &Path) -> Result<SetupSettings, String> {
    Ok(persist::load_yaml::<SetupSettings>(&setup_path(data_dir)?)?.value)
}

/// 写入用户设置。
pub(crate) fn write(data_dir: &Path, settings: &SetupSettings) -> Result<(), String> {
    persist::save_yaml(&setup_path(data_dir)?, settings)
}

/// 组装面向界面的状态。
pub(crate) fn state(data_dir: &Path) -> Result<SetupState, String> {
    let settings = read(data_dir)?;
    let detected = detect::locale();

    Ok(SetupState {
        completed: settings.completed,
        language: effective_language(&settings.language, &detected),
        region: effective_region(&settings.region, &detected),
        detected_language: detect::language_from_locale(&detected),
        detected_region: detect::region_from_locale(&detected),
        supported_languages: SUPPORTED_LANGUAGES.iter().map(|s| s.to_string()).collect(),
        supported_regions: SUPPORTED_REGIONS.iter().map(|s| s.to_string()).collect(),
    })
}

/// 保存语言与地区选择。
///
/// `completed` 为 `None` 表示**保持原有完成状态**（用于引导之后修改语言/地区）；
/// 传 `Some(true)` 即"完成引导"。
pub(crate) fn save_selection(
    data_dir: &Path,
    language: &str,
    region: &str,
    completed: Option<bool>,
) -> Result<SetupSettings, String> {
    let mut settings = read(data_dir)?;
    settings.language = normalize_language(language)?;
    settings.region = normalize_region(region)?;
    if let Some(completed) = completed {
        settings.completed = completed;
    }
    write(data_dir, &settings)?;
    Ok(settings)
}

/// 重置引导状态，让它下次启动重新走一遍（开发调试也常用）。
pub(crate) fn reset(data_dir: &Path) -> Result<SetupSettings, String> {
    let settings = SetupSettings::default();
    write(data_dir, &settings)?;
    Ok(settings)
}

/// 校验并规范化语言标识（大小写不敏感）。
pub(crate) fn normalize_language(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    SUPPORTED_LANGUAGES
        .iter()
        .find(|supported| supported.eq_ignore_ascii_case(trimmed))
        .map(|s| s.to_string())
        .ok_or_else(|| {
            format!(
                "不支持的语言「{}」；可选：{}",
                trimmed,
                SUPPORTED_LANGUAGES.join(", ")
            )
        })
}

/// 校验并规范化地区代码（统一大写）。
pub(crate) fn normalize_region(input: &str) -> Result<String, String> {
    let upper = input.trim().to_ascii_uppercase();
    SUPPORTED_REGIONS
        .iter()
        .find(|supported| **supported == upper)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            format!(
                "不支持的地区「{}」；可选：{}",
                input.trim(),
                SUPPORTED_REGIONS.join(", ")
            )
        })
}

/// 生效语言：用户选择优先；为空或已不再受支持时回退到系统检测值。
///
/// "已不再受支持"这个分支是必要的：将来若下掉某种语言，
/// 老用户文件里仍存着它，不能因此给出一个没有翻译的语言。
fn effective_language(chosen: &str, detected_locale: &str) -> String {
    normalize_language(chosen).unwrap_or_else(|_| detect::language_from_locale(detected_locale))
}

/// 生效地区：规则同 [`effective_language`]。
fn effective_region(chosen: &str, detected_locale: &str) -> String {
    normalize_region(chosen).unwrap_or_else(|_| detect::region_from_locale(detected_locale))
}

/// 系统语言/地区的读取与推导。
///
/// 推导部分是纯函数（可离线单测）；只有 [`detect::locale`] 会碰操作系统。
pub(crate) mod detect {
    use super::*;

    /// 读取系统语言标识（形如 `zh-CN`）；拿不到返回空串。
    pub(crate) fn locale() -> String {
        read_os_locale().unwrap_or_default()
    }

    /// 由系统标识推导可用的界面语言。
    ///
    /// 目前只有中英两种翻译：中文一律落到 `zh-CN`（将来加 `zh-TW` 时在此分流），
    /// 其余语言先落到 [`FALLBACK_LANGUAGE`]。
    pub(crate) fn language_from_locale(locale: &str) -> String {
        let lower = locale.trim().to_ascii_lowercase();
        if lower.starts_with("zh") {
            "zh-CN".to_string()
        } else {
            FALLBACK_LANGUAGE.to_string()
        }
    }

    /// 由系统标识推导地区；不认识的地区落到 [`REGION_OTHER`]。
    ///
    /// 支持 `zh-CN` / `en_US` / `ja-JP` 这类写法。
    pub(crate) fn region_from_locale(locale: &str) -> String {
        let mut parts = locale.split(['-', '_']);
        let _language = parts.next();
        match parts.next().map(str::trim).filter(|r| !r.is_empty()) {
            Some(region) => {
                let upper = region.to_ascii_uppercase();
                if SUPPORTED_REGIONS.contains(&upper.as_str()) {
                    upper
                } else {
                    REGION_OTHER.to_string()
                }
            }
            None => REGION_OTHER.to_string(),
        }
    }

    /// 检测不到可用语言时的兜底。
    pub(crate) const FALLBACK_LANGUAGE: &str = "en-US";

    /// Windows：读用户区域设置（形如 `zh-CN`）。
    #[cfg(windows)]
    fn read_os_locale() -> Option<String> {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Control Panel\\International")
            .ok()?;
        let name: String = key.get_value("LocaleName").ok()?;
        let name = name.trim().to_string();
        (!name.is_empty()).then_some(name)
    }

    /// 其它平台：读环境变量（形如 `zh_CN.UTF-8` → `zh_CN`）。
    #[cfg(not(windows))]
    fn read_os_locale() -> Option<String> {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(var) {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.split('.').next().unwrap_or(value).to_string());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mc-link-setup-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn language_derivation_maps_supported_and_falls_back() {
        assert_eq!(detect::language_from_locale("zh-CN"), "zh-CN");
        assert_eq!(detect::language_from_locale("zh_CN.UTF-8"), "zh-CN");
        assert_eq!(detect::language_from_locale("zh-TW"), "zh-CN");
        assert_eq!(detect::language_from_locale("en-US"), "en-US");
        // 未提供翻译的语言先落到英文，而不是给一个没有翻译的标识
        assert_eq!(detect::language_from_locale("ja-JP"), "en-US");
        assert_eq!(detect::language_from_locale(""), "en-US");
    }

    #[test]
    fn region_derivation_uses_locale_region_when_known() {
        assert_eq!(detect::region_from_locale("zh-CN"), "CN");
        assert_eq!(detect::region_from_locale("en_US"), "US");
        assert_eq!(detect::region_from_locale("ja-JP"), "JP");
        // 不在可选列表里的地区 → OTHER
        assert_eq!(detect::region_from_locale("en-VN"), REGION_OTHER);
        // 只有语言没有地区 → OTHER
        assert_eq!(detect::region_from_locale("en"), REGION_OTHER);
        assert_eq!(detect::region_from_locale(""), REGION_OTHER);
    }

    #[test]
    fn selection_validation_rejects_unknown_values() {
        assert_eq!(normalize_language("zh-cn").unwrap(), "zh-CN");
        assert_eq!(normalize_region("cn").unwrap(), "CN");
        assert!(normalize_language("fr-FR").is_err());
        assert!(normalize_region("XX").is_err());
        // 报错信息应列出可选值，方便排查
        assert!(normalize_region("XX").unwrap_err().contains("OTHER"));
    }

    #[test]
    fn user_choice_wins_over_detection() {
        assert_eq!(effective_language("en-US", "zh-CN"), "en-US");
        assert_eq!(effective_region("US", "zh-CN"), "US");
    }

    #[test]
    fn empty_or_stale_choice_falls_back_to_detection() {
        assert_eq!(effective_language("", "zh-CN"), "zh-CN");
        assert_eq!(effective_region("", "ja-JP"), "JP");
        // 曾经可选、如今已下掉的语言不能让界面拿到没有翻译的标识
        assert_eq!(effective_language("fr-FR", "zh-CN"), "zh-CN");
    }

    #[test]
    fn fresh_install_is_not_completed() {
        let dir = temp_dir("fresh");
        let state = state(&dir).unwrap();
        assert!(!state.completed, "全新安装不应被视为已完成引导");
        assert!(
            state.supported_languages.contains(&state.language),
            "生效语言必须在可选列表内: {}",
            state.language
        );
        assert!(state.supported_regions.contains(&state.region));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn completing_setup_persists_and_marks_completed() {
        let dir = temp_dir("complete");
        save_selection(&dir, "en-US", "US", Some(true)).unwrap();

        let state = state(&dir).unwrap();
        assert!(state.completed);
        assert_eq!(state.language, "en-US");
        assert_eq!(state.region, "US");

        // 重新读取（模拟重启）：值来自文件而不是检测
        let reloaded = read(&dir).unwrap();
        assert_eq!(reloaded.language, "en-US");
        assert_eq!(reloaded.region, "US");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn updating_after_oobe_keeps_completed_flag() {
        let dir = temp_dir("update-after");
        save_selection(&dir, "zh-CN", "CN", Some(true)).unwrap();
        // 引导之后在设置页改语言：完成状态必须保持不变
        save_selection(&dir, "en-US", "US", None).unwrap();

        let state = state(&dir).unwrap();
        assert!(state.completed, "改语言不应把用户打回 OOBE");
        assert_eq!(state.language, "en-US");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reset_returns_to_oobe() {
        let dir = temp_dir("reset");
        save_selection(&dir, "zh-CN", "CN", Some(true)).unwrap();
        reset(&dir).unwrap();

        let state = state(&dir).unwrap();
        assert!(!state.completed);
        assert_eq!(read(&dir).unwrap(), SetupSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_selection_is_rejected_without_touching_the_file() {
        let dir = temp_dir("invalid");
        save_selection(&dir, "zh-CN", "CN", Some(true)).unwrap();

        assert!(save_selection(&dir, "zh-CN", "XX", None).is_err());
        // 拒绝之后，原有设置必须原样保留
        let settings = read(&dir).unwrap();
        assert!(settings.completed);
        assert_eq!(settings.region, "CN");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
