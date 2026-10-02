//! 共享资源（字体、图标等）的同步。
//!
//! # 设计要点
//!
//! 1. **清单驱动**：服务器 manifest 声明全部文件与各自 `sha256`，客户端不硬编码任何资源名。
//!    加一种新资源（比如将来换图标集）只需改服务器清单，客户端代码不动。
//! 2. **哈希校验**：只有内容哈希对得上才算"就绪"；下载回来的字节必须先校验通过再落盘。
//! 3. **不做破坏性清理**：旧实现在版本不一致时直接 `clear_cache(Assets/)`，而前端此刻可能
//!    正通过 `asset://` 引用里面的文件——那会导致"图标/字体凭空消失"。现在改为逐文件
//!    `persist::atomic_write` 覆盖落位，目标文件始终存在。
//! 4. **离线可用**：拉不到远程清单时回退到本地清单，只要哈希对得上仍然算就绪；
//!    离线状态下不会尝试下载，也不会用旧清单覆盖版本号。
//! 5. **陈旧文件回收**：等**全部**就绪之后再删除清单之外的文件
//!    （reconcile-after-success）。失败时不删任何东西，避免把可用的缓存删残。
//!
//! 注意：适配器包与更新包虽然也挂在服务器的 `Assets/` 下，但在客户端它们落在
//! `<data_dir>/Adapter`、`<data_dir>/Cache/Updates`，由各自的清单独立管理，
//! **不在本模块的回收范围内**。

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::asset_server::{assets_server_url, join};
use crate::datadir::assets_dir;
use crate::downloader::verify::sha256_file;
use crate::persist;

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// 单个资源的体积上限。
///
/// 字体/图标这类资源远小于它；这里的真正用处是掐掉"失陷或被劫持的资源服务器
/// 在 20 秒超时内灌满内存"这条路径（`downloader::verified` 有同量级的上限）。
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(8);
const LOCAL_MANIFEST_FILENAME: &str = "manifest.json";

/// 资产清单：服务器与客户端共享同一结构。
///
/// `version` 是全部被声明文件内容的聚合哈希，变了才需要重新取。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AssetsManifest {
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub assets: Vec<AssetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AssetEntry {
    pub path: String,
    #[serde(default)]
    pub size: Option<u64>,
    /// 小写十六进制 SHA256。缺失时**拒绝下载**该文件（fail-closed），
    /// 但对已存在的本地文件仍按"存在 + 大小"判定，以免旧清单把离线用户卡死。
    #[serde(default)]
    pub sha256: Option<String>,
}

