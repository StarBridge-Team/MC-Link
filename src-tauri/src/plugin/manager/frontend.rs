//! 前端可见的查询与广播：游戏画像、路由计划、本地游戏缓存、插件事件。
//!
//! 这些方法与"调度中枢"本身无关（既不拉起进程，也不做权限判定），
//! 单独成文件是为了让 [`super::PluginManager`] 只保留生命周期与调度逻辑。

use serde_json::{json, Value};
use tauri::Emitter;

use crate::plugin::capability::Provider;
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::required_for;
use crate::plugin::protocol::{detector_method, event_topic, LocalGameFound};
use crate::plugin::router::{self, RoutePlan};

use super::PluginManager;

/// 一个插件的运行期状态。
#[derive(Debug, Clone, Default)]
pub struct PluginHealth {
    /// 是否可用（内置=可用；外部=已握手且会话未关闭）。
    pub alive: bool,
    /// 会话建立时间（毫秒时间戳）；内置插件或未连接时为 `None`。
    pub connected_at: Option<i64>,
}

impl PluginManager {
    /// 网关运行状态（不回传令牌）。
    pub fn gateway_info(&self) -> Value {
        let gateway = self.inner.gateway.read().ok().and_then(|g| g.clone());
        match gateway {
            Some(g) => json!({
                "running": true,
                "port": g.port(),
                "endpoint": g.endpoint(),
                "externalPlugins": self
                    .inner
                    .providers
                    .read()
                    .map(|m| m.values().filter(|p| matches!(p, Provider::Remote(_))).count())
                    .unwrap_or(0),
            }),
            None => json!({ "running": false, "port": Value::Null }),
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        self.inner
            .warnings
            .read()
            .map(|w| w.clone())
            .unwrap_or_default()
    }

    /// 某个插件当前的运行期状态。
    ///
    /// 清单里的 `enabled` 只说明"允许运行"，不代表"正在运行"——外部插件要先被拉起、
    /// 完成握手才可用。界面需要这个区别来解释"装了但没反应"。
    pub fn health(&self, plugin_id: &str) -> PluginHealth {
        match self
            .inner
            .providers
            .read()
            .ok()
            .and_then(|m| m.get(plugin_id).cloned())
        {
            // 内置插件没有会话，只要它被调用就一定可用
            Some(Provider::Builtin(_)) => PluginHealth {
                alive: true,
                connected_at: None,
            },
            Some(Provider::Remote(session)) if !session.is_closed() => PluginHealth {
                alive: true,
                connected_at: Some(session.connected_at),
            },
            _ => PluginHealth {
                alive: false,
                connected_at: None,
            },
        }
    }

    // ---------------------------------------------------------------- 游戏画像

    pub fn games(&self) -> Vec<crate::plugin::game::GameProfile> {
        self.inner
            .games
            .read()
            .map(|g| g.all().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------- 路由

    pub fn plan(&self, kind: PluginKind, game_id: Option<&str>) -> RoutePlan {
        let registry = match self.inner.registry.read() {
            Ok(r) => r,
            Err(_) => {
                return RoutePlan {
                    kind,
                    game_id: game_id.map(|s| s.to_string()),
                    candidates: Vec::new(),
                };
            }
        };
        let games = match self.inner.games.read() {
            Ok(g) => g,
            Err(_) => {
                return RoutePlan {
                    kind,
                    game_id: game_id.map(|s| s.to_string()),
                    candidates: Vec::new(),
                };
            }
        };
        router::plan(&registry, &games, kind, game_id)
    }

    /// 给定游戏 ID，返回路由计划首选的适配器展示名（无则空串）。
    fn recommended_adapter_name(&self, game_id: &str) -> String {
        let plan = self.plan(
            PluginKind::Adapter,
            if game_id.is_empty() { None } else { Some(game_id) },
        );
        if let Some(pid) = plan.primary() {
            if let Ok(reg) = self.inner.registry.read() {
                if let Some(rec) = reg.get(pid) {
                    return rec.manifest.name.clone();
                }
            }
        }
        String::new()
    }

    // ---------------------------------------------------------------- 本地游戏

    /// 记录最近一次扫描结果，供前端主动拉取。
    pub(super) fn remember_local_games(&self, games: &[LocalGameFound]) {
        self.inner.local_games.set(games);
    }

    /// 读取最近一次扫描结果（**不重新扫描**）。
    pub fn local_games(&self) -> Vec<LocalGameFound> {
        self.inner.local_games.get()
    }

    /// 应用打开时调用：遍历所有【已启用且可运行】的检测类(detector)插件，
    /// 逐个触发 `detector.scan`，把发现的本地游戏回传前端首页。
    ///
    /// 没有实现扫描能力的检测器时返回空列表——这是正常的：扫描能力本就由插件提供，
    /// 核心只负责编排（见模块文档的职责边界）。
    pub async fn scan_local_games(&self) -> Vec<LocalGameFound> {
        // 先收集检测器清单（释放读锁后再逐个 invoke，避免持锁调用）。
        let detectors: Vec<(String, String)> = match self.inner.registry.read() {
            Ok(reg) => reg
                .by_kind(PluginKind::Detector)
                .iter()
                .map(|r| (r.manifest.id.clone(), r.manifest.name.clone()))
                .collect(),
            Err(_) => Vec::new(),
        };

        let mut found = Vec::new();
        for (id, name) in detectors {
            // 定向调用**这个**检测器。
            //
            // 不能走 `invoke(Detector, ..)`：那会重新算路由计划并对**全部候选**依次
            // 尝试，N 个检测器就退化成 N² 次扫描（每次扫描含 3 秒 UDP 多播）。
            // 这里已经拿到确切的插件 id，直接用它。
            match self
                .invoke_plugin(
                    &id,
                    required_for(PluginKind::Detector, detector_method::SCAN),
                    detector_method::SCAN,
                    json!({}),
                )
                .await
            {
                Ok(raw) => {
                    // `detector.scan` 的契约是返回**数组**（可能有多台服务器），
                    // 这里逐个展开。早期版本把对象取字段，于是 `process` 实际拿到的是
                    // `id`（`minecraft-java`）而不是 MOTD——首页与联机页显示的会是同一个
                    // 游戏却写出两个不同的名字。
                    let entries = match raw {
                        Value::Array(a) => a,
                        other => vec![other],
                    };
                    for value in entries {
                        let game_id = value
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let process = value
                            .get("process")
                            .and_then(|v| v.as_str())
                            .or_else(|| value.get("id").and_then(|v| v.as_str()))
                            .unwrap_or("")
                            .to_string();
                        let game_name = value
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let adapter = self.recommended_adapter_name(&game_id);
                        let item = LocalGameFound {
                            process,
                            game_name,
                            scanner: name.clone(),
                            adapter,
                        };
                        found.push(item.clone());
                        // 同时走既有 detector.found 契约通道，保持插件事件一致。
                        self.emit(json!({
                            "topic": event_topic::DETECTOR_FOUND,
                            "plugin": id,
                            "game": serde_json::to_value(&item).unwrap_or(serde_json::Value::Null),
                        }));
                    }
                }
                Err(e) => {
                    eprintln!("[动作] 检测器 {} 扫描失败: {}", id, e.message);
                }
            }
        }

        // 记下这次结果，供前端主动拉取（事件是一次性广播，补发不了）。
        self.remember_local_games(&found);
        found
    }

    // ---------------------------------------------------------------- 上下文广播

    /// 向所有已就绪插件广播上下文（游戏信息 / 房间信息 / 其他插件信息）。
    ///
    /// `members` 也要发：适配器需要知道"房间里都有谁"才能做成员同步；
    /// 早先漏了它，插件只能从 `adapter.status` 自己数，多适配器场景下对不上。
    pub fn broadcast_context(&self, context: &crate::plugin::protocol::PluginContext) {
        let peers = self.peer_summaries();
        let payload = json!({
            "role": context.role,
            "game": context.game,
            "room": context.room,
            "members": context.members,
            "peers": peers,
        });
        let providers: Vec<Provider> = self
            .inner
            .providers
            .read()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        for provider in providers {
            if let Provider::Remote(session) = &provider {
                let _ = session.notify(
                    crate::plugin::protocol::core_method::CONTEXT_UPDATE,
                    payload.clone(),
                );
            }
        }
    }

    /// 汇总其他插件的能力信息，供适配类插件决定如何协作。
    ///
    /// 这里是**信息面**（"本机还有谁在参与"），因此不按 `peer_message` 过滤——
    /// 没声明该权限的插件依然可能占用端口、拉起游戏，适配器需要知道它们存在。
    /// 可**调用**的插件清单是另一件事，见 `core.peer.list`（只列声明了
    /// `peer_message` 的插件）。
    pub fn peer_summaries(&self) -> Vec<crate::plugin::protocol::PeerPluginInfo> {
        self.list()
            .into_iter()
            .filter(|r| r.enabled && r.trust.is_runnable())
            .map(|r| crate::plugin::protocol::PeerPluginInfo {
                plugin_id: r.manifest.id.clone(),
                kind: r.manifest.kind.as_str().to_string(),
                games: r.manifest.games.clone(),
                endpoints: Vec::new(),
                version: r.manifest.version.clone(),
                alive: self
                    .inner
                    .providers
                    .read()
                    .ok()
                    .and_then(|m| m.get(r.manifest.id.as_str()).map(|p| p.is_available()))
                    .unwrap_or(false),
            })
            .collect()
    }

    // ---------------------------------------------------------------- 前端事件

    /// 向前端广播 `plugin-event`（未绑定 AppHandle 时静默丢弃）。
    pub fn emit(&self, payload: Value) {
        let app = self.inner.app.read().ok().and_then(|g| g.clone());
        if let Some(app) = app {
            let _ = app.emit("plugin-event", payload);
        }
    }
}
