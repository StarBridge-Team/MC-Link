//! HMCL 主题包导入（B7）。
//!
//! # 最新版 HMCL（3.6.x）的主题形态
//!
//! 3.6 起主题分为内置预设与**自定义主题**两类，自定义主题放在
//! `<数据目录>/skins/<主题名>/`（目录），主题市场下载的则是同结构的**压缩包**（zip）。
//! 主题本身是 JavaFX CSS + 若干资源，**没有跨版本稳定的"强调色"字段**：
//! `skin.json` / `theme.json` / `theme.properties` / `theme.css` 在不同版本里都可能出现。
//!
//! 因此这里不按某个版本写死解析，而是**宽松提取**：优先取键名含
//! accent/primary/seed/color 的 `#RRGGBB`，否则取任意第一个非黑白颜色。
//! 目录与 zip 两种形态都支持——zip 只在内存里读文本，不落盘、不解压资源。
//!
//! 主题本身在客户端**不实现**（用户已定：主题走单独系统），这里只负责把 HMCL 主题的
//! 颜色翻译成本程序的种子色。

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 一个可导入的 HMCL 主题。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmclTheme {
    pub name: String,
    pub path: String,
    /// 解析出的种子色（`#rrggbb`）；解析不到时为 `None`。
    pub seed: Option<String>,
}

/// HMCL 候选数据目录。
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("HMCL"));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        dirs.push(PathBuf::from(home).join(".hmcl"));
    }
    dirs
}

/// 已存在的 `skins` 目录（主题根）。
fn skins_dirs() -> Vec<PathBuf> {
    data_dirs()
        .into_iter()
        .map(|d| d.join("skins"))
        .filter(|d| d.is_dir())
        .collect()
}

/// 是否是主题包文件（3.6.x 主题市场下载的 zip）。
fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

/// 列出 HMCL 已安装的主题（目录与 zip 都算）。
#[tauri::command]
pub(crate) fn hmcl_list_themes() -> Vec<HmclTheme> {
    let mut out = Vec::new();
    for skins in skins_dirs() {
        let Ok(entries) = std::fs::read_dir(&skins) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !is_zip(&path) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let seed = extract_seed(&path);
            out.push(HmclTheme {
                name,
                path: path.to_string_lossy().to_string(),
                seed,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 读取指定主题（目录或 zip）的种子色。
#[tauri::command]
pub(crate) fn hmcl_theme_seed(path: String) -> Result<Option<String>, String> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("主题不存在: {path}"));
    }
    Ok(extract_seed(&target))
}

/// 从主题（目录或 zip）提取种子色。
fn extract_seed(path: &Path) -> Option<String> {
    let mut preferred: Option<String> = None;
    let mut any: Option<String> = None;
    if path.is_file() {
        scan_zip(path, &mut preferred, &mut any);
    } else {
        scan_dir(path, 0, &mut preferred, &mut any);
    }
    preferred.or(any)
}

/// 递归扫描目录（限深），收集"偏好颜色"与"任意颜色"。
fn scan_dir(
    dir: &Path,
    depth: usize,
    preferred: &mut Option<String>,
    any: &mut Option<String>,
) {
    // 限深：主题目录通常只有一层资源，深挖只会让大目录拖慢扫描。
    if depth > 2 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, depth + 1, preferred, any);
            continue;
        }
        if !is_text_asset(&path) {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            scan_text(&text, preferred, any);
        }
    }
}

/// 读取 zip 内的文本资源（不落盘、不解压图片等无关条目）。
fn scan_zip(path: &Path, preferred: &mut Option<String>, any: &mut Option<String>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return;
    };
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        let name = entry.name().to_ascii_lowercase();
        if !has_text_ext(&name) {
            continue;
        }
        let mut text = String::new();
        if entry.read_to_string(&mut text).is_err() {
            continue;
        }
        scan_text(&text, preferred, any);
    }
}

/// 目录里哪些文件值得读（只看可能含颜色的文本资源）。
fn is_text_asset(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| has_text_ext(&format!(".{}", e.to_ascii_lowercase())))
        .unwrap_or(false)
}

fn has_text_ext(name: &str) -> bool {
    [".json", ".properties", ".css", ".txt", ".xml"]
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// 从一段文本里收集颜色。
fn scan_text(text: &str, preferred: &mut Option<String>, any: &mut Option<String>) {
    for line in text.lines() {
        let Some(color) = first_hex(line) else {
            continue;
        };
        if is_boring(&color) {
            continue;
        }
        if is_preferred_key(line) {
            if preferred.is_none() {
                *preferred = Some(color);
            }
        } else if any.is_none() {
            *any = Some(color);
        }
    }
}

/// 一行里是否含"像强调色"的键名。
fn is_preferred_key(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    ["accent", "primary", "seed", "color"]
        .iter()
        .any(|k| lower.contains(k))
}

/// 取一行里第一个 `#RRGGBB`（大小写不敏感），归一为小写。
///
/// 用 `get()` 而不是切片：`#` 之后可能紧跟多字节字符，直接切片会在非字符边界 panic。
fn first_hex(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'#' {
            continue;
        }
        if let Some(hex) = line.get(i + 1..i + 7) {
            if hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(format!("#{}", hex.to_ascii_lowercase()));
            }
        }
    }
    None
}

/// 纯黑/纯白作为种子没有意义（生成的调色板几乎是灰的），优先跳过。
fn is_boring(color: &str) -> bool {
    matches!(color, "#000000" | "#ffffff")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_theme_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("mclink-hmcl-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("创建测试目录");
        dir
    }

    #[test]
    fn prefers_accent_over_background() {
        let dir = temp_theme_dir("accent");
        std::fs::write(
            dir.join("theme.properties"),
            "background=#ffffff\naccent=#3b82f6\n",
        )
        .unwrap();
        assert_eq!(extract_seed(&dir), Some("#3b82f6".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_any_color_when_key_unknown() {
        let dir = temp_theme_dir("fallback");
        std::fs::write(dir.join("skin.json"), "{\"foo\": \"#a1b2c3\"}").unwrap();
        assert_eq!(extract_seed(&dir), Some("#a1b2c3".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_pure_black_and_white() {
        let dir = temp_theme_dir("boring");
        std::fs::write(dir.join("theme.css"), ".x { background: #ffffff; }").unwrap();
        assert_eq!(extract_seed(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hex_scan_survives_multibyte() {
        // `#` 后跟中文不应 panic（切片越界而不是字符边界）。
        assert_eq!(first_hex("颜色#abc中文"), None);
        assert_eq!(first_hex("accent: #12AB34"), Some("#12ab34".to_string()));
    }

    #[test]
    fn reads_colors_from_zip_pack() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("mclink-hmcl-{nanos}.zip"));

        // 构造一个最小主题包：zip 里放 theme.css。
        let file = std::fs::File::create(&path).expect("创建 zip");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        use std::io::Write;
        writer.start_file("theme.css", options).unwrap();
        writer.write_all(b".accent { -fx-accent: #ff8800; }").unwrap();
        writer.finish().unwrap();

        assert_eq!(extract_seed(&path), Some("#ff8800".to_string()));
        let _ = std::fs::remove_file(&path);
    }
}
