//! 社区信息：GitHub 上的贡献者与 Issues（"关于 / 鸣谢"页的数据来源）。
//!
//! # 为什么不引 GitHub SDK crate
//!
//! 需求只有两个只读 GET。现有 `reqwest` + `serde_json` 已足够表达它们，而
//! `octocrab` 一类会带进近百个传递依赖：构建时间变长，且每一份都要同步进
//! `legal/THIRD-PARTY.md`（CI 里有阻塞检查），换来的抽象我们一处也用不上。
//!
//! # 三条硬规则
//!
//! 1. **绝不因为拉不到而失败**：关于页必须能离线渲染。网络失败时回退到旧缓存，
//!    并用 `stale` / `error` 如实标注，而不是把异常抛给界面。
//! 2. **缓存放磁盘**：未认证的 GitHub API 只有 60 次/小时**每 IP**，
//!    每次打开页面都拉必然被限流（贡献者名单变化很慢，7 天足够）。
//! 3. **缓存原子写**：先写临时文件再 rename。那份缓存是离线回退的唯一依靠，
//!    半截 JSON 会让"离线时什么都看不到"。

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cache::{cache_path, ensure_cache_dir, is_cached};

/// 规范仓库。
///
/// 曾经的 `DogerMMC/mc-link` 会被 GitHub 301 重定向到这里（仓库已转入组织），
/// 两者取到的数据相同，但列表接口用规范名可以少一跳重定向。
pub const REPO: &str = "StarBridge-Team/MC-Link";

/// 仓库主页（给界面直接用的链接，省得前端自己拼）。
pub const REPO_URL: &str = "https://github.com/StarBridge-Team/MC-Link";

const API_BASE: &str = "https://api.github.com";

/// GitHub **强制**要求请求带 `User-Agent`（缺了直接 403），而 reqwest 默认不发。
const USER_AGENT: &str = concat!("MC-Link/", env!("CARGO_PKG_VERSION"));

/// 贡献者缓存时长：名单变化很慢，而配额只有 60 次/小时。
const CONTRIBUTORS_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// Issues 缓存时长：它是"活"数据，但也没必要每次开页面都拉。
const ISSUES_TTL: Duration = Duration::from_secs(3600);

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// 单页条数（GitHub 上限 100）。
const CONTRIBUTORS_PER_PAGE: u32 = 100;
const ISSUES_PER_PAGE: u32 = 30;

// ------------------------------------------------------------------
// 数据结构
// ------------------------------------------------------------------

/// 一位贡献者（只保留界面需要的字段，其余由 serde 忽略）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Contributor {
    pub login: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub contributions: u32,
}

/// 一条 Issue。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub comments: u32,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub user: Option<IssueUser>,
    /// Pull Request 也会出现在 `/issues` 里，靠这个字段区分。
    ///
    /// `skip` 掉：它是判据，不是要给界面的内容。
    #[serde(default, skip_serializing)]
    pub pull_request: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub name: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct IssueUser {
    pub login: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub html_url: String,
}

/// 交给界面的社区数据。
///
/// **刻意不是 `Result`**：拉不到时给的是"空列表 + 原因"，界面照常渲染，
/// 只是可以顺带提示一句。关于页因为网络问题打不开是最没必要的失败。
#[derive(Serialize, Clone, Debug, Default)]
pub struct Community {
    pub repo: String,
    pub repo_url: String,
    pub contributors: Vec<Contributor>,
    pub issues: Vec<Issue>,
    /// 数据来自缓存（离线、限流或请求失败时为 true）。
    pub stale: bool,
    /// 失败原因；`None` 表示本次是新鲜数据。界面请放在次要位置，不要当错误弹窗。
    pub error: Option<String>,
}

// ------------------------------------------------------------------
// 纯函数：解析与过滤（可离线单测）
// ------------------------------------------------------------------

/// 机器人账号（`dependabot[bot]` 等）不该出现在鸣谢名单里。
pub fn is_bot(login: &str) -> bool {
    login.ends_with("[bot]")
}

/// 解析贡献者：滤掉机器人，按提交数降序。
pub fn parse_contributors(text: &str) -> Result<Vec<Contributor>, String> {
    let raw: Vec<Contributor> =
        serde_json::from_str(text).map_err(|e| format!("解析贡献者列表失败: {e}"))?;
    let mut list: Vec<Contributor> = raw.into_iter().filter(|c| !is_bot(&c.login)).collect();
    list.sort_by(|a, b| {
        b.contributions
            .cmp(&a.contributions)
            .then_with(|| a.login.cmp(&b.login))
    });
    Ok(list)
}

