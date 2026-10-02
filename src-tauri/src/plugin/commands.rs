//! 插件子系统的 Tauri 命令。
//!
//! 这些命令面向**后续的插件管理界面**。当前前端尚未改动，因此它们只新增、
//! 不修改既有命令签名，旧页面完全不受影响。

use serde_json::{json, Value};
use std::sync::Arc;

use crate::plugin::manager::PluginManager;
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::Permission;

/// 列出插件，支持搜索与按维度筛选（参数全可选，不传即"全部"，与旧行为一致）。
///
/// # facets 为什么值得单独算
///
/// 每个维度的计数以**其它**维度的筛选结果作分母：界面因此能实时告诉用户
/// "再勾这个条件还剩几个"。若改成"全部插件的总数"，用户很容易筛出空结果
/// 却不知道是哪一项把自己滤没的。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn plugin_list(
    manager: tauri::State<'_, Arc<PluginManager>>,
    query: Option<String>,
    kinds: Option<Vec<String>>,
    methods: Option<Vec<String>>,
    platforms: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    game_id: Option<String>,
    enabled_only: Option<bool>,
) -> Result<Value, String> {
    /// 参与筛选的字段（与展示用的 JSON 分开，避免每次筛选都去读 `Value`）。
    struct Entry {
        index: usize,
        kind: String,
        methods: Vec<String>,
        platforms: Vec<String>,
        tags: Vec<String>,
        games: Vec<String>,
        enabled: bool,
        haystack: String,
        name_hit: bool,
    }

    let normalized_query = query
        .as_deref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let wanted_kinds: Vec<String> = kinds
        .unwrap_or_default()
        .iter()
        .map(|k| k.trim().to_ascii_lowercase())
        .filter(|k| !k.is_empty())
        .collect();
    let wanted_methods: Vec<String> = methods
        .unwrap_or_default()
        .iter()
        .map(|m| m.trim().to_ascii_lowercase().replace('_', "-"))
        .filter(|m| !m.is_empty())
        .collect();
    let wanted_platforms: Vec<String> = platforms
        .unwrap_or_default()
        .iter()
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let wanted_tags: Vec<String> = tags
        .unwrap_or_default()
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    let wanted_game = game_id
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_string);
    let only_enabled = enabled_only.unwrap_or(false);

    let records = manager.list();
    let mut items: Vec<Value> = Vec::with_capacity(records.len());
    let mut entries: Vec<Entry> = Vec::with_capacity(records.len());

    for record in records.iter() {
        let manifest = &record.manifest;
        // 平台与标签在这里归一化：未知平台被丢弃、标签去重限量。
        // 放在展示层而不是加载层，是为了"标签写错"永远不升级成"插件不可用"。
        let platforms = crate::plugin::manifest::normalize_platforms(&manifest.platforms);
        let tags = crate::plugin::manifest::normalize_tags(&manifest.tags);
        let methods = crate::plugin::manifest::normalize_methods(&manifest.methods);

        let haystack = [
            manifest.id.as_str(),
            manifest.name.as_str(),
            manifest.description.as_deref().unwrap_or(""),
            manifest.author.as_deref().unwrap_or(""),
            &manifest.games.join(" "),
            &tags.join(" "),
        ]
        .join(" ")
        .to_lowercase();

        items.push(json!({
            "id": manifest.id,
            "name": manifest.name,
            "version": manifest.version,
            "kind": manifest.kind.as_str(),
            "author": manifest.author,
            "description": manifest.description,
            "games": manifest.games,
            "platforms": platforms,
            "methods": methods,
            "tags": tags,
            "source": match record.source {
                crate::plugin::registry::PluginSource::Builtin => "builtin",
                crate::plugin::registry::PluginSource::External => "external",
            },
            "trust": record.trust,
            "enabled": record.enabled,
            "runnable": record.trust.is_runnable(),
            "permissions": record.permissions.allowed(),
            "deniedByCeiling": record.permissions.denied_by_ceiling(),
            "declaredPermissions": manifest.permissions,
            "permissionDescriptions": manifest.permissions.iter()
                .map(|p| json!({ "name": p, "description": p.describe(), "highRisk": p.is_high_risk() }))
                .collect::<Vec<_>>(),
            "priority": manifest.priority,
            "fallback": manifest.fallback,
        }));

        entries.push(Entry {
            index: items.len() - 1,
            kind: manifest.kind.as_str().to_string(),
            methods,
            platforms,
            tags,
            games: manifest.games.clone(),
            enabled: record.enabled,
            name_hit: normalized_query
                .as_deref()
                .is_some_and(|q| manifest.name.to_lowercase().contains(q)),
            haystack,
        });
    }

    // `skip` 让 facets 能"忽略本维度自身"地统计，见上面的注释。
    let passes = |entry: &Entry, skip: Option<&str>| -> bool {
        if only_enabled && !entry.enabled {
            return false;
        }
        if skip != Some("kinds") && !wanted_kinds.is_empty() && !wanted_kinds.contains(&entry.kind)
        {
            return false;
        }
        if skip != Some("methods") && !wanted_methods.is_empty() {
            if !entry.methods.iter().any(|m| wanted_methods.contains(m)) {
                return false;
            }
        }
        if skip != Some("platforms") && !wanted_platforms.is_empty() {
            // 未声明平台的插件视为"与平台无关"（内置插件通常如此），不被平台条件滤掉
            if !entry.platforms.is_empty()
                && !entry.platforms.iter().any(|p| wanted_platforms.contains(p))
            {
                return false;
            }
        }
        if skip != Some("tags") && !wanted_tags.is_empty() {
            if !entry
                .tags
                .iter()
                .any(|t| wanted_tags.contains(&t.to_lowercase()))
            {
                return false;
            }
        }
        if let Some(game) = wanted_game.as_deref() {
            if !entry.games.iter().any(|g| g == game || g == "*") {
                return false;
            }
        }
        if let Some(q) = normalized_query.as_deref() {
            if !entry.haystack.contains(q) {
                return false;
            }
        }
        true
    };

    let matched: Vec<&Entry> = entries.iter().filter(|e| passes(e, None)).collect();

    let facets_for = |dim: &str| -> Value {
        let mut map = serde_json::Map::new();
        for entry in entries.iter().filter(|e| passes(e, Some(dim))) {
            let values: Vec<String> = match dim {
                "kinds" => vec![entry.kind.clone()],
                "methods" => entry.methods.clone(),
                "platforms" => entry.platforms.clone(),
                "tags" => entry.tags.clone(),
                _ => entry.games.clone(),
            };
            for value in values {
                let key = if dim == "tags" {
                    value.to_lowercase()
                } else {
                    value
                };
                let counter = map.entry(key).or_insert_with(|| json!(0));
                if let Some(n) = counter.as_u64() {
                    *counter = json!(n + 1);
                }
            }
        }
        Value::Object(map)
    };

    // 有搜索词时把"名字命中"的排前面；同分按 priority 降序（路由也用它）。
    let mut ordered: Vec<&Entry> = matched.clone();
    if normalized_query.is_some() {
        ordered.sort_by(|a, b| {
            b.name_hit
                .cmp(&a.name_hit)
                .then_with(|| entries[b.index].kind.cmp(&entries[a.index].kind))
        });
    }
    let plugins: Vec<Value> = ordered.iter().map(|e| items[e.index].clone()).collect();

    Ok(json!({
        "plugins": plugins,
        "facets": {
            "kinds": facets_for("kinds"),
            "methods": facets_for("methods"),
            "platforms": facets_for("platforms"),
            "tags": facets_for("tags"),
            "games": facets_for("games"),
        },
        // 界面据此默认只看本机能跑的插件
        "currentPlatform": std::env::consts::OS,
        "total": items.len(),
        "matched": matched.len(),
        "warnings": manager.warnings(),
    }))
}

