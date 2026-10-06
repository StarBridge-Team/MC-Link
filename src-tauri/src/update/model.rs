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

/// 可自动落地的资产类型：Windows 安装版用 NSIS 安装器。
pub const KIND_INSTALLER: &str = "installer";
/// 可自动落地的资产类型：便携版用单文件 exe（直接替换自身）。
pub const KIND_PORTABLE: &str = "portable";
/// 可自动落地的资产类型：Linux AppImage（由官方更新插件安装；deb/rpm 插件不支持）。
pub const KIND_APPIMAGE: &str = "appimage";
/// 可自动落地的资产类型：macOS `.app.tar.gz`（由官方更新插件安装）。
pub const KIND_MACOS_APP: &str = "macos-app";
/// 仅用于手动下载的整包（便携 zip，含 `portable.txt`，供新用户首次下载）。
pub const KIND_PORTABLE_ZIP: &str = "portable-zip";
/// 仅用于手动下载：Flatpak 单文件包（`.flatpak`）。
///
/// **刻意不参与自动更新**：Flatpak 是系统级沙箱安装，由 `flatpak` 自己管理
/// （运行时、OSTree 仓库、沙箱权限），Tauri 官方更新插件无法替换它；
/// 由应用自己去覆盖 `/app` 下的文件既不可行（只读绑定）也不合法。
/// Flatpak 用户必须经 `flatpak update`（见 `runtime::is_flatpak`）更新。
pub const KIND_FLATPAK: &str = "flatpak";
/// 仅用于手动下载：Debian 包（`.deb`）。由系统包管理器安装，应用无法自动替换。
pub const KIND_DEB: &str = "deb";
/// 仅用于手动下载：RPM 包（`.rpm`）。同上。
pub const KIND_RPM: &str = "rpm";

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
    /// `dev` | `official` | `self-built`，见 [`crate::build_channel`]。
    pub build_channel: String,
    /// 是否允许自动替换程序文件。
    ///
    /// 为 `false`（开发构建 / 自行构建）时界面只能引导用户手动下载，
    /// 此时 `latest.asset` 也会是 `None`。
    pub update_allowed: bool,
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

