// MC Link revamp 适配器 - 后端 API 客户端
// 中央服务器 103.36.221.57:8000

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

fn get_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("创建 HTTP 客户端失败")
    })
}

fn get_account_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(90))
            .build()
            .expect("创建 HTTP 客户端失败")
    })
}

const CENTRAL_URL: &str = "http://103.36.221.57:8000";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct RevampNode {
    pub ip: String,
    pub port: String,
    pub name: String,
    pub last_update: String,
    pub contributor: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct RevampRoom {
    pub room_id: String,
    pub password: String,
    pub node_ip: String,
    pub node_port: String,
    pub creator_name: String,
    pub players: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct RevampVersion {
    pub version: String,
    pub download_url: String,
    pub update_info: String,
    pub release_date: String,
}

/// 检测中央服务器是否在线
pub(crate) async fn ping_central() -> Result<bool, String> {
    let client = get_http_client();

    let resp = client
        .get(format!("{}/ping", CENTRAL_URL))
        .send()
        .await
        .map_err(|_| "无法连接中央服务器".to_string())?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    Ok(data.get("status").and_then(|s| s.as_str()) == Some("ok"))
}

/// 获取节点列表
pub(crate) async fn get_nodes() -> Result<Vec<RevampNode>, String> {
    let client = get_http_client();

    let nodes = client
        .get(format!("{}/nodes", CENTRAL_URL))
        .send()
        .await
        .map_err(|e| format!("获取节点列表失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析节点列表失败: {}", e))?;

    Ok(nodes)
}

/// 获取房间列表
pub(crate) async fn get_rooms() -> Result<Vec<RevampRoom>, String> {
    let client = get_http_client();

    let rooms = client
        .get(format!("{}/rooms", CENTRAL_URL))
        .send()
        .await
        .map_err(|e| format!("获取房间列表失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析房间列表失败: {}", e))?;

    Ok(rooms)
}

/// 创建房间
pub(crate) async fn create_room(
    room_id: &str,
    password: &str,
    node_ip: &str,
    node_port: &str,
    creator_name: &str,
) -> Result<serde_json::Value, String> {
    let client = get_http_client();

    let body = serde_json::json!({
        "room_id": room_id,
        "password": password,
        "node_ip": node_ip,
        "node_port": node_port,
        "creator_name": creator_name,
    });

    let resp = client
        .post(format!("{}/create_room", CENTRAL_URL))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("创建房间失败: {}", e))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    Ok(data)
}

/// 加入房间
pub(crate) async fn join_room(
    room_id: &str,
    password: &str,
    player_name: &str,
) -> Result<serde_json::Value, String> {
    let client = get_http_client();

    let body = serde_json::json!({
        "room_id": room_id,
        "password": password,
        "player_name": player_name,
    });

    let resp = client
        .post(format!("{}/join_room", CENTRAL_URL))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("加入房间失败: {}", e))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    Ok(data)
}

/// 离开房间
pub(crate) async fn leave_room(
    room_id: &str,
    player_name: &str,
) -> Result<serde_json::Value, String> {
    let client = get_http_client();

    let body = serde_json::json!({
        "room_id": room_id,
        "player_name": player_name,
    });

    let resp = client
        .post(format!("{}/leave_room", CENTRAL_URL))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("离开房间失败: {}", e))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    Ok(data)
}

/// 检测房间是否存在
pub(crate) async fn room_exists(room_id: &str) -> Result<bool, String> {
    let rooms = get_rooms().await?;
    Ok(rooms.iter().any(|r| r.room_id == room_id))
}

/// 获取版本信息
pub(crate) async fn get_version() -> Result<RevampVersion, String> {
    let client = get_http_client();

    let ver = client
        .get(format!("{}/version", CENTRAL_URL))
        .send()
        .await
        .map_err(|e| format!("获取版本信息失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析版本信息失败: {}", e))?;

    Ok(ver)
}

// ===== 账号服务端 43.248.79.27:14117 =====

const ACCOUNT_URL: &str = "http://43.248.79.27:14117";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct AccountResult {
    pub status: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub username: String,
}

/// 注册
pub(crate) async fn register_account(username: &str, password: &str) -> Result<AccountResult, String> {
    let client = get_account_http_client();

    let body = serde_json::json!({ "username": username, "password": password });

    let resp = client
        .post(format!("{}/register", ACCOUNT_URL))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;
    let data: AccountResult = serde_json::from_str(&text)
        .map_err(|e| format!("解析响应失败 ({}): {}", e, &text))?;

    Ok(data)
}

/// 登录
pub(crate) async fn login_account(username: &str, password: &str) -> Result<AccountResult, String> {
    let client = get_account_http_client();

    let body = serde_json::json!({ "username": username, "password": password });

    let resp = client
        .post(format!("{}/login", ACCOUNT_URL))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;
    let data: AccountResult = serde_json::from_str(&text)
        .map_err(|e| format!("解析响应失败 ({}): {}", e, &text))?;

    Ok(data)
}
