// MC Link 账号 API 客户端 — 桌面登录流程
// 通过 HTTP API 调用本地中央服务器完成桌面登录

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use base64::Engine;

fn get_http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("创建 HTTP 客户端失败")
    })
}

/// 公开 HTTP 客户端引用（供 commands 模块使用）
pub fn get_http_client_ref() -> &'static reqwest::blocking::Client {
    get_http_client()
}

/// 本地中央服务器地址
const API_BASE: &str = "http://localhost:3456";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AccountResponse {
    pub status: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub code: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DesktopInitResponse {
    pub code: i32,
    pub session: Option<String>,
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub error: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DesktopPollResponse {
    pub code: i32,
    #[serde(default)]
    pub status: String,
    pub encrypted_token: Option<String>,
    #[serde(default)]
    pub error: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserInfo {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MeResponse {
    pub code: i32,
    #[serde(default)]
    pub user: Option<UserInfo>,
    #[serde(default)]
    pub error: String,
}

/// AES-256-CBC 解密
/// key = sha256(session), 输入格式: hex(iv):hex(ciphertext)
fn aes_decrypt(session: &str, encrypted: &str) -> Result<String, String> {
    use aes::Aes256;
    use cbc::Decryptor;
    use cipher::{BlockDecryptMut, KeyIvInit};
    use sha2::{Sha256, Digest};

    let parts: Vec<&str> = encrypted.split(':').collect();
    if parts.len() != 2 {
        return Err("加密数据格式无效".to_string());
    }

    let iv = hex::decode(parts[0]).map_err(|_| "IV 解析失败".to_string())?;
    let ct = hex::decode(parts[1]).map_err(|_| "密文解析失败".to_string())?;

    if iv.len() != 16 {
        return Err("IV 长度无效".to_string());
    }

    let key = Sha256::digest(session.as_bytes());
    type Aes256CbcDec = Decryptor<Aes256>;

    let mut buf = ct.clone();
    let plaintext = Aes256CbcDec::new_from_slices(&key, &iv)
        .map_err(|e| format!("密钥/IV 初始化失败: {}", e))?
        .decrypt_padded_mut::<cipher::block_padding::Pkcs7>(&mut buf)
        .map_err(|e| format!("解密失败: {}", e))?;

    String::from_utf8(plaintext.to_vec())
        .map_err(|e| format!("解密结果 UTF-8 转换失败: {}", e))
}

/// 桌面登录初始化 — POST /api/desktop/init
pub fn desktop_init(app_id: &str) -> Result<DesktopInitResponse, String> {
    let url = format!("{}/api/desktop/init", API_BASE);
    let body = serde_json::json!({ "app_id": app_id });

    let resp = get_http_client()
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;
    if text.is_empty() {
        return Err("服务器返回空响应，请确认中央服务器是否在运行".to_string());
    }
    serde_json::from_str::<DesktopInitResponse>(&text)
        .map_err(|e| format!("解析响应失败 ({}): 原始响应={}", e, &text[..text.len().min(200)]))
}

/// 桌面登录轮询 — GET /api/desktop/poll
pub fn desktop_poll(app_id: &str, session: &str) -> Result<DesktopPollResponse, String> {
    let url = format!("{}/api/desktop/poll?app_id={}&session={}", API_BASE, app_id, session);

    let resp = get_http_client()
        .get(&url)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;
    if text.is_empty() {
        return Err("轮询返回空响应".to_string());
    }
    serde_json::from_str::<DesktopPollResponse>(&text)
        .map_err(|e| format!("解析响应失败 ({}): 原始响应={}", e, &text[..text.len().min(200)]))
}

/// 解密桌面登录返回的加密 token
pub fn desktop_decrypt_token(session: &str, encrypted_token: &str) -> Result<String, String> {
    aes_decrypt(session, encrypted_token)
}

/// 获取当前用户信息 — GET /api/auth/me
pub fn get_me(token: &str) -> Result<MeResponse, String> {
    let url = format!("{}/api/auth/me", API_BASE);

    let resp = get_http_client()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("X-Token", token)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;
    if text.is_empty() {
        return Err("获取用户信息返回空响应".to_string());
    }
    serde_json::from_str::<MeResponse>(&text)
        .map_err(|e| format!("解析响应失败 ({}): 原始响应={}", e, &text[..text.len().min(200)]))
}

/// 获取头像 — 从服务器下载图片并转为 base64 数据 URL
pub fn get_avatar(token: &str, avatar_path: &str) -> Result<String, String> {
    // 如果已是完整 URL，直接使用；否则拼上 API 基地址
    let url = if avatar_path.starts_with("http://") || avatar_path.starts_with("https://") {
        avatar_path.to_string()
    } else {
        let path = avatar_path.trim_start_matches('/');
        format!("{}/{}", API_BASE, path)
    };

    let resp = get_http_client()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("X-Token", token)
        .send()
        .map_err(|e| format!("下载头像失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("头像服务器返回 {}", resp.status()));
    }

    let bytes = resp.bytes().map_err(|e| format!("读取头像数据失败: {}", e))?;
    if bytes.is_empty() {
        return Err("头像数据为空".to_string());
    }

    // 判断 MIME 类型
    let mime = if url.ends_with(".png") { "image/png" }
               else if url.ends_with(".gif") { "image/gif" }
               else if url.ends_with(".webp") { "image/webp" }
               else { "image/jpeg" };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

/// 验证 token（POST /api/auth/verify-token）
pub fn verify(token: &str) -> Result<AccountResponse, String> {
    let url = format!("{}/api/auth/verify-token", API_BASE);
    let body = serde_json::json!({ "token": token });

    let resp = get_http_client()
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;
    parse_central_response(&text)
}

/// 服务器在线检测（GET /ping）
pub fn ping() -> Result<bool, String> {
    let url = format!("{}/api/auth/ping", API_BASE);

    let resp = get_http_client()
        .get(&url)
        .send()
        .map_err(|_| "无法连接中央服务器".to_string())?;

    Ok(resp.status().is_success())
}

/// 解析中央服务器统一响应 JSON（{code, token, name, message}）为 AccountResponse
pub fn parse_central_response(body: &str) -> Result<AccountResponse, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("解析响应失败: {}", e))?;

    let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(1) as i32;
    let token = v.get("token").and_then(|t| t.as_str()).map(String::from);
    let name = v.get("name").and_then(|n| n.as_str()).map(String::from);
    let message = v
        .get("message")
        .and_then(|m| m.as_str())
        .or_else(|| v.get("error").and_then(|e| e.as_str()))
        .unwrap_or("")
        .to_string();

    if code == 0 {
        Ok(AccountResponse {
            status: "success".to_string(),
            token,
            username: name,
            message,
            code,
        })
    } else {
        Err(message)
    }
}
