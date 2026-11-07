//! 共享运行时（Bun）的定位、安装与版本锁定。
//!
//! # 为什么运行时路径由核心决定
//!
//! 如果清单能写 `runtime.host` 之类的字段，插件就能让核心启动**任意程序**，
//! `launcher::is_within` 那层"只能启动插件目录内的文件"的纵深防御会被整体绕过。
//! 因此运行时可执行文件的位置只有三个来源，全部在核心这边：
//!
//! 1. `<data_dir>/Runtime/bun.exe`（正式安装位置）；
//! 2. `MC_LINK_BUN` 环境变量（**仅开发/测试**用，核心会打日志提示）；
//! 3. 都没有 → 明确报错，由界面引导用户点"安装运行时"。
//!
//! 清单只能声明 `runtime.kind = "node"` 与 `runtime.entry`（必须是插件目录内的
//! `.js` / `.mjs` / `.cjs`）。
//!
//! # 版本策略
//!
//! 运行期**锁定**（[`BUN_PINNED_VERSION`]），构建期不锁。理由：Bun 是**我们提供给
//! 用户**的解释器，插件作者无法控制用户机器上的版本；上游 minor 版本确有行为变更，
//! 跟随最新会让"插件昨天能跑、今天挂了"且用户无法自救。
//!
//! 安装走现有 [`crate::downloader::verified`]：多镜像、4 MiB 分片并发、断点续传、
//! 强制 SHA256、`<data_dir>/Runtime/` 内原子落地。核心里**不出现第二套下载实现**。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::asset_server::{assets_server_url, join};

/// 运行时目录名（`<data_dir>/Runtime`）。
pub const RUNTIME_DIR: &str = "Runtime";

/// 核心锁定的 Bun 版本。
///
/// 升级是**核心发版行为**：改这里 + 更新资源服务器上的
/// `runtime/bun/<os>/manifest.json`（version / urls / sha256 / size）→
/// 对官方 Node 插件与协议测试向量跑一遍回归 → 随核心发布。
pub const BUN_PINNED_VERSION: &str = "1.3.14";

/// 开发/测试用的运行时覆盖变量。只读核心进程的环境，插件无法影响。
pub const DEV_OVERRIDE_ENV: &str = "MC_LINK_BUN";

/// 运行时状态文件名。
const STATE_FILE: &str = "bun.json";

/// 资源服务器上的运行时清单路径（按平台分目录，避免把别的平台的二进制发给你）。
fn manifest_url() -> String {
    let os = std::env::consts::OS;
    join(&assets_server_url(Path::new("")), &format!("runtime/bun/{os}/manifest.json"))
}

/// 可执行文件名（Windows 带 `.exe`）。
pub fn bun_file_name() -> &'static str {
    if cfg!(windows) {
        "bun.exe"
    } else {
        "bun"
    }
}

/// 运行时目录。
pub fn runtime_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(RUNTIME_DIR)
}

/// 正式安装位置。
pub fn bun_path(data_dir: &Path) -> PathBuf {
    runtime_dir(data_dir).join(bun_file_name())
}

fn state_path(data_dir: &Path) -> PathBuf {
    runtime_dir(data_dir).join(STATE_FILE)
}

/// 资源服务器上的运行时清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeManifest {
    /// 产物版本（如 `1.3.14`）。
    pub version: String,
    /// 候选下载地址，按顺序尝试。
    pub urls: Vec<String>,
    /// 小写十六进制 SHA256（**必填**，空值一律拒绝）。
    pub sha256: String,
    /// 文件大小，进度分母的兜底。
    #[serde(default)]
    pub size: Option<u64>,
}

/// 安装记录（`<data_dir>/Runtime/bun.json`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeState {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub sha256: String,
    /// 安装时间（毫秒时间戳）。
    #[serde(default)]
    pub installed_at: i64,
}

