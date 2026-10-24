//! 房间相关命令：**薄适配层**。
//!
//! 这里只做参数转交，不含任何编排逻辑。一个命令 = 一次用户可感知的意图，
//! 前端一个 Pinia Action 对应一个命令，中间不再有"拼装"步骤。
//!
//! 变更类命令（start/join/leave）**返回变更后的快照**：发起方不必等自己的事件，
//! 事件只用来通知其它窗口。

use std::sync::Arc;

use serde_json::Value;

use crate::core::error::AppResult;
use crate::domain::room::RoomSnapshot;
use crate::mgr::room::RoomMgr;

/// 联机页初始化：一次拿齐适配器（含字段声明）与已扫到的本地游戏。
#[tauri::command]
pub(crate) async fn room_prepare(
    room: tauri::State<'_, Arc<RoomMgr>>,
    game_id: Option<String>,
) -> AppResult<Value> {
    let room = (*room).clone();
    Ok(room.prepare(game_id.as_deref()).await)
}

/// 以房主身份建房。立即返回 `sessionId` 与首个快照，后续进度走事件。
#[tauri::command]
pub(crate) fn room_start_host(
    room: tauri::State<'_, Arc<RoomMgr>>,
    adapter_id: String,
    game_id: Option<String>,
    fields: Value,
    player_name: String,
) -> AppResult<RoomSnapshot> {
    room.start_host(adapter_id, game_id, fields, player_name)
}

/// 以成员身份加入房间。
#[tauri::command]
pub(crate) fn room_join(
    room: tauri::State<'_, Arc<RoomMgr>>,
    adapter_id: String,
    game_id: Option<String>,
    fields: Value,
    player_name: String,
) -> AppResult<RoomSnapshot> {
    room.join(adapter_id, game_id, fields, player_name)
}

/// 断开并结束会话。等适配器收尾完成后返回终态快照。
#[tauri::command]
pub(crate) async fn room_leave(
    room: tauri::State<'_, Arc<RoomMgr>>,
    session_id: String,
) -> AppResult<RoomSnapshot> {
    let room = (*room).clone();
    room.leave(&session_id).await
}

/// 向适配器要一次实时状态并合并（成员/端口/阶段）。
///
/// 房间窗口低频轮询它；只在真的变化时才会推 `mclink://room/state`。
#[tauri::command]
pub(crate) async fn room_refresh(
    room: tauri::State<'_, Arc<RoomMgr>>,
    session_id: String,
) -> AppResult<RoomSnapshot> {
    let room = (*room).clone();
    room.refresh(&session_id).await
}

/// 拉单个会话的当前快照。窗口挂载时用它拿权威基线。
#[tauri::command]
pub(crate) fn room_snapshot(
    room: tauri::State<'_, Arc<RoomMgr>>,
    session_id: String,
) -> AppResult<RoomSnapshot> {
    room.snapshot(&session_id)
}

/// 拉所有进行中的会话。主窗口的房间列表用它。
#[tauri::command]
pub(crate) fn room_list(room: tauri::State<'_, Arc<RoomMgr>>) -> Vec<RoomSnapshot> {
    room.list()
}
