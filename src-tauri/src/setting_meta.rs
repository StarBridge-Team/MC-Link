//! 设置项元配置（客户端侧）
//!
//! 从资产服务器拉取 `SettingMeta/<section>.yml` 内容并返回给前端。
//! 前端根据元配置渲染表单字段（label/desc/options 等），不再硬编码。

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::asset_server::{assets_server_url, join};
use crate::cache::{cache_path, ensure_cache_dir, is_cached};
use crate::mgr::AppMgr;

const META_CACHE_TTL: Duration = Duration::from_secs(3600);
const META_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

/// 元配置数据结构（与 assets-server `meta::SettingMeta` 对齐）。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettingMeta {
    pub section: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
    pub fields: Vec<FieldMeta>,
}

/// 设置项清单（列出所有可用的设置分区）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettingManifest {
    pub sections: Vec<SettingSection>,
}

/// 清单中的单个设置分区条目
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettingSection {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FieldMeta {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub options: Vec<FieldOption>,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default = "default_true")]
    pub auto_save: bool,
    #[serde(default)]
    pub group: String,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
}

/// 元配置缓存目录：Setting/MetaCache/
fn meta_cache_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("Cache").join("SettingMeta")
}

/// 元配置缓存文件路径
fn meta_cache_file(data_dir: &Path, section: &str) -> std::path::PathBuf {
    cache_path(&meta_cache_dir(data_dir), &format!("{}.yml", section))
}

/// 清单缓存文件路径
fn manifest_cache_file(data_dir: &Path) -> std::path::PathBuf {
    cache_path(&meta_cache_dir(data_dir), "manifest.json")
}

/// 拉取设置项清单：本地缓存未过期则直接返回，否则从资产服务器拉取。
pub async fn fetch_setting_manifest(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<SettingManifest, String> {
    let cache_dir = meta_cache_dir(data_dir);
    ensure_cache_dir(&cache_dir)?;
    let cache_file = manifest_cache_file(data_dir);

    if is_cached(&cache_file, Some(META_CACHE_TTL)) {
        let text = std::fs::read_to_string(&cache_file)
            .map_err(|e| format!("读取设置清单缓存失败: {}", e))?;
        return parse_manifest(&text);
    }

    let url = join(&assets_server_url(data_dir), "settings/manifest.json");
    let text = client
        .get(&url)
        .timeout(META_DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("拉取设置清单失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取设置清单失败: {}", e))?;

    std::fs::write(&cache_file, &text).map_err(|e| format!("保存设置清单缓存失败: {}", e))?;

    parse_manifest(&text)
}

fn parse_manifest(text: &str) -> Result<SettingManifest, String> {
    serde_json::from_str(text).map_err(|e| format!("解析设置清单失败: {}", e))
}

/// 拉取指定 section 的元配置：本地缓存未过期则直接返回，否则从资产服务器拉取。
pub async fn fetch_setting_meta(
    data_dir: &Path,
    section: &str,
    client: &reqwest::Client,
) -> Result<SettingMeta, String> {
    let cache_dir = meta_cache_dir(data_dir);
    ensure_cache_dir(&cache_dir)?;
    let safe_section = sanitize_section(section);
    if safe_section.is_empty() {
        return Err("无效的 section".to_string());
    }
    let cache_file = meta_cache_file(data_dir, &safe_section);

    if is_cached(&cache_file, Some(META_CACHE_TTL)) {
        let text = std::fs::read_to_string(&cache_file)
            .map_err(|e| format!("读取元配置缓存失败: {}", e))?;
        return parse_meta(&text);
    }

    let url = join(
        &assets_server_url(data_dir),
        &format!("settings/meta/{}", safe_section),
    );
    let text = client
        .get(&url)
        .timeout(META_DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("拉取元配置失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取元配置失败: {}", e))?;

    std::fs::write(&cache_file, &text).map_err(|e| format!("保存元配置缓存失败: {}", e))?;

    parse_meta(&text)
}

fn parse_meta(text: &str) -> Result<SettingMeta, String> {
    serde_yaml::from_str(text).map_err(|e| format!("解析元配置失败: {}", e))
}

fn sanitize_section(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

/// 清理所有元配置缓存
pub fn clear_setting_meta_cache(data_dir: &Path) -> Result<(), String> {
    let dir = meta_cache_dir(data_dir);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("清理元配置缓存失败: {}", e))
}

#[tauri::command]
pub(crate) async fn get_setting_meta(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    section: String,
) -> Result<SettingMeta, String> {
    mgr.pull().setting_meta(&section).await
}

#[tauri::command]
pub(crate) async fn get_setting_manifest(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<SettingManifest, String> {
    mgr.pull().setting_manifest().await
}

#[tauri::command]
pub(crate) fn clear_setting_meta_cache_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<(), String> {
    mgr.write().clear_setting_meta_cache()
}
