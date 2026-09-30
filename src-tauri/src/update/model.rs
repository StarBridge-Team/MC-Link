//! 更新清单的模型与**纯规则**（不做任何 I/O，便于离线单测）。
//!
//! # 清单长什么样
//!
//! ```json
//! {
//!   "manifest_version": 1,
//!   "version": "0.4.1",
//!   "release_date": "2026-09-30",
//!   "release_notes": "...",
//!   "mandatory": false,
//!   "assets": [
//!     { "platform": "windows-x86_64", "kind": "installer",
//!       "file": "MC-Link-0.4.1-windows-x64-setup.exe",
//!       "sha256": "...", "size": 12345678 },
//!     { "platform": "windows-x86_64", "kind": "portable",
//!       "file": "MC-Link-0.4.1-windows-x64-portable.exe",
//!       "sha256": "...", "size": 12345678 },
//!     { "platform": "windows-x86_64", "kind": "portable-zip",
//!       "file": "MC-Link-0.4.1-windows-x64-portable.zip", ... }
//!   ]
//! }
//! ```
//!
//! 同一份清单同时服务便携版与安装版：客户端按**当前平台 + 当前安装形态**挑一份资产，
//! 因此发布方不需要维护两套更新源。
//!
//! `urls` 留空时，客户端按自身资源服务器地址推导 `update/<file>`——
//! 这样清单里不必写死域名，换域名只改客户端一处。

use serde::{Deserialize, Serialize};

use crate::datadir::InstallMode;

/// 可自动落地的资产类型：安装版用安装器。
pub const KIND_INSTALLER: &str = "installer";
/// 可自动落地的资产类型：便携版用单文件 exe（直接替换自身）。
pub const KIND_PORTABLE: &str = "portable";
/// 仅用于手动下载的整包（便携 zip，含 `portable.txt`，供新用户首次下载）。
pub const KIND_PORTABLE_ZIP: &str = "portable-zip";

/// 本客户端支持的清单格式版本。
///
/// 清单里 `manifest_version` 更高时说明清单语义已经变了（例如换了选择规则），
/// 此时**必须拒绝**而不是猜着解析，否则可能出现"把安装版包推给便携版"这类破坏性误判。
pub const MANIFEST_VERSION: u32 = 1;

/// 资源服务器上的更新清单（`/update/latest.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    /// 清单格式版本，缺失视为 1。
    #[serde(default)]
    pub manifest_version: u32,
    /// 该版本的语义化版本号，形如 `0.4.1`。
    pub version: String,
    #[serde(default)]
    pub release_date: String,
    #[serde(default)]
    pub release_notes: String,
    /// 强制更新标记：界面应据此禁止"稍后再说"。
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub assets: Vec<UpdateAsset>,
}

/// 清单中的单个平台资产。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAsset {
    /// 平台标识，形如 `windows-x86_64`，与 [`crate::assets::adapter::current_platform`] 一致。
    pub platform: String,
    /// 资产类型，见本模块 `KIND_*` 常量。
    pub kind: String,
    /// 文件名：既作落地文件名，也作默认下载地址的末段。
    pub file: String,
    /// 候选下载地址，按顺序尝试；留空则按资源服务器地址推导。
    #[serde(default)]
    pub urls: Vec<String>,
    /// 小写十六进制 SHA256。**必填**：更新包会被直接执行，缺失即不允许自动安装。
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
}

/// 面向界面的更新信息。
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub release_notes: String,
    pub release_date: String,
    pub mandatory: bool,
    /// 与当前安装形态匹配、可自动落地的资产；`None` 表示只能手动下载。
    pub asset: Option<UpdateAsset>,
    /// 手动下载地址（便携 zip / 安装包），始终尽量给出。
    pub manual_url: Option<String>,
}

/// 检查更新的结果。
#[derive(Debug, Clone, Serialize)]
pub struct CheckUpdateResult {
    pub has_update: bool,
    pub current_version: String,
    /// `portable` | `installed`。
    pub install_mode: String,
    /// 形如 `windows-x86_64`。
    pub platform: String,
    pub latest: Option<UpdateInfo>,
}

/// 下载结果。
#[derive(Debug, Clone, Serialize)]
pub struct DownloadUpdateResult {
    pub success: bool,
    /// 落地文件名（缓存目录内）。
    pub file: String,
    pub size: u64,
    pub message: String,
}