/// 解析 Issues：**滤掉 Pull Request**（它们混在同一接口里），按更新时间降序。
///
/// 不滤掉的话，"社区反馈"里会混进一批代码 PR，界面上看起来就像有人开了 issue。
pub fn parse_issues(text: &str) -> Result<Vec<Issue>, String> {
    let raw: Vec<Issue> =
        serde_json::from_str(text).map_err(|e| format!("解析 Issues 失败: {e}"))?;
    let mut list: Vec<Issue> = raw
        .into_iter()
        .filter(|i| i.pull_request.is_none())
        .filter(|i| i.user.as_ref().map(|u| !is_bot(&u.login)).unwrap_or(true))
        .collect();
    // RFC3339 定长字符串，字典序即时间序
    list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(list)
}

// ------------------------------------------------------------------
// 缓存与网络
// ------------------------------------------------------------------

/// 社区数据缓存目录：`Cache/Community/`
fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("Cache").join("Community")
}

/// 原子写：先写临时文件再 rename。
///
/// 直接 `fs::write` 会在中断时留下半截 JSON，而这份缓存正是离线回退的唯一依靠
/// —— 缓存坏了，离线时就什么都看不到。
fn write_atomic(path: &Path, content: &str) {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, content).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// 可选令牌：设了就把握把配额从 60/小时 提到 5000/小时。
///
/// 只认环境变量，**不做进设置界面**：普通用户不该被要求提供 GitHub 令牌，
/// 而默认的 60/小时配上磁盘缓存完全够用。
fn token() -> Option<String> {
    std::env::var("MC_LINK_GITHUB_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

async fn request_json<T>(
    client: &reqwest::Client,
    url: &str,
    parse: fn(&str) -> Result<T, String>,
) -> Result<T, String> {
    let mut req = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(HTTP_TIMEOUT);
    if let Some(t) = token() {
        req = req.header("Authorization", format!("Bearer {t}"));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求 GitHub 失败: {e}"))?;
    let status = resp.status();

    if !status.is_success() {
        // 限流是这里最常见的失败（未认证 60 次/小时/IP）。单独说清楚，
        // 否则用户只看到"403"，不知道等一会儿就好。
        let remaining = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if remaining == "0" || status.as_u16() == 429 {
            return Err(format!("GitHub 接口限流（未认证每小时 60 次）：{status}"));
        }
        if status.as_u16() == 404 {
            return Err(format!("仓库不存在或不可见：{REPO}（{status}）"));
        }
        return Err(format!("GitHub 返回 {status}（{url}）"));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 GitHub 响应失败: {e}"))?;
    parse(&text)
}

/// 取数据：新鲜缓存 → 直接用；否则请求网络；失败则回退到任意旧缓存。
///
/// 返回 `(数据, 是否来自旧缓存, 失败原因)`。
async fn fetch_cached<T>(
    data_dir: &Path,
    cache_name: &str,
    ttl: Duration,
    url: &str,
    client: &reqwest::Client,
    parse: fn(&str) -> Result<T, String>,
) -> (T, bool, Option<String>)
where
    T: Serialize + serde::de::DeserializeOwned + Default,
{
    let dir = cache_dir(data_dir);
    // 建目录失败不该让整件事失败：只是这一次没缓存
    let _ = ensure_cache_dir(&dir);
    let file = cache_path(&dir, cache_name);

    if is_cached(&file, Some(ttl)) {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Ok(value) = serde_json::from_str::<T>(&text) {
                return (value, false, None);
            }
        }
        // 缓存坏了就当作没有，继续走网络
    }

    match request_json(client, url, parse).await {
        Ok(value) => {
            if let Ok(text) = serde_json::to_string(&value) {
                write_atomic(&file, &text);
            }
            (value, false, None)
        }
        Err(err) => {
            // 网络失败：退回旧缓存（哪怕已过期）。过期的名单也远好过一片空白。
            if let Ok(text) = std::fs::read_to_string(&file) {
                if let Ok(value) = serde_json::from_str::<T>(&text) {
                    return (value, true, Some(err));
                }
            }
            (T::default(), true, Some(err))
        }
    }
}

/// 拉取社区数据（贡献者 + Issues）。**永不返回 `Err`**，理由见模块头注释。
pub async fn fetch(data_dir: &Path, client: &reqwest::Client) -> Community {
    let contributors_url =
        format!("{API_BASE}/repos/{REPO}/contributors?per_page={CONTRIBUTORS_PER_PAGE}");
    let issues_url = format!(
        "{API_BASE}/repos/{REPO}/issues?state=open&per_page={ISSUES_PER_PAGE}&sort=updated"
    );

    let (contributors, c_stale, c_err) = fetch_cached(
        data_dir,
        "contributors.json",
        CONTRIBUTORS_TTL,
        &contributors_url,
        client,
        parse_contributors,
    )
    .await;

    let (issues, i_stale, i_err) = fetch_cached(
        data_dir,
        "issues.json",
        ISSUES_TTL,
        &issues_url,
        client,
        parse_issues,
    )
    .await;

    let error = match (c_err, i_err) {
        (None, None) => None,
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => Some(format!("{a}；{b}")),
    };

    Community {
        repo: REPO.to_string(),
        repo_url: REPO_URL.to_string(),
        contributors,
        issues,
        stale: c_stale || i_stale,
        error,
    }
}

/// 清理社区缓存（"重新拉取"用）。
pub fn clear_cache(data_dir: &Path) -> Result<(), String> {
    let dir = cache_dir(data_dir);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("清理社区缓存失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个必定连不上的地址：用回环的保留端口，失败是立刻的（不会等超时）。
    const DEAD_URL: &str = "http://127.0.0.1:1/nope";

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mc-link-community-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn contributors_parse_filters_bots_and_sorts_by_contributions() {
        let json = r#"[
            {"login":"alice","avatar_url":"a","html_url":"h","contributions":3},
            {"login":"dependabot[bot]","contributions":99},
            {"login":"bob","contributions":10}
        ]"#;
        let list = parse_contributors(json).unwrap();
        assert_eq!(
            list.iter().map(|c| c.login.as_str()).collect::<Vec<_>>(),
            vec!["bob", "alice"],
            "机器人必须被滤掉，且按提交数降序"
        );
    }

    /// PR 与 issue 共用 `/issues` 接口：不滤掉的话"社区反馈"里会混进代码 PR。
    #[test]
    fn issues_parse_drops_pull_requests() {
        let json = r#"[
            {"number":1,"title":"真 issue","updated_at":"2026-01-02T00:00:00Z",
             "user":{"login":"alice"}},
            {"number":2,"title":"一个 PR","updated_at":"2026-03-02T00:00:00Z",
             "pull_request":{"url":"x"},"user":{"login":"bob"}},
            {"number":3,"title":"机器人提的","updated_at":"2026-04-02T00:00:00Z",
             "user":{"login":"github-actions[bot]"}}
        ]"#;
        let list = parse_issues(json).unwrap();
        assert_eq!(list.len(), 1, "PR 与机器人提交都该被滤掉: {list:?}");
        assert_eq!(list[0].number, 1);
        assert_eq!(list[0].title, "真 issue");
    }

    #[test]
    fn issues_parse_sorts_by_updated_desc() {
        let json = r#"[
            {"number":1,"title":"旧","updated_at":"2026-01-01T00:00:00Z"},
            {"number":2,"title":"新","updated_at":"2026-09-01T00:00:00Z"}
        ]"#;
        let list = parse_issues(json).unwrap();
        assert_eq!(list[0].number, 2);
    }

    #[test]
    fn malformed_json_is_reported_not_panicking() {
        assert!(parse_contributors("{").is_err());
        assert!(parse_issues("[]x").is_err());
    }

    #[test]
    fn bot_detection_covers_common_naming() {
        assert!(is_bot("dependabot[bot]"));
        assert!(is_bot("github-actions[bot]"));
        assert!(!is_bot("alice"));
    }

    /// 新鲜缓存必须**不打网络**：URL 是死的，能返回数据就证明走了缓存。
    #[tokio::test]
    async fn fresh_cache_is_used_without_network() {
        let dir = temp_dir("fresh");
        let cache_dir = cache_dir(&dir);
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("contributors.json"),
            r#"[{"login":"cached","contributions":1}]"#,
        )
        .unwrap();

        let (list, stale, err) = fetch_cached(
            &dir,
            "contributors.json",
            Duration::from_secs(3600),
            DEAD_URL,
            &reqwest::Client::new(),
            parse_contributors,
        )
        .await;

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].login, "cached");
        assert!(!stale, "新鲜缓存不该被标成 stale");
        assert!(err.is_none(), "走缓存时不该有错误: {err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ttl 为 0 时缓存一律视为过期 → 请求网络 → 失败 → 回退旧缓存并标注。
    #[tokio::test]
    async fn expired_cache_falls_back_when_network_fails() {
        let dir = temp_dir("stale");
        let cache_dir = cache_dir(&dir);
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("contributors.json"),
            r#"[{"login":"old","contributions":7}]"#,
        )
        .unwrap();

        let (list, stale, err) = fetch_cached(
            &dir,
            "contributors.json",
            Duration::ZERO,
            DEAD_URL,
            &reqwest::Client::new(),
            parse_contributors,
        )
        .await;

        assert_eq!(list.len(), 1, "网络失败时必须回退旧缓存");
        assert_eq!(list[0].login, "old");
        assert!(stale, "来自旧缓存必须标注为 stale");
        assert!(err.is_some(), "失败原因要如实带上，供界面提示");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 没有缓存又拉不到：给空数据而不是错误 —— 关于页仍要能渲染。
    #[tokio::test]
    async fn no_cache_and_failure_yields_empty_not_error() {
        let dir = temp_dir("empty");
        let (list, stale, err) = fetch_cached(
            &dir,
            "contributors.json",
            Duration::ZERO,
            DEAD_URL,
            &reqwest::Client::new(),
            parse_contributors,
        )
        .await;

        assert!(list.is_empty());
        assert!(stale);
        assert!(err.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn repo_constants_are_consistent() {
        assert_eq!(REPO, "StarBridge-Team/MC-Link");
        assert!(REPO_URL.ends_with(REPO));
    }
}
