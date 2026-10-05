//! 能力路由：给定「能力种类 + 游戏」，选出该由哪个插件来干，并给出理由。
//!
//! 路由必须是**可解释的**：当用户问"为什么这次用的是 B 而不是 A"，
//! 核心要能回答出具体规则命中了哪一条。因此每个候选都附带 `reason`。

use crate::plugin::game::GameRegistry;
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::kind_baseline;
use crate::plugin::registry::{PluginRegistry, PluginSource};

/// 外部插件自述 `priority` 的允许范围（仅作同档内微调）。
///
/// 不限制时，任何插件写 `priority: 10000` 就能越过"游戏画像显式指定"(1000)
/// 与"清单明确覆盖该游戏"(200) 两档，把自己变成全部游戏的首选适配器。
const EXTERNAL_PRIORITY_LIMIT: i32 = 50;

/// 一个候选插件及其入选理由。
#[derive(Debug, Clone)]
pub struct RouteCandidate {
    pub plugin_id: String,
    /// 面向用户/日志的可解释理由。
    pub reason: String,
    /// 排序分值（越大越优先）。
    pub score: i32,
}

/// 路由计划：候选按优先级从高到低排列，第一个即首选。
#[derive(Debug, Clone)]
pub struct RoutePlan {
    pub kind: PluginKind,
    pub game_id: Option<String>,
    pub candidates: Vec<RouteCandidate>,
}

impl RoutePlan {
    /// 首选插件 ID。
    pub fn primary(&self) -> Option<&str> {
        self.candidates.first().map(|c| c.plugin_id.as_str())
    }

