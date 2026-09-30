//! 适配器相关的 Tauri 命令。
//!
//! 这些命令的**名称、参数与返回值与改造前完全一致**，前端无需改动。
//! 变化发生在内部：不再直接操作 `AdapterManager`，而是通过插件管理器把调用
//! 派发给"适配类能力"的提供者（当前是内置的陶瓦适配器插件）。
//!
//! 这样做的好处是：将来接入第二个适配器（例如自研打洞或 ZeroTier）时，
//! 只需要在插件注册表里加一条记录，这里的命令一行都不用改。

use std::sync::Arc;
use std::time::Duration;
use serde_json::{json, Value};
use tauri::Emitter;

use crate::adapter::AdapterStatus;
use crate::assets::adapter::{
    current_platform, download_verified_package, fetch_manifest, pick_entry,
};
use crate::plugin::manager::PluginManager;
use crate::plugin::protocol::adapter_method;

/// 下载并安装内置适配器。
///
/// 流程：向资源服务器索取适配器清单 → 按当前平台选包 → 下载并校验 SHA256 →
/// 校验通过才解压 → 交给适配器插件启动。清单拉不到或校验不通过一律失败，
/// 不会执行未经验证的二进制。
#[tauri::command]
pub(crate) async fn download_adapter(
    window: tauri::Window,
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<String, String> {
    let adapter_dir = plugin_manager.adapter_dir()?.join("Terracotta");
    std::fs::create_dir_all(&adapter_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    window
        .emit("app-log", "[下载] 正在从资源服务器获取适配器校验清单...".to_string())
        .ok();

    let data_dir = plugin_manager.data_dir().to_path_buf();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    // 校验清单来自我们自己的资源服务器；拉不到即中止（fail-closed）
    let manifest = fetch_manifest(&data_dir, &client).await?;
    let platform = current_platform();
    let entry = pick_entry(&manifest, &platform)?;

    window
        .emit(
            "app-log",
            format!("[下载] 目标：{}（版本 {}），开始下载...", entry.file, entry.version),
        )
        .ok();
    window.emit("download-progress", 0u8).ok();

    let archive_path = download_verified_package(&adapter_dir, &client, entry, |pct| {
        let _ = window.emit("download-progress", pct);
    })
    .await?;

    window
        .emit("app-log", "[下载] SHA256 校验通过，开始解压...".to_string())
        .ok();

    let file = std::fs::File::open(&archive_path).map_err(|e| format!("打开下载文件失败: {}", e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(&adapter_dir)
        .map_err(|e| format!("解压失败: {}", e))?;

    std::fs::remove_file(&archive_path).map_err(|e| format!("删除安装包失败: {}", e))?;

    window
        .emit("app-log", "[下载] 陶瓦联机已安装完成，正在启动...".to_string())
        .ok();

    // 落地完成后交给适配器插件自己去启动与轮询，命令层不关心它怎么实现的。
    plugin_manager
        .adapter_invoke(adapter_method::INSTALL, None, json!({}))
        .await?;

    Ok("陶瓦联机已安装并启动".to_string())
}

/// 启动时初始化适配器。
#[tauri::command]
pub(crate) async fn adapter_startup_init(
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<String, String> {
    // `launch_all` 内部会轮询端口（最长约 12 秒），放到阻塞线程上执行，
    // 避免占用 IPC 线程。返回值与前端契约保持不变。
    let manager = plugin_manager.inner().clone();
    tokio::task::spawn_blocking(move || manager.adapter_launch_all())
        .await
        .map_err(|e| format!("适配器初始化任务失败: {}", e))?
}

/// 获取适配器状态。
#[tauri::command]
pub(crate) async fn get_adapter_status(
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<AdapterStatus, String> {
    let value = plugin_manager
        .adapter_invoke(adapter_method::STATUS, None, json!({}))
        .await?;
    serde_json::from_value(value).map_err(|e| format!("解析适配器状态失败: {}", e))
}

/// 获取陶瓦联机状态。
#[tauri::command]
pub(crate) async fn get_terracotta_state(
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<serde_json::Value, String> {
    let value = plugin_manager
        .adapter_invoke(adapter_method::STATUS, None, json!({}))
        .await?;
    Ok(value.get("state").cloned().unwrap_or(Value::Null))
}

/// 启动陶瓦联机（房主）。
#[tauri::command]
pub(crate) async fn start_terracotta_host(
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
    window: tauri::Window,
    room_code: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    window
        .emit("app-log", "[陶瓦] 准备房间并开始扫描本地服务器...".to_string())
        .ok();

    let result = plugin_manager
        .adapter_invoke(
            adapter_method::HOST_START,
            None,
            host_params(&room_code, &player_name),
        )
        .await?;

    emit_state_log(&window, &result);
    Ok(result)
}

/// 加入陶瓦联机（访客）。
#[tauri::command]
pub(crate) async fn start_terracotta_guest(
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
    window: tauri::Window,
    room_code: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    window
        .emit("app-log", format!("[陶瓦] 加入房间 {}...", room_code))
        .ok();

    let result = plugin_manager
        .adapter_invoke(
            adapter_method::JOIN,
            None,
            host_params(&room_code, &player_name),
        )
        .await?;

    emit_state_log(&window, &result);
    Ok(result)
}

/// 组装适配器所需上下文：房间信息 + 玩家身份。
///
/// 游戏信息与其他插件信息由 `PluginManager::broadcast_context` 另行推送，
/// 不重复塞进每次调用参数里。
fn host_params(room_code: &str, player_name: &str) -> serde_json::Value {
    json!({
        "room_code": room_code,
        "player_name": player_name,
        "role": "host",
    })
}

/// 把适配器返回的终态翻译成用户可读日志。
fn emit_state_log(window: &tauri::Window, result: &serde_json::Value) {
    let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("");
    match state {
        "host-ok" => {
            let room = result.get("room").and_then(|v| v.as_str()).unwrap_or("未知");
            window
                .emit("app-log", format!("[陶瓦] 房间已创建! 房间码: {}", room))
                .ok();
        }
        "guest-ok" => {
            let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("未知");
            window
                .emit("app-log", format!("[陶瓦] 已连接! 本地地址: {}", url))
                .ok();
        }
        "exception" => {
            let code = result
                .get("type")
                .and_then(|v| v.as_u64())
                .unwrap_or(99);
            window
                .emit("app-log", format!("[陶瓦] 异常退出 (代码: {})", code))
                .ok();
        }
        other => {
            window
                .emit("app-log", format!("[陶瓦] 未知状态: {}", other))
                .ok();
        }
    }
}