/// 面向界面的运行时状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    /// 运行时种类，固定 `node`。
    pub kind: &'static str,
    /// 是否已就绪（能直接拉起 Node 插件）。
    pub installed: bool,
    /// 实际可用的版本（未安装时为 `null`）。
    pub version: Option<String>,
    /// 可执行文件路径（未安装时为 `null`）。
    pub path: Option<String>,
    /// 核心锁定的版本。
    pub pinned_version: &'static str,
    /// 来源：`installed` / `dev-override` / `missing`。
    pub source: &'static str,
}

/// 读取安装记录（不存在或损坏时返回 `None`）。
pub fn installed_state(data_dir: &Path) -> Option<RuntimeState> {
    crate::persist::load_json::<RuntimeState>(&state_path(data_dir))
        .ok()
        .map(|loaded| loaded.value)
        .filter(|state| !state.version.trim().is_empty())
}

/// 开发覆盖：`MC_LINK_BUN` 指向一个真实存在的文件时才生效。
pub fn dev_override() -> Option<PathBuf> {
    let raw = std::env::var(DEV_OVERRIDE_ENV).ok()?;
    let path = PathBuf::from(raw.trim());
    path.is_file().then_some(path)
}

/// 解析出可用的 Bun 可执行文件。
///
/// 顺序：开发覆盖 → 正式安装位置。都不可用时返回**可操作**的错误文案
/// （界面据此引导用户安装，而不是显示"插件加载失败"）。
pub fn resolve(data_dir: &Path) -> Result<PathBuf, String> {
    if let Some(path) = dev_override() {
        eprintln!(
            "[插件] 使用 {} 覆盖共享运行时（仅开发用途）: {}",
            DEV_OVERRIDE_ENV,
            path.display()
        );
        return Ok(path);
    }
    let path = bun_path(data_dir);
    if path.is_file() {
        return Ok(path);
    }
    Err(format!(
        "共享 Node 运行时未安装（期望 {}，锁定版本 {}）。请在「设置 → 插件」里安装运行时",
        path.display(),
        BUN_PINNED_VERSION
    ))
}

/// 当前状态（不发网络请求）。
pub fn status(data_dir: &Path) -> RuntimeStatus {
    if let Some(path) = dev_override() {
        let state = installed_state(data_dir);
        return RuntimeStatus {
            kind: "node",
            installed: true,
            version: state.map(|s| s.version).or(None),
            path: Some(path.to_string_lossy().to_string()),
            pinned_version: BUN_PINNED_VERSION,
            source: "dev-override",
        };
    }
    let path = bun_path(data_dir);
    let state = installed_state(data_dir);
    match (path.is_file(), state) {
        (true, Some(state)) => RuntimeStatus {
            kind: "node",
            installed: true,
            version: Some(state.version),
            path: Some(path.to_string_lossy().to_string()),
            pinned_version: BUN_PINNED_VERSION,
            source: "installed",
        },
        (true, None) => RuntimeStatus {
            // 文件在但没记录：视为可用（可能是手工放的），但版本未知
            kind: "node",
            installed: true,
            version: None,
            path: Some(path.to_string_lossy().to_string()),
            pinned_version: BUN_PINNED_VERSION,
            source: "installed",
        },
        _ => RuntimeStatus {
            kind: "node",
            installed: false,
            version: None,
            path: None,
            pinned_version: BUN_PINNED_VERSION,
            source: "missing",
        },
    }
}