/// 安装结果。
#[derive(Debug, Clone, Serialize)]
pub struct InstallUpdateResult {
    /// 恒为 true：落地流程已启动，本进程随即退出。
    pub restarting: bool,
    pub message: String,
}

/// 安装形态与自更新能力（界面据此显示"便携版/安装版"并决定按钮文案）。
#[derive(Debug, Clone, Serialize)]
pub struct InstallModeInfo {
    pub install_mode: String,
    pub auto_install_supported: bool,
    pub exe_path: String,
    pub data_dir: String,
}

/// 拒绝语义已变化的清单。
pub(crate) fn ensure_supported(manifest: &UpdateManifest) -> Result<(), String> {
    if manifest.manifest_version > MANIFEST_VERSION {
        return Err(format!(
            "资源服务器上的更新清单版本为 {}，高于本客户端支持的 {}；请手动下载新版本",
            manifest.manifest_version, MANIFEST_VERSION
        ));
    }
    Ok(())
}

/// 比较两个语义化版本字符串，仅支持 `x.y.z`（允许前缀 `v`，缺位按 0 补）。
pub(crate) fn version_greater(left: &str, right: &str) -> bool {
    let parse = |s: &str| {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect::<Vec<_>>()
    };
    let a = parse(left);
    let b = parse(right);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        if av != bv {
            return av > bv;
        }
    }
    false
}

/// 某种安装形态可自动落地的资产类型（按优先级）。
fn auto_kinds(mode: InstallMode) -> &'static [&'static str] {
    match mode {
        // 安装版必须由安装器覆盖系统目录并写注册表；直接替换 exe 会留下
        // "文件已更新但安装记录没变"的不一致状态，卸载/修复都会出错。
        InstallMode::Installed => &[KIND_INSTALLER],
        // 便携版反过来：目录里没有安装记录，替换 exe 就是全部。
        InstallMode::Portable => &[KIND_PORTABLE],
    }
}

/// 选出当前平台 + 安装形态下可自动落地的资产。
pub(crate) fn select_asset<'a>(
    manifest: &'a UpdateManifest,
    platform: &str,
    mode: InstallMode,
    auto_supported: bool,
) -> Option<&'a UpdateAsset> {
    if !auto_supported {
        return None;
    }
    auto_kinds(mode).iter().find_map(|kind| {
        manifest
            .assets
            .iter()
            .find(|a| a.platform == platform && a.kind == *kind)
    })
}

/// 选出手动下载用的资产：优先给"便携 zip"（新用户拿到的是可解压的整包），
/// 否则退回当前形态对应的包，最后退回当前平台的任意资产。
pub(crate) fn select_manual_asset<'a>(
    manifest: &'a UpdateManifest,
    platform: &str,
    mode: InstallMode,
) -> Option<&'a UpdateAsset> {
    let mut order: Vec<&str> = vec![KIND_PORTABLE_ZIP];
    order.extend_from_slice(auto_kinds(mode));
    order.push(KIND_PORTABLE);

    for kind in order {
        if let Some(a) = manifest
            .assets
            .iter()
            .find(|a| a.platform == platform && a.kind == kind)
        {
            return Some(a);
        }
    }
    manifest.assets.iter().find(|a| a.platform == platform)
}

/// 解析资产的候选下载地址。
///
/// 清单给了 `urls` 就用它（可挂镜像）；否则按当前资源服务器地址推导 `update/<file>`。
pub(crate) fn asset_urls(asset: &UpdateAsset, assets_base: &str) -> Vec<String> {
    if !asset.urls.is_empty() {
        return asset.urls.clone();
    }
    vec![crate::asset_server::join(
        assets_base,
        &format!("update/{}", asset.file),
    )]
}