/// 单个资源的就绪状态，回传给前端决定注入哪些 CSS。
#[derive(Debug, Clone, Serialize)]
pub struct AssetState {
    pub path: String,
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 同步结果。
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncOutcome {
    pub version: String,
    pub assets: Vec<AssetState>,
    /// 面向用户的失败原因（已去掉堆栈，可直接展示）。
    pub failures: Vec<String>,
    /// 是否处于离线/降级：未能拉到远程清单，仅用本地缓存判定。
    pub offline: bool,
}

impl SyncOutcome {
    /// 是否全部资源就绪。
    pub fn all_ready(&self) -> bool {
        !self.assets.is_empty() && self.assets.iter().all(|a| a.ready)
    }
}

fn local_manifest_path(data_dir: &Path) -> Result<PathBuf, String> {
    Ok(assets_dir(data_dir)?.join(LOCAL_MANIFEST_FILENAME))
}

fn read_local_manifest(data_dir: &Path) -> Option<AssetsManifest> {
    let path = local_manifest_path(data_dir).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_local_manifest(data_dir: &Path, manifest: &AssetsManifest) -> Result<(), String> {
    let path = local_manifest_path(data_dir)?;
    persist::save_json(&path, manifest)
}

/// 清单里的路径必须是不越界的相对路径。
///
/// 清单来自网络。若不校验就 `join`，被劫持或失陷的资源服务器可以让客户端把文件写到
/// `Assets/` 之外的任意位置（`../..`、绝对路径、Windows 前缀都能做到）。
/// 这里要求路径由**纯 Normal 组件**组成，`.`、`..`、根、盘符一律拒绝。
pub(crate) fn is_safe_relative(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return false;
    }
    p.components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn asset_path(data_dir: &Path, relative: &str) -> Result<PathBuf, String> {
    Ok(assets_dir(data_dir)?.join(relative))
}

/// 本地文件相对清单的状态。
enum LocalState {
    Ready,
    Missing,
    Mismatch(String),
}

async fn local_state(data_dir: &Path, entry: &AssetEntry) -> LocalState {
    let path = match asset_path(data_dir, &entry.path) {
        Ok(p) => p,
        Err(_) => return LocalState::Missing,
    };
    let meta = match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        _ => return LocalState::Missing,
    };

    if let Some(size) = entry.size {
        if meta.len() != size {
            return LocalState::Mismatch(format!("大小不符（期望 {}，实际 {}）", size, meta.len()));
        }
    }

    match entry.sha256.as_deref().map(str::trim) {
        Some(expected) if !expected.is_empty() => match sha256_file(&path).await {
            Ok(actual) if actual.eq_ignore_ascii_case(expected) => LocalState::Ready,
            Ok(actual) => LocalState::Mismatch(format!(
                "哈希不符（期望 {}…，实际 {}…）",
                &expected[..expected.len().min(8)],
                &actual[..actual.len().min(8)]
            )),
            Err(e) => LocalState::Mismatch(format!("无法计算哈希: {}", e)),
        },
        // 旧清单没有哈希：只能按存在 + 大小放行。
        _ => LocalState::Ready,
    }
}

async fn fetch_remote_manifest(
    base: &str,
    client: &reqwest::Client,
) -> Result<AssetsManifest, String> {
    let url = join(base, "manifest.json");
    let resp = client
        .get(&url)
        .timeout(MANIFEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("连接资源服务器失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("资源清单返回 {}", resp.status()));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取资源清单失败: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("解析资源清单失败: {}", e))
}

/// 同步共享资源。
///
/// 返回 [`SyncOutcome`] 描述每个资源的就绪情况，前端据此决定注入哪些 CSS、
/// 以及在降级时如何回退。
pub(crate) async fn sync_assets(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<SyncOutcome, String> {
    let base = assets_server_url(data_dir);
    sync_assets_from(data_dir, client, &base).await
}

/// `sync_assets` 的实现体：服务器地址显式传入。
///
/// 拆出这一层是为了可测试——否则"地址被写死在 `assets_server_url` 里"会让
/// 整条下载→校验→落盘链路无法在测试中驱动（见本文件测试模块里的假服务器）。
async fn sync_assets_from(
    data_dir: &Path,
    client: &reqwest::Client,
    base: &str,
) -> Result<SyncOutcome, String> {
    let (manifest, offline) = match fetch_remote_manifest(base, client).await {
        Ok(m) => (m, false),
        Err(e) => match read_local_manifest(data_dir) {
            // 离线降级：本地清单仍然可信（它自己就是上次校验通过的记录）
            Some(local) => {
                eprintln!("[assets] 拉取远程清单失败（{}），回退到本地清单", e);
                (local, true)
            }
            None => return Err(format!("无法获取资源清单，且本地无缓存: {}", e)),
        },
    };

    if manifest.assets.is_empty() {
        return Err("资源清单为空".to_string());
    }
    if let Some(bad) = manifest.assets.iter().find(|a| !is_safe_relative(&a.path)) {
        // 整份清单作废：宁可不更新，也不接受不可信的路径。
        return Err(format!("资源清单包含非法路径: {}", bad.path));
    }

    let mut states = Vec::with_capacity(manifest.assets.len());
    let mut pending: Vec<&AssetEntry> = Vec::new();
    for entry in &manifest.assets {
        match local_state(data_dir, entry).await {
            LocalState::Ready => states.push(AssetState {
                path: entry.path.clone(),
                ready: true,
                reason: None,
            }),
            LocalState::Missing => {
                states.push(AssetState {
                    path: entry.path.clone(),
                    ready: false,
                    reason: Some("本地缺失".to_string()),
                });
                pending.push(entry);
            }
            LocalState::Mismatch(why) => {
                states.push(AssetState {
                    path: entry.path.clone(),
                    ready: false,
                    reason: Some(why),
                });
                pending.push(entry);
            }
        }
    }

    if pending.is_empty() {
        if !offline {
            write_local_manifest(data_dir, &manifest)?;
        }
        return Ok(SyncOutcome {
            version: manifest.version,
            assets: states,
            failures: Vec::new(),
            offline,
        });
    }

    if offline {
        let failures = vec![format!(
            "{} 个资源未就绪，且当前无法连接资源服务器",
            pending.len()
        )];
        return Ok(SyncOutcome {
            version: manifest.version,
            assets: states,
            failures,
            offline: true,
        });
    }

    // 逐个补齐：下载 → 内存校验 → 覆盖落位。
    let mut failures = Vec::new();
    for entry in pending {
        match fetch_one(data_dir, client, base, entry).await {
            Ok(()) => {
                if let Some(slot) = states.iter_mut().find(|s| s.path == entry.path) {
                    slot.ready = true;
                    slot.reason = None;
                }
            }
            Err(e) => {
                let msg = format!("{}: {}", entry.path, e);
                if let Some(slot) = states.iter_mut().find(|s| s.path == entry.path) {
                    slot.reason = Some(e);
                }
                eprintln!("[assets] {}", msg);
                failures.push(msg);
            }
        }
    }

    // 只有全部就绪才写版本号、才回收陈旧文件：
    // 半成功状态下写版本号会让下次启动跳过补齐；此时删文件则可能删掉还需要的资源。
    if failures.is_empty() {
        write_local_manifest(data_dir, &manifest)?;
        if let Err(e) = prune_stale(data_dir, &manifest) {
            eprintln!("[assets] 清理陈旧资源失败（不影响本次同步）: {}", e);
        }
    }

    Ok(SyncOutcome {
        version: manifest.version,
        assets: states,
        failures,
        offline: false,
    })
}

/// 下载单个资源，校验通过后覆盖落位。
async fn fetch_one(
    data_dir: &Path,
    client: &reqwest::Client,
    base: &str,
    entry: &AssetEntry,
) -> Result<(), String> {
    let expected = entry.sha256.as_deref().map(str::trim).unwrap_or("");
    if expected.is_empty() {
        return Err("清单未提供 sha256，拒绝下载".to_string());
    }

    let url = join(base, &entry.path);
    let resp = client
        .get(&url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status()));
    }
    // 流式读取并在超限时立刻中止。
    //
    // 原先用 `resp.bytes()`：整个响应体先进内存，唯一的兜底是 20 秒总超时——
    // 高速镜像足以在这段时间里灌进 GB 级内存把客户端打崩。而清单与包同源，
    // 也就是说资源服务器一旦失陷或被劫持就能触发。
    let mut bytes: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取响应失败: {}", e))?;
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > MAX_ASSET_BYTES {
            return Err(format!(
                "资源 {} 超过体积上限 {} 字节，已中止",
                entry.path, MAX_ASSET_BYTES
            ));
        }
    }

    if let Some(size) = entry.size {
        if bytes.len() as u64 != size {
            return Err(format!("大小不符（期望 {}，实际 {}）", size, bytes.len()));
        }
    }

    // 先校验内存中的字节，再落盘：避免把校验不过的内容写进缓存。
    let actual = hex::encode(Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "哈希校验失败（期望 {}…，实际 {}…）",
            &expected[..expected.len().min(8)],
            &actual[..actual.len().min(8)]
        ));
    }

    let dest = asset_path(data_dir, &entry.path)?;
    persist::atomic_write(&dest, &bytes)
}

/// 删除清单之外的文件（只在全部就绪后调用）。
///
/// 只处理 `Assets/` 下清单声明的资源；`manifest.json` 是本地记录，保留。
fn prune_stale(data_dir: &Path, manifest: &AssetsManifest) -> Result<(), String> {
    let root = assets_dir(data_dir)?;
    if !root.is_dir() {
        return Ok(());
    }

    let keep: std::collections::HashSet<&str> =
        manifest.assets.iter().map(|a| a.path.as_str()).collect();

    let mut removed = 0usize;
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(&root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if relative == LOCAL_MANIFEST_FILENAME || keep.contains(relative.as_str()) {
                continue;
            }
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }

    // 顺手清掉空目录（自底向上，失败无所谓）
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path.clone());
                    dirs.push(path);
                }
            }
        }
    }
    for dir in dirs.into_iter().rev() {
        let _ = std::fs::remove_dir(&dir);
    }

    if removed > 0 {
        eprintln!("[assets] 已回收 {} 个陈旧资源文件", removed);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
