//! `manifest.rs` 的单元测试。
//!
//! 独立成文件：清单校验的用例比实现还长，内联会把实现文件撑过行数上限。

use super::*;
use crate::plugin::permission::TrustLevel;

fn sample() -> serde_json::Value {
    serde_json::json!({
        "id": "dev.example.adapter",
        "name": "示例适配器",
        "version": "1.0.0",
        "kind": "adapter",
        "permissions": ["net_listen_local", "net_udp"],
        "runtime": { "kind": "process", "entry": "bin/adapter.exe" }
    })
}

fn parse(
    mut value: serde_json::Value,
    patch: impl FnOnce(&mut serde_json::Value),
) -> Result<PluginManifest, String> {
    patch(&mut value);
    PluginManifest::parse(&value.to_string())
}

#[test]
fn parses_minimal_manifest() {
    let m = PluginManifest::parse(&sample().to_string()).unwrap();
    assert_eq!(m.kind, PluginKind::Adapter);
    assert_eq!(m.games, vec!["*"]);
}

/// 回归保护：清单里写什么都不能影响信任。
///
/// 早期版本让清单自带 `signature` 字段并据此提权，那是"自述即可信"。现在
/// 该字段已从结构体移除，serde 会静默忽略它——本用例锁死这个行为：即使插件
/// 声明了 `permissions` 里的高权限、又伪造了签名字段，也只能拿到 `Unsigned`
/// 的权限上限。
#[test]
fn manifest_cannot_self_declare_trust() {
    let m = parse(sample(), |v| {
        v["permissions"] = serde_json::json!(["net_listen_public", "proc_spawn_game"]);
        v["signature"] = serde_json::json!({
            "algorithm": "ed25519",
            "public_key": "deadbeef",
            "signature": "cafebabe"
        });
    })
    .expect("未知字段应被静默忽略，而不是解析失败");

    let set = crate::plugin::permission::PermissionSet::resolve(
        &m.permissions,
        TrustLevel::Unsigned,
        None,
    );
    assert!(!set.contains(crate::plugin::permission::Permission::ProcSpawnGame));
    assert!(!set.contains(crate::plugin::permission::Permission::NetListenPublic));
}

#[test]
fn rejects_non_dotted_id() {
    assert!(parse(sample(), |v| v["id"] = "adapter".into()).is_err());
}

/// 越界入口必须被拒，且**判定与编译平台无关**。
///
/// `C:/evil.exe` 曾经只在 Windows 被拒（Linux 下 `is_absolute()` 为 false、
/// `C:` 是普通组件），导致同一份清单在两个平台校验结果不同。
#[test]
fn rejects_escaping_entry() {
    for bad in [
        "../../evil.exe",
        "/etc/passwd",
        // Windows 盘符（正/反斜杠两种写法）
        "C:/evil.exe",
        r"C:\evil.exe",
        "d:evil.exe",
        // UNC 与反斜杠逃逸
        r"\\server\share\evil.exe",
        r"..\..\evil.exe",
        // 空 / 仅空白
        "",
        "   ",
    ] {
        assert!(
            parse(sample(), |v| v["runtime"]["entry"] = bad.into()).is_err(),
            "入口 {bad:?} 应被拒绝"
        );
    }
}

#[test]
fn process_runtime_requires_entry() {
    assert!(parse(sample(), |v| v["runtime"]["entry"] =
        serde_json::Value::Null)
    .is_err());
}

/// Node 轨道：入口必须是插件目录内的脚本，且只认 `.js/.mjs/.cjs`。
#[test]
fn node_runtime_requires_script_entry() {
    assert!(parse(sample(), |v| {
        v["runtime"] = serde_json::json!({ "kind": "node", "entry": "dist/main.js" });
    })
    .is_ok());

    for bad in [
        "bin/adapter.exe",   // 原生二进制不能走 node 轨道
        "dist/main.ts",      // 刻意不支持 .ts（见 `is_node_script`）
        "dist/main",         // 没有扩展名
        "../outside.js",     // 越界
        "C:/evil.js",        // 盘符
        "",
    ] {
        assert!(
            parse(sample(), |v| {
                v["runtime"] = serde_json::json!({ "kind": "node", "entry": bad });
            })
            .is_err(),
            "node 入口 {bad:?} 应被拒绝"
        );
    }
}