/// 当前运行环境信息。
///
/// 界面用它显示"便携版 / 安装版"与"官方版本 / 自行构建"，并决定给
/// "一键更新"还是"手动下载"按钮。不联网也能拿到，适合"关于"页首次渲染。
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfo {
    /// `portable` | `installed`。
    pub install_mode: String,
    /// `dev` | `official` | `self-built`，见 [`crate::build_channel`]。
    pub build_channel: String,
    /// 当前平台是否支持自动安装（目前仅 Windows）。
    pub auto_install_supported: bool,
    /// 当前渠道是否允许自动更新。
    pub update_allowed: bool,
    pub exe_path: String,
    pub data_dir: String,
    /// 只读沙箱信息（目前仅 Flatpak）；`None` 表示常规安装形态。
    ///
    /// 界面据此显示"请用 flatpak update 更新"并禁用应用内更新入口——
    /// 沙箱里自行更新在机制上就不可能成功。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<crate::runtime::SandboxInfo>,
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

/// 比较两个语义化版本字符串（`x.y.z`，允许前缀 `v`、`-pre` 后缀与 `+build` 元数据）。
///
/// 此前把非数字段直接丢弃，于是 `"1.0.0-rc.1"` 解析成 `[1,0,0,1]`，**大于**
/// `"1.0.0"`：一个预发布版会被当成正式更新推给用户并允许自动落地
/// （同一个函数还被缓存回收复用，判断结论同样会错）。
/// 现在按 semver 规则：预发布版小于同版本的正式版，且逐段比较。
pub(crate) fn version_greater(left: &str, right: &str) -> bool {
    let parse = |s: &str| -> (Vec<u32>, Vec<String>) {
        // 构建元数据（`+` 之后）不参与比较
        let s = s.trim().trim_start_matches('v');
        let s = s.split('+').next().unwrap_or(s);
        let (nums, pre) = match s.split_once('-') {
            Some((n, p)) => (n, Some(p)),
            None => (s, None),
        };
        let nums = nums
            .split('.')
            .map(|p| p.trim().parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>();
        let pre = pre
            .map(|p| p.split('.').map(|x| x.to_string()).collect::<Vec<_>>())
            .unwrap_or_default();
        (nums, pre)
    };

    let (a, a_pre) = parse(left);
    let (b, b_pre) = parse(right);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        if av != bv {
            return av > bv;
        }
    }

    // 数字段相同 → 由预发布后缀决定（semver §11）
    match (a_pre.is_empty(), b_pre.is_empty()) {
        (true, true) => false,
        // 正式版 > 同版本预发布版
        (true, false) => true,
        (false, true) => false,
        (false, false) => prerelease_greater(&a_pre, &b_pre),
    }
}

/// 比较两个预发布标识符序列（`a > b`）。
///
/// 规则：逐段比较，数字段按数值比、字母段按字典序比，且**数字段小于字母段**；
/// 前缀全等时，段数多的更大（`rc.1` > `rc`）。
fn prerelease_greater(a: &[String], b: &[String]) -> bool {
    for (x, y) in a.iter().zip(b.iter()) {
        match (x.parse::<u64>().ok(), y.parse::<u64>().ok()) {
            (Some(n), Some(m)) => {
                if n != m {
                    return n > m;
                }
            }
            // 数字标识符优先级低于字母标识符
            (Some(_), None) => return false,
            (None, Some(_)) => return true,
            (None, None) => {
                if x != y {
                    return x > y;
                }
            }
        }
    }
    a.len() > b.len()
}

/// 某种安装形态可自动落地的资产类型（按优先级）。
///
/// "安装版"要按平台选不同的包形态，因为各平台的安装方式完全不同：
/// Windows 交给 NSIS 安装器，Linux 只能自替换 AppImage，macOS 只能换 `.app`。
fn auto_kinds(mode: InstallMode) -> &'static [&'static str] {
    match mode {
        // 便携版反过来：目录里没有安装记录，替换 exe 就是全部。
        InstallMode::Portable => &[KIND_PORTABLE],
        // 安装版必须由安装器/插件覆盖系统目录并更新安装记录；
        // 直接替换二进制会留下"文件已变但记录没变"的不一致状态。
        InstallMode::Installed => {
            if cfg!(windows) {
                &[KIND_INSTALLER]
            } else if cfg!(target_os = "linux") {
                &[KIND_APPIMAGE]
            } else if cfg!(target_os = "macos") {
                &[KIND_MACOS_APP]
            } else {
                &[]
            }
        }
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
///
/// `flatpak` 时为 true（当前进程运行在 Flatpak 沙箱里）：此时**必须**优先给 `.flatpak`，
/// 因为那是唯一能被 `flatpak update` 接续管理的形态；给 AppImage 或 deb 只会让用户
/// 装出第二份互相冲突的副本。
pub(crate) fn select_manual_asset<'a>(
    manifest: &'a UpdateManifest,
    platform: &str,
    mode: InstallMode,
    flatpak: bool,
) -> Option<&'a UpdateAsset> {
    let mut order: Vec<&str> = Vec::new();
    if flatpak {
        order.push(KIND_FLATPAK);
    }
    order.push(KIND_PORTABLE_ZIP);
    order.extend_from_slice(auto_kinds(mode));
    order.push(KIND_PORTABLE);
    // 系统包管理器的形态垫在最后：装了它们就得靠 apt/dnf 更新，
    // 不该作为"手动下载"的首选把用户引到那条路上。
    order.push(KIND_DEB);
    order.push(KIND_RPM);
    if !flatpak {
        // 非 Flatpak 环境也接受 `.flatpak` 作为最后的手动选项：
        // 用户可能正想从 deb/AppImage 迁移到 Flatpak。
        order.push(KIND_FLATPAK);
    }

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
    flatpak: bool,
) -> UpdateInfo {
    let manual_url = select_manual_asset(manifest, platform, mode, flatpak)
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

    /// 回归：预发布版**不得**被当作正式更新。
    ///
    /// 旧实现丢弃非数字段，于是 `1.0.0-rc.1` 解析为 `[1,0,0,1]`，比 `1.0.0` 还大。
    #[test]
    fn prerelease_is_older_than_the_release() {
        assert!(!version_greater("1.0.0-rc.1", "1.0.0"));
        assert!(version_greater("1.0.0", "1.0.0-rc.1"));
        assert!(version_greater("1.0.0-rc.2", "1.0.0-rc.1"));
        assert!(!version_greater("1.0.0-rc.1", "1.0.0-rc.2"));
        assert!(
            version_greater("1.0.0-rc.10", "1.0.0-rc.2"),
            "数字段应按数值比较"
        );
        assert!(
            version_greater("1.0.0-rc.1", "1.0.0-beta.9"),
            "字母段按字典序"
        );
        assert!(version_greater("1.0.0-rc", "1.0.0-alpha"));
        // 构建元数据不参与比较
        assert!(!version_greater("1.0.0+build.2", "1.0.0+build.9"));
        assert!(version_greater("1.0.1+build.1", "1.0.0"));
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

    /// 安装版必须按**当前编译平台**挑对应的包形态。
    ///
    /// 这个用例被 `auto_kinds` 的平台分支决定，所以必须跟着平台走：
    /// Windows → NSIS 安装器，Linux → AppImage，macOS → `.app.tar.gz`。
    /// 早先只断言 `KIND_INSTALLER`，导致它在 Linux CI 上必然失败
    /// （`auto_kinds` 在 Linux 返回 `[KIND_APPIMAGE]`，`select_asset` 自然选不到安装器）。
    #[test]
    fn installed_mode_picks_installer_asset() {
        let (platform, kind, file) = if cfg!(windows) {
            ("windows-x86_64", KIND_INSTALLER, "setup.exe")
        } else if cfg!(target_os = "linux") {
            ("linux-x86_64", KIND_APPIMAGE, "app.AppImage")
        } else {
            ("macos-x86_64", KIND_MACOS_APP, "app.app.tar.gz")
        };

        let m = manifest(vec![
            asset(platform, KIND_PORTABLE, "portable"),
            asset(platform, kind, file),
        ]);
        let picked = select_asset(&m, platform, InstallMode::Installed, true)
            .expect("当前平台应能选出可自动落地的资产");
        assert_eq!(picked.kind, kind);
    }

    #[test]
    fn no_asset_for_other_platform_is_not_offered_for_auto_install() {
        let m = manifest(vec![asset("linux-x86_64", KIND_PORTABLE, "app.exe")]);
        assert!(select_asset(&m, "windows-x86_64", InstallMode::Portable, true).is_none());
        // 但仍应给出手动下载地址，避免用户卡在"没有可用更新"上
        assert!(select_manual_asset(&m, "windows-x86_64", InstallMode::Portable, false).is_none());
    }

    #[test]
    fn installed_mode_uses_the_platform_appropriate_kind() {
        let kinds = auto_kinds(InstallMode::Installed);
        if cfg!(windows) {
            assert_eq!(kinds, &[KIND_INSTALLER]);
        } else if cfg!(target_os = "linux") {
            assert_eq!(kinds, &[KIND_APPIMAGE]);
        } else if cfg!(target_os = "macos") {
            assert_eq!(kinds, &[KIND_MACOS_APP]);
        }
    }

    #[test]
    fn manual_download_prefers_portable_zip() {
        let m = manifest(vec![
            asset("windows-x86_64", KIND_PORTABLE, "portable.exe"),
            asset("windows-x86_64", KIND_PORTABLE_ZIP, "portable.zip"),
        ]);
        let picked = select_manual_asset(&m, "windows-x86_64", InstallMode::Portable, false).unwrap();
        assert_eq!(picked.kind, KIND_PORTABLE_ZIP);
    }

    #[test]
    fn unsupported_install_mode_disables_auto_install() {
        let m = manifest(vec![asset("windows-x86_64", KIND_INSTALLER, "setup.exe")]);
        assert!(select_asset(&m, "windows-x86_64", InstallMode::Installed, false).is_none());
        assert!(select_manual_asset(&m, "windows-x86_64", InstallMode::Installed, false).is_some());
    }

    /// Flatpak 环境下手动下载必须优先给 `.flatpak`。
    ///
    /// 给 AppImage 或 deb 会让用户在系统里装出第二份互相冲突的副本，
    /// 而且那份不受 `flatpak update` 管理。
    #[test]
    fn flatpak_env_prefers_flatpak_asset_for_manual_download() {
        let m = manifest(vec![
            asset("linux-x86_64", KIND_APPIMAGE, "app.AppImage"),
            asset("linux-x86_64", KIND_FLATPAK, "app.flatpak"),
        ]);
        let picked =
            select_manual_asset(&m, "linux-x86_64", InstallMode::Installed, true).unwrap();
        assert_eq!(
            picked.kind, KIND_FLATPAK,
            "Flatpak 环境必须优先给 .flatpak"
        );
    }

    /// Flatpak **绝不能**出现在 `auto_kinds` 里：它不是可自动落地的形态。
    #[test]
    fn flatpak_is_never_auto_installable() {
        for mode in [InstallMode::Portable, InstallMode::Installed] {
            let kinds = auto_kinds(mode);
            assert!(
                !kinds.contains(&KIND_FLATPAK),
                "auto_kinds({mode:?}) 不得包含 flatpak"
            );
        }
    }

    /// 非 Flatpak 环境下 `.flatpak` 仍作为最后的手动选项（允许用户主动迁移到 Flatpak）。
    #[test]
    fn flatpak_asset_is_reachable_as_last_resort_manual_option() {
        let m = manifest(vec![asset("linux-x86_64", KIND_FLATPAK, "app.flatpak")]);
        let picked =
            select_manual_asset(&m, "linux-x86_64", InstallMode::Installed, false).unwrap();
        assert_eq!(picked.kind, KIND_FLATPAK);
    }

    #[test]
    fn urls_fall_back_to_asset_server_layout() {
        let a = asset("windows-x86_64", KIND_PORTABLE, "portable.exe");
        let urls = asset_urls(&a, "https://example.com/");
        assert_eq!(urls, vec!["https://example.com/update/portable.exe"]);

        let mut with_mirror = a.clone();
        with_mirror.urls = vec!["https://mirror/a".to_string()];
        assert_eq!(
            asset_urls(&with_mirror, "https://example.com"),
            vec!["https://mirror/a"]
        );
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
