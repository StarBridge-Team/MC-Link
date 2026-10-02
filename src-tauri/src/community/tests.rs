
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
    let json = r#"{
            "repo":"StarBridge-Team/MC-Link",
            "repo_url":"https://github.com/StarBridge-Team/MC-Link",
            "contributors":[
                {"login":"alice","avatar_url":"a","html_url":"h","contributions":3},
                {"login":"dependabot[bot]","contributions":99},
                {"login":"bob","contributions":10}
            ]
        }"#;
    let doc = parse_contributors(json).unwrap();
    assert_eq!(
        doc.contributors
            .iter()
            .map(|c| c.login.as_str())
            .collect::<Vec<_>>(),
        vec!["bob", "alice"],
        "机器人必须被滤掉，且按提交数降序"
    );
    assert_eq!(doc.repo, "StarBridge-Team/MC-Link");
}

/// PR 与 issue 共用 `/issues` 接口：不滤掉的话"社区反馈"里会混进代码 PR。
#[test]
fn issues_parse_drops_pull_requests() {
    let json = r#"{
            "repo":"StarBridge-Team/MC-Link",
            "issues":[
                {"number":1,"title":"真 issue","updated_at":"2026-01-02T00:00:00Z",
                 "user":{"login":"alice"}},
                {"number":2,"title":"一个 PR","updated_at":"2026-03-02T00:00:00Z",
                 "pull_request":{"url":"x"},"user":{"login":"bob"}},
                {"number":3,"title":"机器人提的","updated_at":"2026-04-02T00:00:00Z",
                 "user":{"login":"github-actions[bot]"}}
            ]
        }"#;
    let doc = parse_issues(json).unwrap();
    assert_eq!(doc.issues.len(), 1, "PR 与机器人提交都该被滤掉");
    assert_eq!(doc.issues[0].number, 1);
    assert_eq!(doc.issues[0].title, "真 issue");
}

#[test]
fn issues_parse_sorts_by_updated_desc() {
    let json = r#"{"repo":"r","issues":[
            {"number":1,"title":"旧","updated_at":"2026-01-01T00:00:00Z"},
            {"number":2,"title":"新","updated_at":"2026-09-01T00:00:00Z"}
        ]}"#;
    let doc = parse_issues(json).unwrap();
    assert_eq!(doc.issues[0].number, 2);
}

/// 空列表是常态（仓库可能还没有 issue），不是错误。
#[test]
fn empty_documents_are_valid() {
    let c = parse_contributors(r#"{"repo":"r","contributors":[]}"#).unwrap();
    assert!(c.contributors.is_empty());
    assert_eq!(c.repo, "r");
    assert!(parse_issues(r#"{"repo":"r","issues":[]}"#)
        .unwrap()
        .issues
        .is_empty());
}

/// 缺 `repo` 的文档一律不接受。
///
/// serde 会把 JSON 数组（`[]`）也当成"所有字段取默认值"的结构体，所以只有必填字段
/// 能挡住格式回归 —— 否则服务器退回旧格式时，客户端会安静地显示空列表。
#[test]
fn documents_without_repo_are_rejected() {
    assert!(parse_contributors("{").is_err());
    assert!(parse_issues("[]x").is_err());
    assert!(parse_contributors("[]").is_err());
    assert!(parse_contributors("{}").is_err());
    assert!(parse_issues(r#"{"issues":[]}"#).is_err());
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
    let cache = cache_dir(&dir);
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        cache.join("contributors.json"),
        r#"{"repo":"cached-repo","contributors":[{"login":"cached","contributions":1}]}"#,
    )
    .unwrap();

    let (doc, stale, err) = fetch_cached(
        &dir,
        "contributors.json",
        DEAD_URL,
        &reqwest::Client::new(),
        parse_contributors,
    )
    .await;

    assert_eq!(doc.contributors.len(), 1);
    assert_eq!(doc.contributors[0].login, "cached");
    assert!(!stale, "新鲜缓存不该被标成 stale");
    assert!(err.is_none(), "走缓存时不该有错误: {err:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 缓存过期 → 请求网络 → 失败 → 回退旧缓存并如实标注。
#[tokio::test]
async fn expired_cache_used_when_network_fails() {
    let dir = temp_dir("stale");
    let cache = cache_dir(&dir);
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        cache.join("contributors.json"),
        r#"{"repo":"r","contributors":[{"login":"old","contributions":7}]}"#,
    )
    .unwrap();

    // 让缓存立刻过期：把 mtime 改成很久以前
    let file = cache.join("contributors.json");
    let old = std::time::SystemTime::now() - Duration::from_secs(60 * 60 * 24 * 30);
    let f = std::fs::File::options().write(true).open(&file).unwrap();
    let _ = f.set_modified(old);

    let (doc, stale, err) = fetch_cached(
        &dir,
        "contributors.json",
        DEAD_URL,
        &reqwest::Client::new(),
        parse_contributors,
    )
    .await;

    assert_eq!(doc.contributors.len(), 1, "网络失败时必须回退旧缓存");
    assert_eq!(doc.contributors[0].login, "old");
    assert!(stale, "来自旧缓存必须标注为 stale");
    assert!(err.is_some(), "失败原因要如实带上，供界面提示");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 没有缓存又拉不到：给空数据而不是错误 —— 关于页仍要能渲染。
#[tokio::test]
async fn no_cache_and_failure_yields_empty_not_error() {
    let dir = temp_dir("empty");
    let (doc, stale, err) = fetch_cached(
        &dir,
        "contributors.json",
        DEAD_URL,
        &reqwest::Client::new(),
        parse_contributors,
    )
    .await;

    assert!(doc.contributors.is_empty());
    assert!(stale);
    assert!(err.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repo_constants_are_consistent() {
    assert_eq!(REPO, "StarBridge-Team/MC-Link");
    assert!(REPO_URL.ends_with(REPO));
}
