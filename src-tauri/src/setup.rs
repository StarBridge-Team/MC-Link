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
    "CN",
    "HK",
    "TW",
    "JP",
    "KR",
    "SG",
    "US",
    "GB",
    "DE",
    "FR",
    "AU",
    "CA",
    "BR",
    REGION_OTHER,
];

/// 未选择/未列出时的地区占位。
pub const REGION_OTHER: &str = "OTHER";

/// EULA 同意记录。
///
/// **为什么由后端记而不是前端**:同意是一个法律事实，必须落盘、可追溯，
/// 且"是否同意过"不能由界面自己判断——界面只负责展示。
///
/// 同时存 `sha256`（**后端自己算的正文哈希**）而不只存版本号：
/// 只存版本号挡不住"版本号没变、内容被换掉"的情况。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EulaConsent {
    /// 条款版本（来自资源服务器清单）。
    #[serde(default)]
    pub version: String,
    /// 条款正文的 SHA256（小写十六进制）。
    #[serde(default)]
    pub sha256: String,
    /// 同意时间，RFC3339。
    #[serde(default)]
    pub accepted_at: String,
}

impl EulaConsent {
    /// 是否已经同意过（空时间戳即"没同意过"）。
    pub fn is_accepted(&self) -> bool {
        !self.accepted_at.trim().is_empty()
    }
}

/// OOBE 各步骤的就绪情况。
///
/// 刻意**不存"当前第几步"**：那会与三个字段构成两份状态源，必然漂移。
/// 由这些布尔值推导步骤，中断后再进来自然落在缺失的那一步。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetupSteps {
    pub language: bool,
    pub eula: bool,
    pub game: bool,
}

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
    /// 已同意的 EULA；`accepted_at` 为空表示尚未同意。
    #[serde(default)]
    pub eula: EulaConsent,
    /// 用户选择的第一个游戏 id（来自插件系统的游戏画像）；空表示尚未选择。
    #[serde(default)]
    pub game: String,
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
    /// 每一步是否就绪：界面据此决定从哪一步继续。
    pub steps: SetupSteps,
    /// EULA 同意记录；`None` 表示尚未同意。
    pub eula_accepted: Option<EulaConsent>,
    /// 已选择的游戏 id；空串表示未选。
    pub game: String,
    /// 本次运行是否处于"开发构建跳过引导"状态（界面可据此提示）。
    pub dev_skip: bool,
}

/// 当前 UTC 时间的 RFC3339 形式（形如 `2026-10-01T23:00:00Z`）。
///
/// 为什么自带而不引依赖：项目没有 chrono/time，而**法律记录里的时间必须人可读、
/// 且不随本机时区变化**——存 Unix 秒在审计时得再换算一次，很容易看错。
/// 这是 Howard Hinnant 的 civil_from_days 算法的最小实现，不含闰秒（审计足够）。
pub(crate) fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    rfc3339_from_unix(secs)
}

/// Unix 秒 → RFC3339（UTC）。
fn rfc3339_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 {
        yoe + era * 400 + 1
    } else {
        yoe + era * 400
    };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
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

/// 开发构建是否跳过 OOBE 并视为已同意 EULA。
///
/// 两处都要判断，缺一不可：
/// - `debug_assertions`：只有开发构建豁免，release 一律要走真流程；
/// - `test`：单元测试必须在 debug 下检验**真实**的引导逻辑，否则
///   "全新安装未完成引导"这类断言会被开发期豁免静默改成通过。
fn dev_skip_oobe() -> bool {
    cfg!(debug_assertions) && !cfg!(test)
}

