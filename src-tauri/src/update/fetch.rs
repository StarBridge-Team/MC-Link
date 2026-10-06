//! 更新清单的拉取与"是否需要更新"的判断。
//!
//! 清单**不做本地缓存**：更新信息的价值在于"当下是否是最新"，缓存只会让新版本
//! 延迟可见，而清单本身只有几 KB。
//!
//! # 拉取顺序（三级，逐级回退）
//!
//! 1. **GitHub 发行版**（直连）—— 最快，也最省资源服务器的带宽；
//! 2. **GitHub 代理加速**（并发探测，选延迟最小的）—— 国内直连不通时的常规路径。
//!    代理是"前缀型"：把原始链接直接拼在前缀之后，因此探测与下载走同一条链路；
//! 3. **自有资源服务器**（兜底）—— 前两者都不可达时才用，保证极端网络下仍能更新。
//!
//! 同一组择优结果不仅用于**清单**，也用于**更新包**：[`release_asset_urls`] 用完全
//! 相同的探测为 [`super::download`] 产出候选地址，下载器按序回退并保留续传进度。
//!
//! 探测用 HEAD 而非 GET：不清点正文，只比"谁先答话"。所有候选并发发起，
//! 直连可达就优先（首选源能直连就不绕第三方），其余按延迟升序。

use std::path::Path;
use std::time::{Duration, Instant};

use crate::asset_server::{assets_server_url, join};
use crate::build_channel::BuildChannel;
use crate::datadir::install_mode;

use super::install::is_auto_install_supported;
use super::model::{
    build_info, ensure_supported, version_greater, CheckUpdateResult, UpdateManifest,
};

/// 单次清单拉取超时（每一级各自计时）。
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(10);

/// 代理探测（HEAD）超时。
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// GitHub 仓库（`owner/repo`），用于拼发行版资源直链。
const GITHUB_REPO: &str = "StarBridge-Team/MC-Link";

/// 发行版里更新清单的文件名（由发布脚本生成并挂在 GitHub Release 上）。
const GITHUB_MANIFEST_FILE: &str = "latest.json";

/// 资源服务器上的兜底清单路径。
const ASSETS_SERVER_MANIFEST: &str = "update/latest.json";

/// 前缀型 GitHub 代理候选。全部并发探测，取**延迟最小**的那个。
///
/// 之所以放多个：单一代理随时可能失效或被限流，而用户无从察觉"更新检查失败"
/// 是因为某个第三方站点挂了。多候选 + 延迟择优能把这类故障自动绕开。
const GH_PROXIES: &[&str] = &[
    "https://gh-proxy.com",
    "https://ghproxy.net",
    "https://ghfast.top",
    "https://ghproxy.homeboyc.cn",
];

/// GitHub 发行版清单的原始（直连）地址。
fn github_manifest_url() -> String {
    format!(
        "https://github.com/{}/releases/latest/download/{}",
        GITHUB_REPO, GITHUB_MANIFEST_FILE
    )
}

/// GitHub 发行版某个产物文件的直链（更新包用，按版本锁定，不走 latest）。
///
/// 清单里的资产文件名带版本号（`MC-Link-<ver>-<platform>-<kind>.<ext>`），
/// 与挂在 Release 上的文件名一致，因此能直接拼出下载地址。
pub(crate) fn github_release_url(version: &str, file: &str) -> String {
    format!(
        "https://github.com/{}/releases/download/v{}/{}",
        GITHUB_REPO, version, file
    )
}

/// 把原始地址套上前缀型代理。
fn proxied_url(proxy: &str, origin: &str) -> String {
    format!("{}/{}", proxy.trim_end_matches('/'), origin)
}

/// 资源服务器上的兜底清单地址。
fn assets_server_manifest_url(data_dir: &Path) -> String {
    join(&assets_server_url(data_dir), ASSETS_SERVER_MANIFEST)
}

