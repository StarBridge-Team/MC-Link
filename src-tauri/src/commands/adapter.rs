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
use futures_util::StreamExt;
use serde_json::{json, Value};
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

use crate::adapter::AdapterStatus;
use crate::plugin::manager::PluginManager;
use crate::plugin::protocol::adapter_method;

/// 下载并安装内置适配器。
#[tauri::command]
pub(crate) async fn download_adapter(
    window: tauri::Window,
    plugin_manager: tauri::State<'_, Arc<PluginManager>>,
) -> Result<String, String> {
    let adapter_dir = plugin_manager.adapter_dir()?.join("Terracotta");
    std::fs::create_dir_all(&adapter_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let url = "https://gitee.com/burningtnt/Terracotta/releases/download/v0.4.2/terracotta-0.4.2-windows-x86_64-pkg.tar.gz";
    let filename = "terracotta-0.4.2-windows-x86_64-pkg.tar.gz";
    let archive_path = adapter_dir.join(filename);

    window.emit("app-log", "[下载] 开始下载陶瓦联机...").ok();
    window.emit("download-progress", 0u8).ok();

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载失败 (HTTP {})", response.status()));
    }

    let tmp_path = adapter_dir.join(format!("{}.tmp", filename));
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| format!("创建临时文件失败: {}", e))?;

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取数据失败: {}", e))?;
        downloaded += chunk.len() as u64;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入文件失败: {}", e))?;
        if total > 0 {
            let pct = (downloaded as f64 / total as f64 * 100.0) as u8;
            let _ = window.emit("download-progress", pct);
        }
    }

    file.flush().await.map_err(|e| format!("刷新文件失败: {}", e))?;
    drop(file);

    std::fs::rename(&tmp_path, &archive_path).map_err(|e| format!("重命名文件失败: {}", e))?;

    window
        .emit("app-log", format!("[下载] 下载完成 ({} bytes)，开始解压...", downloaded))
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
