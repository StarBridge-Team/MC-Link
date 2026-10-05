//! 社区信息：贡献者与 Issues（"关于 / 鸣谢"页的数据来源）。
//!
//! # 数据从哪来
//!
//! 客户端**不直接访问 GitHub**，只从资源服务器取两个静态 JSON：
//! `community/contributors.json` 与 `community/issues.json`。
//!
//! 为什么不直连 GitHub：`api.github.com` 在中国大陆经常不可达，而未认证配额只有
//! 60 次/小时**每 IP**。放到资源服务器（部署在亚太）走的是同一套已就绪的 HTTPS 通道，
//! 既能到得了，也不受配额约束。
//!
//! 这两个 JSON 由 CI 生成 —— GitHub Actions 的 runner 访问 GitHub 天然可达，且自带
//! `GITHUB_TOKEN`（5000 次/小时）。见 `scripts/sync-community.mjs` 与
//! `.github/workflows/community-refresh.yml`（每日刷新 + 发版时一并刷新）。
//!
//! # 三条硬规则（与 `setting_meta.rs` 同一套）
//!
//! 1. **绝不因为拉不到而失败**：失败时回退旧缓存并标 `stale` + `error`，
//!    关于页必须能离线渲染。
//! 2. **缓存放磁盘**：没必要每次打开页面都打一次网络。
//! 3. **缓存原子写**：先写临时文件再 rename —— 那份缓存是离线回退的唯一依靠。

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::asset_server::{assets_server_url, join};
use crate::cache::{cache_path, ensure_cache_dir, is_cached};

/// 规范仓库。旧地址 `DogerMMC/mc-link` 会被 GitHub 301 重定向到这里（仓库已转入组织）。
pub const REPO: &str = "StarBridge-Team/MC-Link";
pub const REPO_URL: &str = "https://github.com/StarBridge-Team/MC-Link";

/// 服务器上的路径（与 `scripts/sync-community.mjs` 的产出位置一一对应）。
const CONTRIBUTORS_PATH: &str = "community/contributors.json";
const ISSUES_PATH: &str = "community/issues.json";

/// 缓存时长。
///
/// 比"服务器多久刷新"短一档：服务器每日刷新（`community-refresh.yml`），客户端 1 小时
/// 检查一次，新数据最多滞后一小时可见。取 1 小时而不是更短，是因为打开关于页属于
/// 高频操作，没必要每次都打网络。
const CACHE_TTL: Duration = Duration::from_secs(3600);

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

// ------------------------------------------------------------------
// 数据结构（与服务器发布的 JSON 对齐）
// ------------------------------------------------------------------

/// 一位贡献者。
///
/// `login` 既是 GitHub 用户名，也用作界面上的显示名。
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
    /// Pull Request 在 GitHub 的 `/issues` 里与 issue 混在一起，靠这个字段区分。
    ///
    /// 服务器侧已经滤掉（见 `sync-community.mjs`）；这里保留判据做二次防御，
    /// 且 `skip_serializing` 不把它带给界面 —— 它是判据，不是内容。
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

/// 服务器发布的贡献者文档。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct ContributorsDoc {
    /// **必填**：用来确认"这确实是我们发布的文档"。
    ///
    /// serde 允许结构体从 JSON 数组反序列化（`[]` 会把所有有默认值的字段填成空），
    /// 所以只有"存在无默认值的字段"才能真正挡住格式回归 —— 否则服务器哪天退回旧格式，
    /// 客户端会安静地显示空列表，而不是给出可诊断的错误。
    repo: String,
    #[serde(default)]
    repo_url: String,
    #[serde(default)]
    contributors: Vec<Contributor>,
}

/// 服务器发布的 Issues 文档。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct IssuesDoc {
    /// **必填**，理由同 [`ContributorsDoc::repo`]。
    repo: String,
    #[serde(default)]
    repo_url: String,
    #[serde(default)]
    issues: Vec<Issue>,
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
    /// 数据来自缓存（离线或请求失败时为 true）。
    pub stale: bool,
    /// 失败原因；`None` 表示本次是新鲜数据。界面请放在次要位置，不要当错误弹窗。
    pub error: Option<String>,
}

// ------------------------------------------------------------------
// 纯函数：解析与过滤（可离线单测）
// ------------------------------------------------------------------

