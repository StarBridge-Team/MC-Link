use super::PersonalizationSettings;
use crate::datadir::{background_dir, old_background_dir};
use crate::mgr::AppMgr;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub(crate) struct BackgroundFile {
    pub(crate) name: String,
    pub(crate) is_video: bool,
}

#[tauri::command]
pub(crate) fn get_setting(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    section: String,
) -> Result<String, String> {
    mgr.read().setting(&section)
}

#[tauri::command]
pub(crate) fn save_setting(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    section: String,
    content: String,
) -> Result<(), String> {
    mgr.write().setting(&section, &content)
}

#[tauri::command]
pub(crate) fn get_personalization(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<PersonalizationSettings, String> {
    mgr.read().personalization()
}

#[tauri::command]
pub(crate) fn save_personalization(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    settings: PersonalizationSettings,
) -> Result<(), String> {
    mgr.write().personalization(settings)
}

#[tauri::command]
pub(crate) fn get_default_effect() -> String {
    crate::effect::get_default_effect()
}

#[tauri::command]
pub(crate) fn get_background_files(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<Vec<BackgroundFile>, String> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let bg_dir = background_dir(mgr.data_dir());
    read_bg_files(&bg_dir, &mut files, &mut seen);

    if let Some(old_bg) = old_background_dir() {
        if old_bg != bg_dir {
            read_bg_files(&old_bg, &mut files, &mut seen);
        }
    }

    Ok(files)
}

fn read_bg_files(
    bg_dir: &std::path::Path,
    files: &mut Vec<BackgroundFile>,
    seen: &mut std::collections::HashSet<String>,
) {
    if !bg_dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(bg_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    let is_video =
                        matches!(ext.as_str(), "mp4" | "webm" | "avi" | "mov" | "mkv" | "flv");
                    let is_image = matches!(
                        ext.as_str(),
                        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg"
                    );
                    if is_image || is_video {
                        if let Some(name) = path.file_name() {
                            let name = name.to_string_lossy().to_string();
                            if seen.insert(name.clone()) {
                                files.push(BackgroundFile { name, is_video });
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tauri::command]
pub(crate) fn get_background_file_url(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    filename: String,
) -> Result<String, String> {
    // 只接受纯文件名。这个命令的参数来自前端，而它会把路径回传给
    // `convertFileSrc` 去渲染（CSP 的 `asset:` 源对任意绝对路径都成立），
    // 所以不收口就等于开了一条任意文件读取通道：`../../Windows/win.ini`
    // 或 `C:\Windows\win.ini` 都能被回传出去。
    let name = sanitize_background_name(&filename)?;

    for dir in [Some(background_dir(mgr.data_dir())), old_background_dir()]
        .into_iter()
        .flatten()
    {
        let full = dir.join(&name);
        // canonicalize + 包含性校验：连"目录里放了指向别处的软链接"一起挡掉
        if let (Ok(real), Ok(base)) = (full.canonicalize(), dir.canonicalize()) {
            if real.is_file() && real.starts_with(&base) {
                return Ok(real.to_string_lossy().to_string());
            }
        }
    }

    Err(format!("文件不存在: {}", name))
}

/// 校验背景图文件名：必须是纯文件名（不含分隔符、盘符、`..`）。
fn sanitize_background_name(filename: &str) -> Result<String, String> {
    let name = filename.trim();
    let plain = std::path::Path::new(name)
        .file_name()
        .map(|f| f == name)
        .unwrap_or(false);
    let ok = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && plain;
    if !ok {
        return Err(format!("非法的背景文件名: {}", filename));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::sanitize_background_name;

    #[test]
    fn background_name_rejects_traversal_and_absolute_paths() {
        // 该参数来自前端，且会被回传给 convertFileSrc 渲染 —— 不收口就是任意文件读取
        assert!(sanitize_background_name("../../Windows/win.ini").is_err());
        assert!(sanitize_background_name(r"..\..\win.ini").is_err());
        assert!(sanitize_background_name(r"C:\Windows\win.ini").is_err());
        assert!(sanitize_background_name("/etc/passwd").is_err());
        assert!(sanitize_background_name("sub/dir/a.png").is_err());
        assert!(sanitize_background_name("").is_err());
        assert!(sanitize_background_name("   ").is_err());
        assert!(sanitize_background_name(".").is_err());
        assert!(sanitize_background_name("..").is_err());
    }

    #[test]
    fn background_name_accepts_plain_file_names() {
        assert_eq!(
            sanitize_background_name("背景图.png").unwrap(),
            "背景图.png"
        );
        assert_eq!(sanitize_background_name("  a.jpg  ").unwrap(), "a.jpg");
        assert_eq!(
            sanitize_background_name("b (1).webp").unwrap(),
            "b (1).webp"
        );
    }
}
