//! 联机页的静态数据装配：可用适配器（含各自声明的字段）+ 已扫到的本地游戏。
//!
//! # 为什么单独一个文件
//!
//! 它只做一件事：对**每个**候选适配器各发一次 `adapter.init`，把返回值翻译成前端契约。
//! 留在 `room.rs` 里会把它撑过行数上限，也会让"会话编排"和"列表装配"混在一起——
//! 两者唯一的共同点是都需要 `PluginManager`。
//!
//! # 为什么必须定向调用
//!
//! 走 `PluginManager::invoke` 会重新做一次路由，于是**所有**适配器都拿到排第一的那个
//! 的字段。多适配器并存时这会让用户选第二个适配器却看到第一个的表单。

use serde_json::{json, Value};

use crate::plugin::manager::PluginManager;
use crate::plugin::manifest::PluginKind;
use crate::plugin::permission::required_for;
use crate::plugin::protocol::adapter_method;

use super::helpers::normalize_fields;

/// 装配 `room_prepare` 的返回值。
pub(super) async fn prepare(plugins: &PluginManager, game_id: Option<&str>) -> Value {
    let plan = plugins.plan(PluginKind::Adapter, game_id);
    let mut adapters = Vec::new();

    for candidate in &plan.candidates {
        let Some(record) = plugins.get(&candidate.plugin_id) else {
            continue;
        };
        let init = plugins
            .invoke_plugin(
                &candidate.plugin_id,
                required_for(PluginKind::Adapter, adapter_method::INIT),
                adapter_method::INIT,
                json!({}),
            )
            .await
            .ok();

        // 适配器自己知道"引擎有没有就位"（首次使用要下载），因此以它的 `ready` 为准；
        // 老适配器没有该字段时退回"启用且未被拉黑"。
        let runnable = record.enabled && record.trust.is_runnable();
        let declared_ready = init
            .as_ref()
            .and_then(|value| value.get("ready"))
            .and_then(|value| value.as_bool())
            .unwrap_or(runnable);

        adapters.push(json!({
            "pluginId": record.manifest.id,
            "name": record.manifest.name,
            "trust": record.trust,
            "ready": runnable && declared_ready,
            "hostFields": normalize_fields(init.as_ref().and_then(|v| v.get("host_fields").cloned())),
            "joinFields": normalize_fields(init.as_ref().and_then(|v| v.get("join_fields").cloned())),
            "engine": init.as_ref().and_then(|v| v.get("engine").cloned()),
        }));
    }

    json!({
        "adapters": adapters,
        "games": plugins.local_games(),
    })
}