/// 机器人账号（`dependabot[bot]` 等）不该出现在鸣谢名单或社区反馈里。
pub fn is_bot(login: &str) -> bool {
    login.ends_with("[bot]")
}

/// 解析贡献者文档：滤掉机器人，按提交数降序。
pub fn parse_contributors(text: &str) -> Result<ContributorsDoc, String> {
    let mut doc: ContributorsDoc =
        serde_json::from_str(text).map_err(|e| format!("解析贡献者列表失败: {e}"))?;
    doc.contributors.retain(|c| !is_bot(&c.login));
    doc.contributors.sort_by(|a, b| {
        b.contributions
            .cmp(&a.contributions)
            .then_with(|| a.login.cmp(&b.login))
    });
    Ok(doc)
}

/// 解析 Issues 文档：**滤掉 Pull Request**，按更新时间降序。
///
/// 不滤掉的话，"社区反馈"里会混进一批代码 PR，界面上看起来就像有人开了 issue。
pub fn parse_issues(text: &str) -> Result<IssuesDoc, String> {
    let mut doc: IssuesDoc =
        serde_json::from_str(text).map_err(|e| format!("解析 Issues 失败: {e}"))?;
    doc.issues.retain(|i| {
        i.pull_request.is_none() && i.user.as_ref().map(|u| !is_bot(&u.login)).unwrap_or(true)
    });
    // RFC3339 定长字符串，字典序即时间序
    doc.issues.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(doc)
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

async fn request_doc<T>(
    client: &reqwest::Client,
    url: &str,
    parse: fn(&str) -> Result<T, String>,
) -> Result<T, String> {
    let resp = client
        .get(url)
        .timeout(HTTP_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("请求社区数据失败: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("资源服务器返回 {status}（{url}）"));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取社区数据失败: {e}"))?;
    parse(&text)
}

/// 取数据：新鲜缓存 → 直接用；否则请求网络；失败则回退到任意旧缓存。
///
/// 返回 `(数据, 是否来自旧缓存, 失败原因)`。
async fn fetch_cached<T>(
    data_dir: &Path,
    cache_name: &str,
    url: &str,
    client: &reqwest::Client,
    parse: fn(&str) -> Result<T, String>,
) -> (T, bool, Option<String>)
where
    T: Serialize + DeserializeOwned + Default,
{
    let dir = cache_dir(data_dir);
    // 建目录失败不该让整件事失败：只是这一次没缓存
    let _ = ensure_cache_dir(&dir);
    let file = cache_path(&dir, cache_name);

    if is_cached(&file, Some(CACHE_TTL)) {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Ok(value) = serde_json::from_str::<T>(&text) {
                return (value, false, None);
            }
        }
        // 缓存坏了就当作没有，继续走网络
    }

    match request_doc(client, url, parse).await {
        Ok(value) => {
            if let Ok(text) = serde_json::to_string(&value) {
                write_atomic(&file, &text);
            }
            (value, false, None)
        }
        Err(err) => {
            // 网络失败：退回旧缓存（哪怕已过期）。过期名单也远好过一片空白。
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
    let base = assets_server_url(data_dir);
    let contributors_url = join(&base, CONTRIBUTORS_PATH);
    let issues_url = join(&base, ISSUES_PATH);

    let (c_doc, c_stale, c_err) = fetch_cached::<ContributorsDoc>(
        data_dir,
        "contributors.json",
        &contributors_url,
        client,
        parse_contributors,
    )
    .await;

    let (i_doc, i_stale, i_err) =
        fetch_cached::<IssuesDoc>(data_dir, "issues.json", &issues_url, client, parse_issues).await;

    let error = match (c_err, i_err) {
        (None, None) => None,
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => Some(format!("{a}；{b}")),
    };

    // 仓库信息以服务器发布的为准（换仓库不用跟着发一次客户端），缺失时退回内置常量
    let repo_url = if !c_doc.repo_url.is_empty() {
        c_doc.repo_url.clone()
    } else if !i_doc.repo_url.is_empty() {
        i_doc.repo_url.clone()
    } else {
        REPO_URL.to_string()
    };

    Community {
        repo: c_doc.repo.clone(),
        repo_url,
        contributors: c_doc.contributors,
        issues: i_doc.issues,
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
mod tests;
