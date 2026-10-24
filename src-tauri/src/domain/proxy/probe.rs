//! 候选地址的测速与排序。
//!
//! 探测用 HEAD 而非 GET：不清点正文，只比"谁先答话"。所有候选并发发起，
//! **直连可达就排第一**（首选源能直连就不该绕第三方），代理按延迟升序。
//!
//! 探测目标与后续下载目标是同一个 URL，所以耗时可以直接当作下载速度的近似。

use std::time::{Duration, Instant};

use super::pool;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// 把原始地址套上前缀型代理。
pub fn proxied_url(proxy: &str, origin: &str) -> String {
    format!("{}/{}", proxy.trim_end_matches('/'), origin)
}

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

/// 产出**有序**候选地址。全部不可达时返回空列表，由调用方走自己的兜底。
///
/// 404 也判为不可达：文件都没挂上去的渠道不该参与择优。
pub async fn ordered_channels(client: &reqwest::Client, origin: &str) -> Vec<String> {
    let direct_url = origin.to_string();
    let proxies = pool::snapshot();

    let mut tasks = Vec::with_capacity(proxies.len());
    for proxy in proxies {
        let url = proxied_url(&proxy, origin);
        let client = client.clone();
        tasks.push(tokio::spawn(async move {
            probe(&client, &url).await.map(|elapsed| (elapsed, url))
        }));
    }

    let direct_client = client.clone();
    let direct_task = tokio::spawn(async move {
        probe(&direct_client, &direct_url)
            .await
            .map(|_| direct_url.clone())
    });

    let mut ranked: Vec<(Duration, String)> = Vec::new();
    for task in tasks {
        // 探测任务自带超时；panic（JoinErr）视作不可达。
        if let Ok(Some(item)) = task.await {
            ranked.push(item);
        }
    }
    ranked.sort_by_key(|(elapsed, _)| *elapsed);

    let mut out = Vec::with_capacity(ranked.len() + 1);
    if let Ok(Some(url)) = direct_task.await {
        out.push(url);
    }
    out.extend(ranked.into_iter().map(|(_, url)| url));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
