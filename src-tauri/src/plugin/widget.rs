//! `widget` 类插件的**声明式视图**校验。
//!
//! # 为什么不是"插件交 HTML/JS"
//!
//! 挂件跑在核心的主窗口里。如果插件能交任意 HTML/JS，它就能读 DOM、拿会话、绕过
//! 全部权限模型——那是把整个安全边界交给第三方代码。因此这里只接受**声明式节点树**：
//! 核心决定它长什么样，前端只渲染白名单里的类型，用户的点击以 `action` 回传。
//!
//! 代价是表达力有限（没有自定义样式、没有布局引擎）。这是刻意的取舍：挂件要的是
//! "把插件状态显示出来、把按钮点下去"，不是再造一个前端框架。
//!
//! # 校验策略
//!
//! **白名单 + 上限 + 逐字段裁剪**：未知类型直接丢弃（不报错，避免一个拼写错误让整个
//! 挂件渲染失败），未知字段一律不保留（防止插件塞 `dangerouslySetInnerHTML` 之类
//! 的字段指望前端"顺手"支持）。树大小、深度、文本长度都有硬上限。

use serde_json::{json, Map, Value};

/// 单次渲染的节点总数上限。
pub const MAX_NODES: usize = 200;
/// 节点树深度上限。
pub const MAX_DEPTH: usize = 8;
/// 单个文本字段的字符上限。
pub const MAX_TEXT_CHARS: usize = 2000;
/// 按钮/动作名长度上限。
pub const MAX_ACTION_CHARS: usize = 96;
/// 允许的节点类型（白名单）。
const KINDS: &[&str] = &[
    "text", "heading", "badge", "button", "row", "column", "divider", "progress", "spacer",
];

/// 校验并规范化插件返回的视图。
///
/// 接受两种形状：
/// - `{ title?, subtitle?, nodes: [...] }`
/// - `[...]`（裸节点数组）
///
/// 返回规范化后的同一形状；任何非法输入都会被裁剪而不是抛错（除了"完全无法解析"）。
pub fn validate_view(raw: &Value) -> Result<Value, String> {
    let mut budget = MAX_NODES;

    match raw {
        Value::Array(items) => {
            let nodes = normalize_nodes(items, 1, &mut budget);
            Ok(json!({ "title": "", "nodes": nodes }))
        }
        Value::Object(map) => {
            let nodes = map
                .get("nodes")
                .and_then(Value::as_array)
                .map(|items| normalize_nodes(items, 1, &mut budget))
                .unwrap_or_default();
            let mut out = Map::new();
            out.insert(
                "title".to_string(),
                Value::String(text_field(map.get("title"), 200)),
            );
            out.insert(
                "subtitle".to_string(),
                Value::String(text_field(map.get("subtitle"), 200)),
            );
            out.insert("nodes".to_string(), Value::Array(nodes));
            Ok(Value::Object(out))
        }
        Value::Null => Ok(json!({ "title": "", "nodes": [] })),
        other => Err(format!("挂件视图必须是对象或数组，收到 {}", kind_name(other))),
    }
}

fn normalize_nodes(items: &[Value], depth: usize, budget: &mut usize) -> Vec<Value> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for item in items {
        if *budget == 0 {
            break;
        }
        if let Some(node) = normalize_node(item, depth, budget) {
            *budget -= 1;
            out.push(node);
        }
    }
    out
}

