//! 代理池的候选来源与缓存。

use std::path::Path;
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 内置默认源。**永远排在最前面**：远端列表拉不到时应用仍能更新。
pub const BUILTIN: [&str; 3] = [
    "https://gh-proxy.com",
    "https://ghproxy.net",
    "https://ghfast.top",
];

const CACHE_FILE: &str = "proxy.json";
/// 列表一天一换：更新渠道失效通常以天计，更频繁地拉只是浪费请求。
const TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_PROXIES: usize = 12;
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

static ACTIVE: RwLock<Vec<String>> = RwLock::new(Vec::new());

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProxyList {
    #[serde(default)]
    proxies: Vec<String>,
}

/// 当前生效的候选（内置 + 远端去重后）。永不返回空。
pub fn snapshot() -> Vec<String> {
    let cached = ACTIVE.read().map(|list| list.clone()).unwrap_or_default();
    if cached.is_empty() {
        return BUILTIN.iter().map(|s| s.to_string()).collect();
    }
    cached
}

/// 刷新候选列表。启动时调用一次，失败不影响功能（沿用缓存或内置）。
///
/// 缓存命中时不发任何网络请求——这是"平时走缓存"的落点。
pub async fn refresh(data_dir: &Path, client: &reqwest::Client) {
    let dir = data_dir.join("Cache");
    let path = crate::cache::cache_path(&dir, CACHE_FILE);

    if crate::cache::is_cached(&path, Some(TTL)) {
        if let Some(list) = read_cache(&path) {
            install(list);
            return;
        }
    }

    match fetch_remote(data_dir, client).await {
        Ok(list) => {
            if crate::cache::ensure_cache_dir(&dir).is_ok() {
                if let Err(e) = crate::persist::save_json(&path, &ProxyList { proxies: list.clone() }) {
                    eprintln!("[代理] 写入缓存失败: {}", e);
                }
            }
            install(list);
        }
        Err(e) => {
            eprintln!("[代理] 拉取远端列表失败，沿用缓存/内置: {}", e);
            if let Some(list) = read_cache(&path) {
                install(list);
            }
        }
    }
}

fn install(remote: Vec<String>) {
    let merged = merge(&BUILTIN, &remote);
    if let Ok(mut slot) = ACTIVE.write() {
        *slot = merged;
    }
}

fn read_cache(path: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let parsed: ProxyList = serde_json::from_str(&text).ok()?;
    let list = sanitize(parsed.proxies);
    (!list.is_empty()).then_some(list)
}

async fn fetch_remote(data_dir: &Path, client: &reqwest::Client) -> Result<Vec<String>, String> {
    let url = crate::asset_server::join(
        &crate::asset_server::assets_server_url(data_dir),
        "proxy/list.json",
    );
    let response = client
        .get(&url)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("请求失败（{}）: {}", url, e))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}（{}）", response.status(), url));
    }
    let parsed: ProxyList = response
        .json()
        .await
        .map_err(|e| format!("解析列表失败: {}", e))?;
    let list = sanitize(parsed.proxies);
    if list.is_empty() {
        return Err("远端列表为空".to_string());
    }
    Ok(list)
}

/// 只接受 `https://` 前缀：代理会把原始链接拼在它后面，
/// `http://` 或 `javascript:` 之类的值会让下载走明文或直接失效。
fn sanitize(raw: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let value = item.trim().trim_end_matches('/');
        if !value.starts_with("https://") || value.len() <= "https://".len() {
            continue;
        }
        if !out.iter().any(|existing| existing == value) {
            out.push(value.to_string());
        }
        if out.len() >= MAX_PROXIES {
            break;
        }
    }
    out
}

fn merge(builtin: &[&str], remote: &[String]) -> Vec<String> {
    let mut out: Vec<String> = builtin
        .iter()
        .map(|s| s.trim_end_matches('/').to_string())
        .collect();
    for item in remote {
        if !out.iter().any(|existing| existing == item) {
            out.push(item.clone());
        }
        if out.len() >= MAX_PROXIES {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_always_present() {
        let merged = merge(&BUILTIN, &["https://extra.example".to_string()]);
        assert_eq!(merged[0], BUILTIN[0]);
        assert!(merged.contains(&"https://extra.example".to_string()));
        assert_eq!(merged.len(), BUILTIN.len() + 1);
    }

    #[test]
    fn merge_dedupes() {
        let merged = merge(&BUILTIN, &[BUILTIN[0].to_string()]);
        assert_eq!(merged.len(), BUILTIN.len());
    }

    #[test]
    fn sanitize_rejects_non_https_and_trims() {
        let list = sanitize(vec![
            "https://ok.example/".into(),
            "http://plain.example".into(),
            " https://ok.example ".into(),
            "https://".into(),
            String::new(),
        ]);
        assert_eq!(list, vec!["https://ok.example".to_string()]);
    }

    #[test]
    fn snapshot_never_empty() {
        assert!(!snapshot().is_empty());
    }
}