/// 查询插件网关状态。网关**只监听回环地址**。
#[tauri::command]
pub(crate) fn plugin_gateway(manager: tauri::State<'_, Arc<PluginManager>>) -> Value {
    let mut info = manager.gateway_info();
    if let Some(map) = info.as_object_mut() {
        map.insert("loopbackOnly".to_string(), json!(true));
        map.insert(
            "protocolVersion".to_string(),
            json!(crate::plugin::protocol::PROTOCOL_VERSION),
        );
        map.insert(
            "subprotocol".to_string(),
            json!(crate::plugin::protocol::WS_SUBPROTOCOL),
        );
    }
    info
}

/// 列出已登记的游戏画像，支持搜索与按工作方式筛选。
///
/// 游戏的"工作方式"是**派生**出来的：看有哪些插件声明覆盖它。这样同一份事实
/// （谁能处理哪个游戏）只写一处，不会出现"游戏画像说支持 P2P、插件说不行"这种矛盾。
#[tauri::command]
pub(crate) fn game_list(
    manager: tauri::State<'_, Arc<PluginManager>>,
    query: Option<String>,
    methods: Option<Vec<String>>,
) -> Value {
    struct Row {
        item: Value,
        haystack: String,
        methods: Vec<String>,
    }

    let normalized_query = query
        .as_deref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let wanted: Vec<String> = methods
        .unwrap_or_default()
        .iter()
        .map(|m| m.trim().to_ascii_lowercase().replace('_', "-"))
        .filter(|m| !m.is_empty())
        .collect();

    let records = manager.list();
    let methods_for = |game_id: &str| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for record in records.iter() {
            let covers = record
                .manifest
                .games
                .iter()
                .any(|g| g == game_id || g == "*");
            if !covers {
                continue;
            }
            for value in crate::plugin::manifest::normalize_methods(&record.manifest.methods) {
                if !out.contains(&value) {
                    out.push(value);
                }
            }
        }
        out
    };

    let mut rows: Vec<Row> = Vec::new();
    for game in manager.games() {
        let methods = methods_for(&game.id);
        let haystack = [
            game.id.as_str(),
            game.name.as_str(),
            &game.aliases.join(" "),
        ]
        .join(" ")
        .to_lowercase();
        rows.push(Row {
            item: json!({
                "id": game.id,
                "name": game.name,
                "aliases": game.aliases,
                "port": game.default_port,
                "transport": game.transport.as_str(),
                "traits": game.traits,
                "processNames": game.process_names,
                "requiresCoupler": game.requires_coupler,
                "preferredAdapter": game.preferred_adapter,
                "fallbackAdapters": game.fallback_adapters,
                "methods": methods,
            }),
            haystack,
            methods,
        });
    }

    let matched: Vec<&Row> = rows
        .iter()
        .filter(|row| {
            if let Some(q) = normalized_query.as_deref() {
                if !row.haystack.contains(q) {
                    return false;
                }
            }
            if !wanted.is_empty() && !row.methods.iter().any(|m| wanted.contains(m)) {
                return false;
            }
            true
        })
        .collect();

    // 只有"工作方式"一个维度，所以它的 facets 以搜索词结果为分母
    let mut method_facets = serde_json::Map::new();
    for row in rows.iter().filter(|row| match normalized_query.as_deref() {
        Some(q) => row.haystack.contains(q),
        None => true,
    }) {
        for method in &row.methods {
            let counter = method_facets
                .entry(method.clone())
                .or_insert_with(|| json!(0));
            if let Some(n) = counter.as_u64() {
                *counter = json!(n + 1);
            }
        }
    }

    json!({
        "games": matched.iter().map(|row| row.item.clone()).collect::<Vec<_>>(),
        "facets": { "methods": Value::Object(method_facets) },
        "total": rows.len(),
        "matched": matched.len(),
    })
}