fn normalize_node(raw: &Value, depth: usize, budget: &mut usize) -> Option<Value> {
    let map = raw.as_object()?;
    let kind = map.get("type").and_then(Value::as_str)?.trim();
    if !KINDS.contains(&kind) {
        // 未知类型静默丢弃：一个拼写错误不该让整个挂件白屏
        return None;
    }

    let mut out = Map::new();
    out.insert("type".to_string(), Value::String(kind.to_string()));
    if let Some(id) = map.get("id").and_then(Value::as_str) {
        let id = clamp(id, 64);
        if !id.is_empty() {
            out.insert("id".to_string(), Value::String(id));
        }
    }

    for (key, limit) in [
        ("text", MAX_TEXT_CHARS),
        ("label", 200),
        ("value", 500),
        ("placeholder", 200),
        ("tone", 16),
    ] {
        if map.contains_key(key) {
            out.insert(key.to_string(), Value::String(text_field(map.get(key), limit)));
        }
    }

    if let Some(action) = map.get("action").and_then(Value::as_str) {
        let action = clamp(action, MAX_ACTION_CHARS);
        if !action.is_empty() {
            out.insert("action".to_string(), Value::String(action));
        }
    }
    if let Some(disabled) = map.get("disabled").and_then(Value::as_bool) {
        out.insert("disabled".to_string(), Value::Bool(disabled));
    }
    if let Some(percent) = map.get("percent").and_then(Value::as_f64) {
        out.insert(
            "percent".to_string(),
            json!(percent.clamp(0.0, 100.0).round()),
        );
    }

    // 只有容器类允许 children，避免 `text` 里塞一棵树把渲染搞乱
    if matches!(kind, "row" | "column") {
        if let Some(children) = map.get("children").and_then(Value::as_array) {
            out.insert(
                "children".to_string(),
                Value::Array(normalize_nodes(children, depth + 1, budget)),
            );
        }
    }

    Some(Value::Object(out))
}

fn text_field(value: Option<&Value>, limit: usize) -> String {
    value
        .and_then(Value::as_str)
        .map(|s| clamp(s, limit))
        .unwrap_or_default()
}

/// 按**字符**截断（不是字节）：中文被切成半个字会渲染成乱码。
fn clamp(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    trimmed.chars().take(limit).collect()
}

fn kind_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_whitelisted_nodes_and_fields() {
        let raw = json!({
            "title": "  房间状态  ",
            "nodes": [
                { "type": "heading", "text": "已连接" },
                { "type": "button", "label": "刷新", "action": "refresh", "danger": "rm -rf" },
                { "type": "script", "text": "<script>alert(1)</script>" }
            ]
        });
        let view = validate_view(&raw).unwrap();
        assert_eq!(view["title"], "房间状态");
        let nodes = view["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2, "未知类型必须被丢弃: {view}");
        assert_eq!(nodes[0]["type"], "heading");
        assert_eq!(nodes[1]["action"], "refresh");
        assert!(
            nodes[1].get("danger").is_none(),
            "未知字段不得透传给前端: {view}"
        );
    }

    #[test]
    fn caps_depth_nodes_and_text() {
        // 深度：10 层嵌套，只有前 8 层保留
        let mut node = json!({ "type": "text", "text": "x" });
        for _ in 0..10 {
            node = json!({ "type": "column", "children": [node] });
        }
        let view = validate_view(&json!([node])).unwrap();
        let mut depth = 0;
        let mut cursor = &view["nodes"][0];
        while let Some(children) = cursor.get("children").and_then(Value::as_array) {
            let Some(first) = children.first() else { break };
            cursor = first;
            depth += 1;
        }
        assert!(depth <= MAX_DEPTH, "嵌套深度应被截断，实际 {depth}");

        // 节点总数
        let many: Vec<Value> = (0..(MAX_NODES + 50))
            .map(|_| json!({ "type": "text", "text": "x" }))
            .collect();
        let view = validate_view(&json!(many)).unwrap();
        assert_eq!(view["nodes"].as_array().unwrap().len(), MAX_NODES);

        // 文本按字符截断
        let long = "字".repeat(MAX_TEXT_CHARS + 100);
        let view = validate_view(&json!([{ "type": "text", "text": long }])).unwrap();
        assert_eq!(
            view["nodes"][0]["text"].as_str().unwrap().chars().count(),
            MAX_TEXT_CHARS
        );
    }

    #[test]
    fn children_are_ignored_on_leaf_nodes() {
        let view = validate_view(&json!([{
            "type": "text",
            "text": "a",
            "children": [{ "type": "text", "text": "b" }]
        }]))
        .unwrap();
        assert!(view["nodes"][0].get("children").is_none());
    }

    #[test]
    fn null_becomes_an_empty_view() {
        let view = validate_view(&Value::Null).unwrap();
        assert_eq!(view["nodes"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn rejects_non_object_roots() {
        assert!(validate_view(&json!("<html>")).is_err());
        assert!(validate_view(&json!(42)).is_err());
    }
}