/// 从指定地址拉取并解析清单（一级的完整动作）。
async fn fetch_from(url: &str, client: &reqwest::Client) -> Result<UpdateManifest, String> {
    let response = client
        .get(url)
        .timeout(MANIFEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("请求失败（{}）: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}（{}）", response.status(), url));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败（{}）: {}", url, e))?;

    let manifest: UpdateManifest =
        serde_json::from_str(&text).map_err(|e| format!("解析清单失败（{}）: {}", url, e))?;

    ensure_supported(&manifest)?;
    Ok(manifest)
}

/// 探测一个地址是否可达；可达时返回耗时（HEAD 请求）。
async fn probe(client: &reqwest::Client, url: &str) -> Option<Duration> {
    let started = Instant::now();
    client
        .head(url)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map(|r| r.status().is_success() || r.status().is_redirection())
        .unwrap_or(false)
        .then(|| started.elapsed())
}

/// 并发探测 GitHub 直连与所有代理，产出**有序**候选地址：
///
/// - 直连可达就排第一——即便它不是延迟最低的：GitHub 发行版是首选源，
///   能直连就不该绕第三方；
/// - 代理按探测延迟升序（多个优选节点先试延迟最小的那个）；
/// - 全部不可达时返回空列表，由调用方走自己的兜底。
///
/// 探测目标与后续下载目标是同一个 URL，耗时可以直接当作下载速度的近似。
/// 404 也判为不可达：文件都没挂上去的渠道不该参与择优。
async fn ordered_channels(client: &reqwest::Client, origin: &str) -> Vec<String> {
    let direct_url = origin.to_string();
    let mut tasks = Vec::with_capacity(GH_PROXIES.len());

    for &proxy in GH_PROXIES {
        let url = proxied_url(proxy, origin);
        let client = client.clone();
        tasks.push(tokio::spawn(async move {
            probe(&client, &url).await.map(|elapsed| (elapsed, url))
        }));
    }

    // 直连与代理并发探测：不额外串行一个探测超时
    let direct_client = client.clone();
    let probe_direct = tokio::spawn(async move {
        probe(&direct_client, &direct_url)
            .await
            .map(|_| direct_url.clone())
    });

    let mut proxies: Vec<(Duration, String)> = Vec::new();
    for task in tasks {
        // 探测任务自身带超时；panic（JoinErr）视作不可达
        if let Ok(Some(item)) = task.await {
            proxies.push(item);
        }
    }
    proxies.sort_by_key(|(elapsed, _)| *elapsed);

    let mut out = Vec::with_capacity(proxies.len() + 1);
    if let Ok(Some(url)) = probe_direct.await {
        out.push(url);
    }
    out.extend(proxies.into_iter().map(|(_, url)| url));
    out
}

/// 更新包的候选下载地址（GitHub 直连 + 代理择优，按可用性排序）。
///
/// 文件名里解析不出版本号时返回空列表——拼不出发行版链接，
/// 调用方据此只用清单自带的地址。
pub(crate) async fn release_asset_urls(client: &reqwest::Client, file: &str) -> Vec<String> {
    let Some(version) = super::download::package_version(file) else {
        return Vec::new();
    };
    ordered_channels(client, &github_release_url(&version, file)).await
}