    /// 回退链：首选之后的候选，按顺序依次尝试。
    pub fn fallbacks(&self) -> Vec<&str> {
        self.candidates
            .iter()
            .skip(1)
            .map(|c| c.plugin_id.as_str())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

/// 依据注册表与游戏画像生成路由计划。
///
/// 评分规则（从高到低）：
/// 1. 游戏画像显式指定的首选插件：`1000 - 序号`；
/// 2. 游戏画像指定的回退插件：`800 - 序号`；
/// 3. 首选插件清单里声明的 `fallback` 链：`600 - 序号`；
/// 4. 清单明确声明覆盖该游戏：`200`；
/// 5. 清单用 `*` 通配覆盖：`100`；
/// 6. 加上清单自带的 `priority` 作为同档内的微调
///    —— 外部插件被夹在 ±[`EXTERNAL_PRIORITY_LIMIT`] 内，只能微调、不能越档。
///
/// 另外候选必须先满足**能力基线**（[`kind_baseline`]）：选中了却拿不到权限的插件
/// 只会让调用失败。这条同时堵住"未验签插件靠清单自述当上首选适配器"——它默认
/// 拿不到基线权限，因而进不了候选。
pub fn plan(
    registry: &PluginRegistry,
    games: &GameRegistry,
    kind: PluginKind,
    game_id: Option<&str>,
) -> RoutePlan {
    let profile = game_id.and_then(|id| games.get(id));

    // 显式优先级链（来自游戏画像）。
    let mut preferred: Vec<String> = Vec::new();
    if let Some(p) = profile {
        if kind == PluginKind::Adapter {
            if let Some(first) = &p.preferred_adapter {
                preferred.push(first.clone());
            }
            preferred.extend(p.fallback_adapters.iter().cloned());
        }
    }

    // 首选插件自己声明的回退链。
    let declared_fallback: Vec<String> = preferred
        .first()
        .and_then(|id| registry.get(id))
        .map(|r| r.manifest.fallback.clone())
        .unwrap_or_default();

    let mut candidates = Vec::new();

    for record in registry.list() {
        if record.manifest.kind != kind || !record.enabled || !record.trust.is_runnable() {
            continue;
        }
        // 能力基线：连基线权限都没有的插件不参与路由（选了也只会失败）。
        // 未验签插件默认拿不到基线权限，于是天然不会成为首选。
        if !record.permissions.contains(kind_baseline(kind)) {
            continue;
        }
        let id = record.id();

        // 游戏限定：既不在显式链上，也不覆盖该游戏 → 直接排除。
        let covers = match game_id {
            Some(g) => record.manifest.covers_game(g),
            None => true,
        };
        let explicit_rank = preferred.iter().position(|p| p == id);
        let fallback_rank = declared_fallback.iter().position(|p| p == id);
        if !covers && explicit_rank.is_none() && fallback_rank.is_none() {
            continue;
        }

        let (mut score, reason) = match explicit_rank {
            Some(i) => (
                1000 - i as i32,
                format!("游戏画像 `{}` 显式指定的适配器", game_id.unwrap_or("-")),
            ),
            None => match fallback_rank {
                Some(i) => (600 - i as i32, "首选适配器清单声明的回退链".to_string()),
                None => {
                    let wildcard = record.manifest.games.iter().any(|g| g == "*");
                    if wildcard {
                        (100, "通过 `*` 通配覆盖全部游戏".to_string())
                    } else {
                        (
                            200,
                            format!("清单声明支持游戏 `{}`", game_id.unwrap_or("-")),
                        )
                    }
                }
            },
        };
        // 外部插件的 priority 是**清单自述**的，只允许同档微调
        score += match record.source {
            PluginSource::Builtin => record.manifest.priority,
            PluginSource::External => record
                .manifest
                .priority
                .clamp(-EXTERNAL_PRIORITY_LIMIT, EXTERNAL_PRIORITY_LIMIT),
        };

        candidates.push(RouteCandidate {
            plugin_id: id.to_string(),
            reason,
            score,
        });
    }

    // 分值降序，同分按 ID 升序，保证结果稳定可复现。
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.plugin_id.cmp(&b.plugin_id))
    });

    RoutePlan {
        kind,
        game_id: game_id.map(|s| s.to_string()),
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::crypto;
    use crate::plugin::manifest::PluginManifest;
    use crate::plugin::permission::{Permission, PermissionSet, TrustLevel};
    use crate::plugin::registry::{PluginRecord, PluginRegistry, PluginSource};

    fn record(json: serde_json::Value, dir: &std::path::Path, trust: TrustLevel) -> PluginRecord {
        let manifest = PluginManifest::parse(&json.to_string()).unwrap();
        let perms = PermissionSet::resolve(&manifest.permissions, trust, None);
        PluginRecord {
            manifest,
            source: PluginSource::External,
            dir: dir.to_path_buf(),
            trust,
            enabled: true,
            permissions: perms,
        }
    }

    fn registry_with(records: Vec<PluginRecord>) -> PluginRegistry {
        let tmp = std::env::temp_dir().join(format!("mclink-router-{}", crypto::random_hex(6)));
        let mut reg = PluginRegistry::new(&tmp);
        reg.reload(records);
        reg
    }

    fn adapter(id: &str, games: &[&str], priority: i32) -> serde_json::Value {
        // 真实适配器必须声明 net_connect_any（HOST_START/JOIN/PROBE 在
        // `required_for` 里就要求它），否则连能力基线都过不了，压根不会被路由选中。
        serde_json::json!({
            "id": id,
            "name": id,
            "version": "1.0.0",
            "kind": "adapter",
            "games": games,
            "priority": priority,
            "permissions": ["net_listen_local", "net_connect_any"]
        })
    }

    #[test]
    fn wildcard_candidate_is_kept_for_any_game() {
        let tmp = std::env::temp_dir();
        let reg = registry_with(vec![record(
            adapter("dev.a.one", &["*"], 0),
            &tmp,
            TrustLevel::Official,
        )]);
        let games = GameRegistry::with_builtins();
        let p = plan(&reg, &games, PluginKind::Adapter, Some("terraria"));
        assert_eq!(p.primary(), Some("dev.a.one"));
        assert!(p.candidates[0].reason.contains("通配"));
    }

    #[test]
    fn game_specific_plugin_outranks_wildcard() {
        let tmp = std::env::temp_dir();
        let reg = registry_with(vec![
            record(
                adapter("dev.a.generic", &["*"], 0),
                &tmp,
                TrustLevel::Official,
            ),
            record(
                adapter("dev.a.terraria", &["terraria"], 0),
                &tmp,
                TrustLevel::Official,
            ),
        ]);
        let games = GameRegistry::with_builtins();
        let p = plan(&reg, &games, PluginKind::Adapter, Some("terraria"));
        assert_eq!(p.primary(), Some("dev.a.terraria"));
        assert!(p.fallbacks().contains(&"dev.a.generic"));
    }

    #[test]
    fn non_covering_plugin_is_excluded() {
        let tmp = std::env::temp_dir();
        let reg = registry_with(vec![record(
            adapter("dev.a.mc", &["minecraft-java"], 5),
            &tmp,
            TrustLevel::Official,
        )]);
        let games = GameRegistry::with_builtins();
        let p = plan(&reg, &games, PluginKind::Adapter, Some("terraria"));
        assert!(p.is_empty());
    }

    #[test]
    fn disabled_and_blocked_plugins_are_ignored() {
        let tmp = std::env::temp_dir();
        let mut blocked = record(
            adapter("dev.a.blocked", &["*"], 0),
            &tmp,
            TrustLevel::Blocked,
        );
        blocked.enabled = true;
        let reg = registry_with(vec![blocked]);
        let games = GameRegistry::with_builtins();
        let p = plan(&reg, &games, PluginKind::Adapter, None);
        assert!(p.is_empty());
    }

    #[test]
    fn priority_breaks_ties_deterministically() {
        let tmp = std::env::temp_dir();
        let reg = registry_with(vec![
            record(adapter("dev.a.low", &["*"], 0), &tmp, TrustLevel::Official),
            record(
                adapter("dev.a.high", &["*"], 50),
                &tmp,
                TrustLevel::Official,
            ),
        ]);
        let games = GameRegistry::with_builtins();
        let p = plan(&reg, &games, PluginKind::Adapter, None);
        assert_eq!(p.primary(), Some("dev.a.high"));
    }

    #[test]
    fn only_matching_kind_is_returned() {
        let tmp = std::env::temp_dir();
        let mut detector = adapter("dev.d.scan", &["*"], 100);
        detector["kind"] = serde_json::json!("detector");
        detector["permissions"] = serde_json::json!(["game_scan"]);
        let reg = registry_with(vec![record(detector, &tmp, TrustLevel::Official)]);
        let games = GameRegistry::with_builtins();
        assert!(plan(&reg, &games, PluginKind::Adapter, None).is_empty());
        assert_eq!(
            plan(&reg, &games, PluginKind::Detector, None).primary(),
            Some("dev.d.scan")
        );
    }

    #[test]
    fn permission_type_is_used() {
        // 防止 `Permission` 被误删导致权限模型失去类型约束。
        let _ = Permission::GameScan;
    }

    /// 能力基线：拿不到基线权限的插件不参与路由。
    ///
    /// 未验签插件默认只有最小权限（不含 `net_connect_any`），因此不会成为首选适配器
    /// ——这比"分值排序"更可靠，因为权限不来自清单自述。
    #[test]
    fn plugin_without_baseline_permission_is_not_routed() {
        let tmp = std::env::temp_dir();
        let reg = registry_with(vec![record(
            adapter("dev.a.weak", &["*"], 0),
            &tmp,
            TrustLevel::Unsigned,
        )]);
        let games = GameRegistry::with_builtins();
        assert!(plan(&reg, &games, PluginKind::Adapter, None).is_empty());
    }

    /// 外部插件的自述 `priority` 只能同档微调：写多大都压不过"明确覆盖该游戏"。
    #[test]
    fn external_priority_is_clamped_to_same_tier() {
        let tmp = std::env::temp_dir();
        let reg = registry_with(vec![
            record(
                adapter("dev.a.greedy", &["*"], 10_000),
                &tmp,
                TrustLevel::Verified,
            ),
            record(
                adapter("dev.a.specific", &["terraria"], 0),
                &tmp,
                TrustLevel::Verified,
            ),
        ]);
        let games = GameRegistry::with_builtins();
        let p = plan(&reg, &games, PluginKind::Adapter, Some("terraria"));
        assert_eq!(p.primary(), Some("dev.a.specific"));

        let greedy = p
            .candidates
            .iter()
            .find(|c| c.plugin_id == "dev.a.greedy")
            .expect("通配插件仍应是候选（只是排在后面）");
        assert_eq!(greedy.score, 100 + EXTERNAL_PRIORITY_LIMIT);
    }
}
