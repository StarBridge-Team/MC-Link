//! 联机的**只读**能力：扫描本机游戏、读最近一次扫描结果。
//!
//! 建房 / 加入 / 断开已迁到 `mgr::room`（见 `commands/room.rs`）：那些是会话级操作，
//! 需要 `sessionId` 与状态机，放在插件层会让"多房间"无处安放。
//!
//! 保留在本文件的两个命令都是无状态的查询，供联机页与首页共用。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::plugin::manager::PluginManager;
use crate::plugin::manifest::PluginKind;
use crate::plugin::protocol::detector_method;

/// 扫描本机游戏实例（经检测器插件），返回发现到的游戏数组（可能为空）。
///
/// 房主模式据此展示「扫到的所有游戏」卡片；空数组即「没扫到游戏」，前端呈现空状态。
#[tauri::command]
pub async fn connect_scan(
    manager: tauri::State<'_, Arc<PluginManager>>,
    game_id: Option<String>,
) -> Result<Value, String> {
    let res = manager
        .invoke(
            PluginKind::Detector,
            game_id.as_deref(),
            detector_method::SCAN,
            json!({}),
        )
        .await
        .map_err(|e| e.message)?;
    // 统一成数组返回，便于前端 v-for。
    let arr = match res.1 {
        Value::Array(a) => a,
        other => vec![other],
    };
    Ok(json!(arr))
}

/// 取最近一次检测器扫描到的游戏，**不重新扫描**。
///
/// # 为什么需要它
///
/// 首页靠 `local-game-found` 事件驱动（扫描在应用打开时执行一次）。事件是一次性广播、
/// **没有补发**：监听器晚一步（页面重载、启动竞态、扫描失败一次）首页就会永远停在
/// "正在寻找本地游戏…"。这个命令让首页在挂载时主动拉一次兜底。
#[tauri::command]
pub async fn connect_local_games(
    manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<Value, String> {
    Ok(json!(manager.local_games()))
}
