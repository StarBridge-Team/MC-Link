use std::sync::Arc;
use crate::datadir::{background_dir, old_background_dir};
use crate::mgr::AppMgr;
use super::PersonalizationSettings;
use serde::Serialize;

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

fn read_bg_files(bg_dir: &std::path::Path, files: &mut Vec<BackgroundFile>, seen: &mut std::collections::HashSet<String>) {
    if !bg_dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(bg_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    let is_video = matches!(ext.as_str(), "mp4" | "webm" | "avi" | "mov" | "mkv" | "flv");
                    let is_image = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg");
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
    let bg_dir = background_dir(mgr.data_dir());
    let full_path = bg_dir.join(&filename);
    if full_path.exists() {
        return Ok(full_path.to_string_lossy().to_string());
    }

    if let Some(old_bg) = old_background_dir() {
        let old_path = old_bg.join(&filename);
        if old_path.exists() {
            return Ok(old_path.to_string_lossy().to_string());
        }
    }

    Err(format!("文件不存在: {}", filename))
}