/// 计算某能力在某游戏下的路由计划（用于诊断"为什么用这个插件"）。
#[tauri::command]
pub(crate) fn plugin_route_plan(
    manager: tauri::State<'_, Arc<PluginManager>>,
    kind: String,
    game_id: Option<String>,
) -> Result<Value, String> {
    let kind = parse_kind(&kind)?;
    let plan = manager.plan(kind, game_id.as_deref());
    Ok(json!({
        "kind": kind.as_str(),
        "gameId": plan.game_id,
        "primary": plan.primary(),
        "candidates": plan.candidates.iter()
            .map(|c| json!({ "pluginId": c.plugin_id, "reason": c.reason, "score": c.score }))
            .collect::<Vec<_>>(),
    }))
}

/// 启用/停用插件。
#[tauri::command]
pub(crate) fn plugin_set_enabled(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    manager.set_enabled(&plugin_id, enabled)
}

/// 写入用户授权集合；`granted` 为 `null` 表示撤销用户决策、回落到清单声明。
#[tauri::command]
pub(crate) fn plugin_set_grants(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    granted: Option<Vec<Permission>>,
) -> Result<(), String> {
    manager.set_grants(&plugin_id, granted)
}

/// 拉黑/解除拉黑插件。
#[tauri::command]
pub(crate) fn plugin_set_blocked(
    manager: tauri::State<'_, Arc<PluginManager>>,
    plugin_id: String,
    blocked: bool,
) -> Result<(), String> {
    manager.set_blocked(&plugin_id, blocked)
}

/// 重新扫描磁盘上的插件目录。
#[tauri::command]
pub(crate) fn plugin_reload(manager: tauri::State<'_, Arc<PluginManager>>) -> Vec<String> {
    manager.reload()
}

fn parse_kind(kind: &str) -> Result<PluginKind, String> {
    match kind.to_ascii_lowercase().as_str() {
        "adapter" => Ok(PluginKind::Adapter),
        "detector" => Ok(PluginKind::Detector),
        "coupler" => Ok(PluginKind::Coupler),
        other => Err(format!("未知插件种类: {}", other)),
    }
}