/// 组装面向界面的更新信息。
pub(crate) fn build_info(
    manifest: &UpdateManifest,
    platform: &str,
    mode: InstallMode,
    auto_supported: bool,
    assets_base: &str,
) -> UpdateInfo {
    let manual_url = select_manual_asset(manifest, platform, mode)
        .and_then(|a| asset_urls(a, assets_base).into_iter().next());

    UpdateInfo {
        version: manifest.version.clone(),
        release_notes: manifest.release_notes.clone(),
        release_date: manifest.release_date.clone(),
        mandatory: manifest.mandatory,
        asset: select_asset(manifest, platform, mode, auto_supported).cloned(),
        manual_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(platform: &str, kind: &str, file: &str) -> UpdateAsset {
        UpdateAsset {
            platform: platform.to_string(),
            kind: kind.to_string(),
            file: file.to_string(),
            urls: vec![],
            sha256: "a".repeat(64),
            size: None,
        }
    }

    fn manifest(assets: Vec<UpdateAsset>) -> UpdateManifest {
        UpdateManifest {
            manifest_version: MANIFEST_VERSION,
            version: "0.4.1".to_string(),
            release_date: "2026-09-30".to_string(),
            release_notes: "notes".to_string(),
            mandatory: false,
            assets,
        }
    }

    #[test]
    fn version_compare_handles_prefix_and_length() {
        assert!(version_greater("v0.4.1", "v0.4.0"));
        assert!(version_greater("0.5", "0.4.9"));
        assert!(!version_greater("0.4.0", "0.4.0"));
        assert!(!version_greater("0.4.0", "v0.4.1"));
    }

    #[test]
    fn portable_mode_picks_portable_asset() {
        let m = manifest(vec![
            asset("windows-x86_64", KIND_INSTALLER, "setup.exe"),
            asset("windows-x86_64", KIND_PORTABLE, "portable.exe"),
        ]);
        let picked = select_asset(&m, "windows-x86_64", InstallMode::Portable, true).unwrap();
        assert_eq!(picked.kind, KIND_PORTABLE);
    }

    #[test]
    fn installed_mode_picks_installer_asset() {
        let m = manifest(vec![
            asset("windows-x86_64", KIND_PORTABLE, "portable.exe"),
            asset("windows-x86_64", KIND_INSTALLER, "setup.exe"),
        ]);
        let picked = select_asset(&m, "windows-x86_64", InstallMode::Installed, true).unwrap();
        assert_eq!(picked.kind, KIND_INSTALLER);
    }

    #[test]
    fn no_asset_for_other_platform_is_not_offered_for_auto_install() {
        let m = manifest(vec![asset("linux-x86_64", KIND_PORTABLE, "app.exe")]);
        assert!(select_asset(&m, "windows-x86_64", InstallMode::Portable, true).is_none());
        // 但仍应给出手动下载地址，避免用户卡在"没有可用更新"上
        assert!(select_manual_asset(&m, "windows-x86_64", InstallMode::Portable).is_none());
    }

    #[test]
    fn manual_download_prefers_portable_zip() {
        let m = manifest(vec![
            asset("windows-x86_64", KIND_PORTABLE, "portable.exe"),
            asset("windows-x86_64", KIND_PORTABLE_ZIP, "portable.zip"),
        ]);
        let picked = select_manual_asset(&m, "windows-x86_64", InstallMode::Portable).unwrap();
        assert_eq!(picked.kind, KIND_PORTABLE_ZIP);
    }

    #[test]
    fn unsupported_install_mode_disables_auto_install() {
        let m = manifest(vec![asset("windows-x86_64", KIND_INSTALLER, "setup.exe")]);
        assert!(select_asset(&m, "windows-x86_64", InstallMode::Installed, false).is_none());
        assert!(select_manual_asset(&m, "windows-x86_64", InstallMode::Installed).is_some());
    }

    #[test]
    fn urls_fall_back_to_asset_server_layout() {
        let a = asset("windows-x86_64", KIND_PORTABLE, "portable.exe");
        let urls = asset_urls(&a, "https://example.com/");
        assert_eq!(urls, vec!["https://example.com/update/portable.exe"]);

        let mut with_mirror = a.clone();
        with_mirror.urls = vec!["https://mirror/a".to_string()];
        assert_eq!(asset_urls(&with_mirror, "https://example.com"), vec!["https://mirror/a"]);
    }

    #[test]
    fn rejects_manifest_from_newer_format() {
        let mut m = manifest(vec![]);
        m.manifest_version = MANIFEST_VERSION + 1;
        assert!(ensure_supported(&m).is_err());

        m.manifest_version = MANIFEST_VERSION;
        assert!(ensure_supported(&m).is_ok());
    }
}