/// 组装面向界面的状态。
pub(crate) fn state(data_dir: &Path) -> Result<SetupState, String> {
    let settings = read(data_dir)?;
    let detected = detect::locale();
    let dev_skip = dev_skip_oobe();
    let accepted = settings.eula.is_accepted();

    Ok(SetupState {
        completed: settings.completed || dev_skip,
        language: effective_language(&settings.language, &detected),
        region: effective_region(&settings.region, &detected),
        detected_language: detect::language_from_locale(&detected),
        detected_region: detect::region_from_locale(&detected),
        supported_languages: SUPPORTED_LANGUAGES.iter().map(|s| s.to_string()).collect(),
        supported_regions: SUPPORTED_REGIONS.iter().map(|s| s.to_string()).collect(),
        steps: SetupSteps {
            language: !settings.language.trim().is_empty(),
            // 开发构建视为已同意：否则每次清数据后都要手点一遍条款
            eula: accepted || dev_skip,
            game: !settings.game.trim().is_empty(),
        },
        eula_accepted: accepted.then(|| settings.eula.clone()),
        game: settings.game.clone(),
        dev_skip,
    })
}

/// 记录 EULA 同意。
///
/// `version` 与 `sha256` 由命令层在**核对服务端清单之后**传入；这里拒绝空值，
/// 避免写出一条"同意过、但不知道同意了什么"的记录。
pub(crate) fn accept_eula(
    data_dir: &Path,
    version: &str,
    sha256: &str,
    accepted_at: &str,
) -> Result<SetupSettings, String> {
    let version = version.trim();
    let sha256 = sha256.trim();
    let accepted_at = accepted_at.trim();
    if version.is_empty() || sha256.is_empty() || accepted_at.is_empty() {
        return Err("EULA 的版本/哈希/时间不完整，拒绝记录同意".into());
    }
    let mut settings = read(data_dir)?;
    settings.eula = EulaConsent {
        version: version.to_string(),
        sha256: sha256.to_string(),
        accepted_at: accepted_at.to_string(),
    };
    write(data_dir, &settings)?;
    Ok(settings)
}

/// 记录用户选择的第一个游戏。
///
/// 合法性（该 id 是否真实存在）由命令层对着插件系统校验——这里只负责落盘。
pub(crate) fn set_game(data_dir: &Path, game: &str) -> Result<SetupSettings, String> {
    let game = game.trim();
    if game.is_empty() {
        return Err("游戏 id 不能为空".into());
    }
    let mut settings = read(data_dir)?;
    settings.game = game.to_string();
    write(data_dir, &settings)?;
    Ok(settings)
}