#[test]
fn node_script_detection_is_case_insensitive() {
    assert!(is_node_script("dist/MAIN.JS"));
    assert!(is_node_script("a.mjs"));
    assert!(is_node_script("a.cjs"));
    assert!(!is_node_script("a.ts"));
    assert!(!is_node_script("a.js.map"));
}

#[test]
fn covers_game_matches_wildcard_and_id() {
    let m = PluginManifest::parse(&sample().to_string()).unwrap();
    assert!(m.covers_game("minecraft-java"));

    let m = parse(sample(), |v| {
        v["kind"] = "detector".into();
        v["games"] = serde_json::json!(["terraria"]);
    })
    .unwrap();
    assert!(m.covers_game("Terraria"));
    assert!(!m.covers_game("minecraft-java"));
}

#[test]
fn rejects_unknown_protocol_version() {
    assert!(parse(sample(), |v| v["protocol_version"] = 99.into()).is_err());
}

#[test]
fn rejects_absurd_frame_limit() {
    assert!(parse(sample(), |v| v["limits"]["max_frame_bytes"] = 0.into()).is_err());
}

/// 平台：未知取值丢弃、大小写归一、去重，但**不影响插件加载**。
#[test]
fn platform_normalization_drops_unknown_values() {
    let out = normalize_platforms(&[
        "Windows".to_string(),
        " windows ".to_string(),
        "linux".to_string(),
        "solaris".to_string(),
        String::new(),
    ]);
    assert_eq!(out, vec!["windows".to_string(), "linux".to_string()]);
}

/// 标签是开发者自定义的：不设白名单（中文允许），但要去重、截长、限量。
#[test]
fn tag_normalization_is_lenient_but_bounded() {
    let out = normalize_tags(&["免登录".to_string(), "免登录".to_string(), "x".repeat(200)]);
    assert_eq!(out.len(), 2, "重复标签必须合并: {out:?}");
    assert_eq!(out[0], "免登录");
    assert_eq!(out[1].chars().count(), 24, "超长标签必须截断");

    let many: Vec<String> = (0..50).map(|i| format!("tag-{i}")).collect();
    assert_eq!(normalize_tags(&many).len(), 12, "标签数量必须封顶");
}

/// 工作方式的字符串表示必须稳定（筛选界面直接依赖），解析要宽容。
#[test]
fn connection_method_parsing_is_tolerant_and_stable() {
    assert_eq!(ConnectionMethod::P2p.as_str(), "p2p");
    assert_eq!(ConnectionMethod::PortMapping.as_str(), "port-mapping");
    assert_eq!(
        ConnectionMethod::parse(" P2P "),
        Some(ConnectionMethod::P2p)
    );
    assert_eq!(
        ConnectionMethod::parse("port_mapping"),
        Some(ConnectionMethod::PortMapping)
    );
    assert_eq!(ConnectionMethod::parse("telepathy"), None);
    assert_eq!(ConnectionMethod::ALL.len(), 4);
}

/// 写错的工作方式只应被丢掉，不能让插件加载失败（与平台同口径）。
#[test]
fn unknown_method_is_dropped_not_fatal() {
    let m = parse(sample(), |v| {
        v["methods"] = serde_json::json!(["p2p", "telepathy", "relay"]);
    })
    .expect("未知工作方式不应导致解析失败");
    assert_eq!(
        normalize_methods(&m.methods),
        vec!["p2p".to_string(), "relay".to_string()]
    );
}

/// 声明了三个新维度的清单要能完整解析出来。
#[test]
fn parses_platforms_methods_and_tags() {
    let m = parse(sample(), |v| {
        v["platforms"] = serde_json::json!(["windows", "linux"]);
        v["methods"] = serde_json::json!(["p2p", "relay"]);
        v["tags"] = serde_json::json!(["低延迟"]);
    })
    .unwrap();
    assert_eq!(
        normalize_platforms(&m.platforms),
        vec!["windows".to_string(), "linux".to_string()]
    );
    assert_eq!(
        normalize_methods(&m.methods),
        vec!["p2p".to_string(), "relay".to_string()]
    );
    assert_eq!(normalize_tags(&m.tags), vec!["低延迟".to_string()]);
}