/// 拉取运行时清单。
pub async fn fetch_manifest() -> Result<RuntimeManifest, String> {
    let url = manifest_url();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("拉取运行时可下载清单失败（{url}）: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("运行时清单返回 {}（{url}）", resp.status()));
    }
    let manifest: RuntimeManifest = resp
        .json()
        .await
        .map_err(|e| format!("解析运行时清单失败: {e}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// 清单的完整性校验（纯函数，便于单测）。
///
/// `sha256` 必填：下载下来的 `bun` 会被核心直接执行，没有哈希就等于没有校验。
pub fn validate_manifest(manifest: &RuntimeManifest) -> Result<(), String> {
    if manifest.version.trim().is_empty() {
        return Err("运行时清单缺少 version".to_string());
    }
    if manifest.sha256.trim().is_empty() {
        return Err("运行时清单缺少 sha256，拒绝安装".to_string());
    }
    if manifest.urls.iter().all(|u| u.trim().is_empty()) {
        return Err("运行时清单没有可用的下载地址".to_string());
    }
    Ok(())
}

/// 安装（或复用）共享运行时，返回实际安装的版本。
///
/// 已安装且记录完好时直接返回，不重新下载。
pub async fn ensure(data_dir: &Path) -> Result<String, String> {
    if let (true, Some(state)) = (bun_path(data_dir).is_file(), installed_state(data_dir)) {
        return Ok(state.version);
    }

    let manifest = fetch_manifest().await?;
    if manifest.version != BUN_PINNED_VERSION {
        // 不阻断，但必须留痕：说明服务端与客户端锁定的版本漂移了
        eprintln!(
            "[插件] 运行时版本漂移：客户端锁定 {}，服务端发布 {}",
            BUN_PINNED_VERSION, manifest.version
        );
    }

    let dir = runtime_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建运行时目录失败: {e}"))?;

    let client = reqwest::Client::new();
    let remote = crate::downloader::verified::RemoteFile {
        file: bun_file_name(),
        urls: &manifest.urls,
        sha256: &manifest.sha256,
        size: manifest.size,
    };
    crate::downloader::verified::download_verified(&dir, &client, &remote, |_, _| {}).await?;

    let state = RuntimeState {
        version: manifest.version.clone(),
        sha256: manifest.sha256.to_ascii_lowercase(),
        installed_at: now_millis(),
    };
    crate::persist::save_json(&state_path(data_dir), &state)?;
    Ok(manifest.version)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "mclink-runtime-{}",
            crate::plugin::crypto::random_hex(6)
        ))
    }

    /// 未安装时必须是**可操作**的报错，而不是"未知错误"。
    #[test]
    fn resolve_fails_with_actionable_message_when_missing() {
        let dir = temp_dir();
        // 开发覆盖变量在测试进程里可能被外部设置，先确认它没生效
        std::env::remove_var(DEV_OVERRIDE_ENV);
        let err = resolve(&dir).unwrap_err();
        assert!(err.contains("未安装"), "错误文案要能引导用户安装: {err}");
        let status = status(&dir);
        assert!(!status.installed);
        assert_eq!(status.kind, "node");
        assert_eq!(status.pinned_version, BUN_PINNED_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installed_file_is_resolved_and_reported() {
        let dir = temp_dir();
        let runtime = runtime_dir(&dir);
        std::fs::create_dir_all(&runtime).unwrap();
        let exe = bun_path(&dir);
        std::fs::write(&exe, b"not a real bun").unwrap();
        std::fs::write(
            state_path(&dir),
            serde_json::json!({ "version": "1.3.14", "sha256": "ab", "installed_at": 1 })
                .to_string(),
        )
        .unwrap();

        std::env::remove_var(DEV_OVERRIDE_ENV);
        assert_eq!(resolve(&dir).unwrap(), exe);
        let status = status(&dir);
        assert!(status.installed);
        assert_eq!(status.version.as_deref(), Some("1.3.14"));
        assert_eq!(status.source, "installed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_requires_sha256_and_urls() {
        let good = RuntimeManifest {
            version: "1.3.14".into(),
            urls: vec!["https://example.invalid/bun.exe".into()],
            sha256: "ab".repeat(32),
            size: Some(1),
        };
        assert!(validate_manifest(&good).is_ok());

        let no_hash = RuntimeManifest {
            sha256: String::new(),
            ..good.clone()
        };
        assert!(validate_manifest(&no_hash).unwrap_err().contains("sha256"));

        let no_urls = RuntimeManifest {
            urls: vec![],
            ..good.clone()
        };
        assert!(validate_manifest(&no_urls).is_err());

        let no_version = RuntimeManifest {
            version: "  ".into(),
            ..good.clone()
        };
        assert!(validate_manifest(&no_version).is_err());

        // `sha256` 没有 `serde(default)`：缺字段时**解析期**就失败，不会走到校验
        assert!(serde_json::from_str::<RuntimeManifest>(r#"{"version":"1","urls":[]}"#).is_err());
    }
}