/// 拉取更新清单：GitHub 直连 → 代理择优 → 资源服务器兜底。
pub(crate) async fn fetch_manifest(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<UpdateManifest, String> {
    let mut errors: Vec<String> = Vec::new();

    // 第一、二级：GitHub 直连（可达则排首）+ 代理按延迟升序
    for url in ordered_channels(client, &github_manifest_url()).await {
        match fetch_from(&url, client).await {
            Ok(manifest) => {
                eprintln!("[更新] 清单来源: {}", url);
                return Ok(manifest);
            }
            Err(e) => {
                eprintln!("[更新] 渠道失败（{}）: {}", url, e);
                errors.push(e);
            }
        }
    }

    // 第三级：自有资源服务器兜底
    let fallback = assets_server_manifest_url(data_dir);
    match fetch_from(&fallback, client).await {
        Ok(manifest) => {
            eprintln!("[更新] 清单来自资源服务器（兜底）");
            return Ok(manifest);
        }
        Err(e) => {
            eprintln!("[更新] 资源服务器失败: {}", e);
            errors.push(format!("资源服务器: {}", e));
        }
    }

    Err(format!(
        "无法获取更新清单，所有渠道均失败：{}",
        errors.join("；")
    ))
}

/// 检查更新：判定渠道 → 拉清单 → 比版本 → 按平台与安装形态选出资产。
///
/// # 渠道闸门
///
/// 只有**官方发布构建**允许自动更新（见 [`crate::build_channel`]）：
///
/// - **开发构建**：直接返回"无可用更新"且**不发起任何网络请求**。它随时在变，
///   "是否是最新版本"对它没有意义，跳过请求也免得开发者每次打开关于页都打一次网络。
///   调试更新流程本身时可用 `MC_LINK_UPDATE_OVERRIDE=1` 强制开启。
/// - **自行构建**：照常检查并告知"官方已发布新版本"，但 `asset` 恒为 `None`，
///   只给手动下载链接——绝不静默替换别人的构建成果。
pub(crate) async fn check_update(
    data_dir: &Path,
    client: &reqwest::Client,
) -> Result<CheckUpdateResult, String> {
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let channel = crate::build_channel::channel();
    let allowed = crate::build_channel::update_allowed();

    let platform = crate::assets::adapter::current_platform();
    let mode = install_mode();

    if channel == BuildChannel::Dev && !allowed {
        return Ok(CheckUpdateResult {
            has_update: false,
            current_version,
            install_mode: mode.as_str().to_string(),
            platform,
            build_channel: channel.as_str().to_string(),
            update_allowed: false,
            latest: None,
        });
    }

    let manifest = fetch_manifest(data_dir, client).await?;
    let has_update = version_greater(&manifest.version, &current_version);

    // 渠道不允许自更新时，连同"平台是否支持"一起收敛为 false：
    // 选不到资产，界面就只能走手动下载这条路
    let auto_supported = is_auto_install_supported() && allowed;

    Ok(CheckUpdateResult {
        has_update,
        current_version,
        install_mode: mode.as_str().to_string(),
        platform: platform.clone(),
        build_channel: channel.as_str().to_string(),
        update_allowed: allowed,
        latest: if has_update {
            Some(build_info(
                &manifest,
                &platform,
                mode,
                auto_supported,
                &assets_server_url(data_dir),
                crate::runtime::is_flatpak(),
            ))
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 发行版直链必须按 tag 锁定版本：清单版本 `0.5.1` ↔ tag `v0.5.1`。
    /// 拼错前缀或漏掉 `v`，所有 GitHub 渠道都会 404 并静默回退到资源服务器。
    #[test]
    fn release_url_pins_tag_and_file() {
        assert_eq!(
            github_release_url("0.5.1", "MC-Link-0.5.1-windows-x86_64-portable.exe"),
            "https://github.com/StarBridge-Team/MC-Link/releases/download/v0.5.1/\
             MC-Link-0.5.1-windows-x86_64-portable.exe"
        );
    }

    /// 前缀型代理就是"原始链接直接拼在前缀之后"，多斜杠要收敛掉。
    #[test]
    fn proxy_is_pure_prefix() {
        let origin = "https://github.com/o/r/releases/download/v1/a.exe";
        assert_eq!(
            proxied_url("https://gh-proxy.com/", origin),
            format!("https://gh-proxy.com/{}", origin)
        );
        assert_eq!(
            proxied_url("https://ghproxy.net", origin),
            format!("https://ghproxy.net/{}", origin)
        );
    }

    /// 清单直链走 `releases/latest`：与发布流程挂载的文件名必须一致。
    #[test]
    fn manifest_uses_latest_download() {
        assert_eq!(
            github_manifest_url(),
            "https://github.com/StarBridge-Team/MC-Link/releases/latest/download/latest.json"
        );
    }
}
