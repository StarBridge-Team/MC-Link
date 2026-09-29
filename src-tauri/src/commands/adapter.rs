use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use crate::adapter::AdapterManager;
use crate::terracotta_client::TerracottaClient;

#[tauri::command]
pub(crate) async fn download_adapter(window: tauri::Window, adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    let adapter_dir = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        mgr.adapter_dir.join("Terracotta")
    };
    std::fs::create_dir_all(&adapter_dir)
        .map_err(|e| format!("创建目录失败: {}", e))?;

    let url = "https://gitee.com/burningtnt/Terracotta/releases/download/v0.4.2/terracotta-0.4.2-windows-x86_64-pkg.tar.gz";
    let filename = "terracotta-0.4.2-windows-x86_64-pkg.tar.gz";
    let archive_path = adapter_dir.join(filename);

    window.emit("app-log", "[下载] 开始下载陶瓦联机...").ok();
    window.emit("download-progress", 0u8).ok();

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;

    let response = client.get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载失败 (HTTP {})", response.status()));
    }

    // 创建临时文件用于流式写入
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
        file.write_all(&chunk).await.map_err(|e| format!("写入文件失败: {}", e))?;
        if total > 0 {
            let pct = (downloaded as f64 / total as f64 * 100.0) as u8;
            let _ = window.emit("download-progress", pct);
        }
    }

    file.flush().await.map_err(|e| format!("刷新文件失败: {}", e))?;
    drop(file);

    // 重命名临时文件为正式文件
    std::fs::rename(&tmp_path, &archive_path)
        .map_err(|e| format!("重命名文件失败: {}", e))?;

    window.emit("app-log", format!("[下载] 下载完成 ({} bytes)，开始解压...", downloaded)).ok();

    let file = std::fs::File::open(&archive_path)
        .map_err(|e| format!("打开下载文件失败: {}", e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(&adapter_dir)
        .map_err(|e| format!("解压失败: {}", e))?;

    std::fs::remove_file(&archive_path)
        .map_err(|e| format!("删除安装包失败: {}", e))?;

    window.emit("app-log", "[下载] 陶瓦联机已安装完成，正在启动...".to_string()).ok();

    let manager = match adapter_manager.lock() {
        Ok(m) => m,
        Err(e) => return Err(format!("内部错误: {}", e)),
    };
    manager.launch_terracotta_after_download();

    Ok("陶瓦联机已安装并启动".to_string())
}

#[tauri::command]
pub(crate) fn adapter_startup_init(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<String, String> {
    let manager = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
    manager.launch_all();
    let status = manager.get_status();
    if status.running {
        Ok("适配器已就绪".to_string())
    } else if status.installed {
        Ok("适配器已安装，正在启动...".to_string())
    } else {
        Ok("适配器正在安装...".to_string())
    }
}

#[tauri::command]
pub(crate) fn get_adapter_status(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<crate::adapter::AdapterStatus, String> {
    let manager = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
    Ok(manager.get_status())
}

#[tauri::command]
pub(crate) async fn get_terracotta_state(adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        let p = *mgr.terracotta_port.lock().map_err(|e| format!("内部错误: {}", e))?;
        p
    };
    match port {
        Some(p) => {
            let client = TerracottaClient::new(p);
            let state = client.get_state().await?;
            serde_json::to_value(&state).map_err(|e| format!("序列化失败: {}", e))
        }
        None => {
            Ok(serde_json::json!({"state": "waiting", "index": 0}))
        }
    }
}

#[tauri::command]
pub(crate) async fn start_terracotta_host(
    adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>,
    window: tauri::Window,
    room_code: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        mgr.ensure_running()?
    };
    let client = TerracottaClient::new(port);

    window.emit("app-log", "[陶瓦] 重置状态...".to_string()).ok();
    client.set_ide().await?;

    window.emit("app-log", "[陶瓦] 开始扫描本地Minecraft服务器...".to_string()).ok();
    let room_opt = if room_code.is_empty() { None } else { Some(room_code.as_str()) };
    let player_opt = if player_name.is_empty() { None } else { Some(player_name.as_str()) };
    client.start_host(room_opt, player_opt, &[]).await?;

    window.emit("app-log", "[陶瓦] 等待房间创建...".to_string()).ok();
    let result = client.poll_state(60, 500).await?;

    let state_str = &result.state;
    if state_str == "host-ok" {
        let room_code = result.room.as_deref().unwrap_or("未知");
        window.emit("app-log", format!("[陶瓦] 房间已创建! 房间码: {}", room_code)).ok();
    } else if state_str == "exception" {
        let excode = result.exception_type.unwrap_or(99);
        window.emit("app-log", format!("[陶瓦] 异常退出 (代码: {})", excode)).ok();
    } else {
        window.emit("app-log", format!("[陶瓦] 未知状态: {}", state_str)).ok();
    }

    serde_json::to_value(&result).map_err(|e| format!("序列化失败: {}", e))
}

#[tauri::command]
pub(crate) async fn start_terracotta_guest(
    adapter_manager: tauri::State<'_, Arc<Mutex<AdapterManager>>>,
    window: tauri::Window,
    room_code: String,
    player_name: String,
) -> Result<serde_json::Value, String> {
    let port = {
        let mgr = adapter_manager.lock().map_err(|e| format!("内部错误: {}", e))?;
        mgr.ensure_running()?
    };
    let client = TerracottaClient::new(port);

    window.emit("app-log", "[陶瓦] 重置状态...".to_string()).ok();
    client.set_ide().await?;

    window.emit("app-log", format!("[陶瓦] 加入房间 {}...", room_code)).ok();
    let player_opt = if player_name.is_empty() { None } else { Some(player_name.as_str()) };
    client.start_guest(&room_code, player_opt, &[]).await?;

    window.emit("app-log", "[陶瓦] 等待连接...".to_string()).ok();
    let result = client.poll_state(60, 500).await?;

    let state_str = &result.state;
    if state_str == "guest-ok" {
        let url = result.url.as_deref().unwrap_or("未知");
        window.emit("app-log", format!("[陶瓦] 已连接! 本地地址: {}", url)).ok();
    } else if state_str == "exception" {
        let excode = result.exception_type.unwrap_or(99);
        window.emit("app-log", format!("[陶瓦] 异常退出 (代码: {})", excode)).ok();
    } else {
        window.emit("app-log", format!("[陶瓦] 未知状态: {}", state_str)).ok();
    }

    serde_json::to_value(&result).map_err(|e| format!("序列化失败: {}", e))
}