/// 完成引导（**带闸门**）。
///
/// 必须同时满足三条：语言已选、**EULA 已同意**、首个游戏已选。任一不满足就报错
/// 且**不写 `completed`**——这样即便前端漏做某一步（或被绕过、被改），
/// 引导也完成不了。"同意"因此成为后端事实，而不是界面上的一个勾选框。
pub(crate) fn complete(
    data_dir: &Path,
    language: &str,
    region: &str,
) -> Result<SetupSettings, String> {
    let language = normalize_language(language)?;
    let region = normalize_region(region)?;

    let mut settings = read(data_dir)?;
    if !settings.eula.is_accepted() {
        return Err("尚未同意最终用户许可协议（EULA），无法完成引导".into());
    }
    if settings.game.trim().is_empty() {
        return Err("尚未选择首个游戏，无法完成引导".into());
    }

    settings.language = language;
    settings.region = region;
    settings.completed = true;
    write(data_dir, &settings)?;
    Ok(settings)
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
        // 语言子标签之后可能还有 **script**（`zh-Hans-CN` 里的 `Hans`）与地区：
        // 只看第二段会把 `Hans` 当成地区，于是 `zh-Hans-CN` 被判成"其它地区"，
        // 真实地区 `CN` 反而丢了。这里跳过 script，取第一个形状像地区的段。
        const SCRIPT_TAGS: &[&str] = &[
            "HANS", "HANT", "LATN", "CYRL", "ARAB", "GREK", "HEBR", "JPAN", "KORE", "DEVA", "THAI",
        ];
        for part in locale.split(['-', '_']).skip(1) {
            let upper = part.trim().to_ascii_uppercase();
            if upper.len() == 4 && SCRIPT_TAGS.contains(&upper.as_str()) {
                continue;
            }
            let looks_like_region = (upper.len() == 2
                && upper.chars().all(|c| c.is_ascii_alphabetic()))
                || (upper.len() == 3 && upper.chars().all(|c| c.is_ascii_digit()));
            if looks_like_region {
                return if SUPPORTED_REGIONS.contains(&upper.as_str()) {
                    upper
                } else {
                    REGION_OTHER.to_string()
                };
            }
        }
        REGION_OTHER.to_string()
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
        let dir =
            std::env::temp_dir().join(format!("mc-link-setup-{}-{}", tag, std::process::id()));
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

    /// 闸门一：没同意 EULA 不能完成引导。
    #[test]
    fn complete_requires_eula_consent() {
        let dir = temp_dir("gate-eula");
        save_selection(&dir, "zh-CN", "CN", None).unwrap();
        set_game(&dir, "minecraft-java").unwrap();

        let err = complete(&dir, "zh-CN", "CN").unwrap_err();
        assert!(err.contains("EULA"), "报错要指明缺的是哪一步: {err}");
        assert!(
            !read(&dir).unwrap().completed,
            "闸门未过时绝不能写入 completed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 闸门二：没选首个游戏不能完成引导。
    #[test]
    fn complete_requires_first_game() {
        let dir = temp_dir("gate-game");
        accept_eula(&dir, "1.0", "abc", "2026-10-01T00:00:00Z").unwrap();

        let err = complete(&dir, "zh-CN", "CN").unwrap_err();
        assert!(err.contains("游戏"), "报错要指明缺的是哪一步: {err}");
        assert!(!read(&dir).unwrap().completed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 三步齐备后完成，且同意记录持久化（模拟重启后仍在）。
    #[test]
    fn complete_succeeds_after_all_steps_and_persists_consent() {
        let dir = temp_dir("gate-ok");
        save_selection(&dir, "zh-CN", "CN", None).unwrap();
        set_game(&dir, "minecraft-java").unwrap();
        accept_eula(&dir, "1.0", "deadbeef", "2026-10-01T12:00:00Z").unwrap();

        complete(&dir, "en-US", "US").unwrap();

        let reloaded = read(&dir).unwrap();
        assert!(reloaded.completed);
        assert_eq!(reloaded.language, "en-US");
        assert_eq!(reloaded.game, "minecraft-java");
        assert_eq!(reloaded.eula.version, "1.0");
        assert_eq!(reloaded.eula.sha256, "deadbeef");
        assert!(reloaded.eula.is_accepted());

        let state = state(&dir).unwrap();
        assert!(state.steps.language && state.steps.eula && state.steps.game);
        assert!(!state.dev_skip, "测试环境不得启用开发期豁免");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 残缺的同意记录必须被拒绝：不能写出"同意过但不知道同意了什么"。
    #[test]
    fn accept_eula_rejects_incomplete_records() {
        let dir = temp_dir("eula-empty");
        assert!(accept_eula(&dir, "", "abc", "2026-10-01T00:00:00Z").is_err());
        assert!(accept_eula(&dir, "1.0", "   ", "2026-10-01T00:00:00Z").is_err());
        assert!(accept_eula(&dir, "1.0", "abc", "").is_err());
        assert!(!read(&dir).unwrap().eula.is_accepted());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 重置引导必须连同意记录与游戏选择一起清掉，否则"重新走一遍"是假的。
    #[test]
    fn reset_clears_eula_and_game() {
        let dir = temp_dir("reset-eula");
        accept_eula(&dir, "1.0", "abc", "2026-10-01T00:00:00Z").unwrap();
        set_game(&dir, "minecraft-java").unwrap();
        reset(&dir).unwrap();

        let settings = read(&dir).unwrap();
        assert!(!settings.eula.is_accepted());
        assert!(settings.game.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 时间戳必须是 UTC 且格式正确（法律记录不随本机时区变化）。
    #[test]
    fn rfc3339_formatting_is_utc() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix(1_000_000_000), "2001-09-09T01:46:40Z");
        // 闰日
        assert_eq!(rfc3339_from_unix(1_582_934_400), "2020-02-29T00:00:00Z");
        // 2100-01-01 的前一秒（能被 100 整除但不能被 400 整除的年份不是闰年）
        assert_eq!(rfc3339_from_unix(4_102_444_799), "2099-12-31T23:59:59Z");
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
