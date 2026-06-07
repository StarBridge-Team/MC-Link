//! OAuth2 标准单点登录
//!
//! 流程：PKCE 授权码模式
//! 1. 在本地启动一个 HTTP 服务器（随机端口）
//! 2. 生成 PKCE 挑战码，打开系统浏览器跳转到授权页
//! 3. 授权服务器回调到本地服务器 → 获取授权码
//! 4. 用授权码 + PKCE verifier 交换 token
//! 5. 用 access_token 获取用户信息
//! 6. 加密存储 token 到本地文件

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// OAuth2 提供商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProvider {
    /// 显示名称（如 "GitHub", "自建"）
    pub name: String,
    /// 客户端 ID
    pub client_id: String,
    /// 客户端密钥
    pub client_secret: String,
    /// 授权端点 URL
    pub auth_url: String,
    /// Token 端点 URL
    pub token_url: String,
    /// 用户信息端点 URL
    pub userinfo_url: String,
    /// 请求的 scope（空格分隔）
    pub scopes: String,
}

impl Default for OAuthProvider {
    fn default() -> Self {
        Self {
            name: "OAuth2".to_string(),
            client_id: String::new(),
            client_secret: String::new(),
            auth_url: String::new(),
            token_url: String::new(),
            userinfo_url: String::new(),
            scopes: "openid profile email".to_string(),
        }
    }
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUser {
    /// 用户唯一 ID
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 头像 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// 邮箱
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// 提供商名称
    pub provider: String,
}

/// Token 数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub expires_at: u64, // UNIX 时间戳（秒）
    pub token_type: String,
}

/// OAuth 状态管理器
pub struct OAuthState {
    config: Mutex<Option<OAuthProvider>>,
    tokens: Mutex<Option<OAuthTokens>>,
    user: Mutex<Option<OAuthUser>>,
    data_dir: PathBuf,
}

impl OAuthState {
    pub fn new(data_dir: PathBuf) -> Self {
        let state = Self {
            config: Mutex::new(None),
            tokens: Mutex::new(None),
            user: Mutex::new(None),
            data_dir,
        };
        // 尝试从文件加载已保存的登录状态
        state.load_from_disk();
        state
    }

    /// 获取提供商配置
    pub fn get_config(&self) -> Option<OAuthProvider> {
        self.config.lock().unwrap().clone()
    }

    /// 保存提供商配置
    pub fn save_config(&self, config: OAuthProvider) {
        *self.config.lock().unwrap() = Some(config.clone());
        let path = self.data_dir.join("oauth_provider.json");
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = std::fs::write(&path, &json);
        }
    }

    /// 获取当前用户（如果有）
    pub fn get_user(&self) -> Option<OAuthUser> {
        self.user.lock().unwrap().clone()
    }

    /// 是否已登录
    pub fn is_logged_in(&self) -> bool {
        self.user.lock().unwrap().is_some()
    }

    /// 执行完整的 OAuth 登录流程（阻塞）
    pub fn login(&self) -> Result<OAuthUser, String> {
        let config = self.config.lock().unwrap().clone()
            .ok_or_else(|| "请先配置 OAuth 提供商".to_string())?;

        // 1. 启动本地回调服务器
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("启动回调服务器失败: {}", e))?;
        let port = listener.local_addr().map_err(|e| format!("获取端口失败: {}", e))?.port();
        let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

        // 2. 生成 PKCE 挑战
        let (code_verifier, code_challenge) = generate_pkce_challenge();

        // 3. 生成 state（防 CSRF）
        let state = generate_random_state();

        // 4. 构建授权 URL
        let auth_url = build_auth_url(&config, &redirect_uri, &code_challenge, &state);

        // 5. 打开系统浏览器
        open_browser(&auth_url)?;

        // 6. 等待回调（阻塞等待 HTTP 请求）
        let auth_code = wait_for_callback(&listener, &state)?;

        // 7. 交换 token
        let tokens = exchange_code(&config, &auth_code, &code_verifier, &redirect_uri)?;

        // 8. 获取用户信息
        let user = fetch_user_info(&config, &tokens.access_token)?;

        // 9. 保存到内存和磁盘
        *self.tokens.lock().unwrap() = Some(tokens.clone());
        *self.user.lock().unwrap() = Some(user.clone());
        self.save_to_disk();

        Ok(user)
    }

    /// 登出
    pub fn logout(&self) {
        *self.tokens.lock().unwrap() = None;
        *self.user.lock().unwrap() = None;
        // 删除磁盘上的 token 和用户文件
        let token_path = self.data_dir.join("oauth_tokens.enc");
        let user_path = self.data_dir.join("oauth_user.json");
        let _ = std::fs::remove_file(&token_path);
        let _ = std::fs::remove_file(&user_path);
    }

    // ===== 文件持久化 =====

    fn config_path(&self) -> PathBuf {
        self.data_dir.join("oauth_provider.json")
    }

    fn tokens_path(&self) -> PathBuf {
        self.data_dir.join("oauth_tokens.enc")
    }

    fn user_path(&self) -> PathBuf {
        self.data_dir.join("oauth_user.json")
    }

    fn load_from_disk(&self) {
        // 加载配置
        if let Ok(json) = std::fs::read_to_string(self.config_path()) {
            if let Ok(config) = serde_json::from_str::<OAuthProvider>(&json) {
                *self.config.lock().unwrap() = Some(config);
            }
        }
        // 加载用户信息（明文存储，不含敏感 token）
        if let Ok(json) = std::fs::read_to_string(self.user_path()) {
            if let Ok(user) = serde_json::from_str::<OAuthUser>(&json) {
                // 加载加密的 token
                if let Ok(tokens) = load_encrypted_tokens(&self.tokens_path()) {
                    if tokens.expires_at > unix_now() {
                        *self.tokens.lock().unwrap() = Some(tokens);
                        *self.user.lock().unwrap() = Some(user);
                    } else {
                        // Token 过期，清除
                        let _ = std::fs::remove_file(&self.tokens_path());
                        let _ = std::fs::remove_file(&self.user_path());
                    }
                }
            }
        }
    }

    fn save_to_disk(&self) {
        // 保存用户信息（明文）
        if let Some(user) = self.user.lock().unwrap().clone() {
            if let Ok(json) = serde_json::to_string_pretty(&user) {
                let _ = std::fs::write(self.user_path(), &json);
            }
        }
        // 加密保存 token
        if let Some(tokens) = self.tokens.lock().unwrap().clone() {
            save_encrypted_tokens(&self.tokens_path(), &tokens);
        }
    }
}

// ===== PKCE 工具函数 =====

/// 生成 PKCE code_verifier 和 code_challenge
fn generate_pkce_challenge() -> (String, String) {
    let verifier: String = (0..64)
        .map(|_| {
            let idx = rand::thread_rng().gen_range(0..ALPHANUMERIC.len());
            ALPHANUMERIC[idx] as char
        })
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = BASE64.encode(hasher.finalize());
    // URL-safe base64 without padding
    let challenge = challenge
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_string();

    (verifier, challenge)
}

const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// 生成随机 state（防 CSRF）
fn generate_random_state() -> String {
    (0..32)
        .map(|_| ALPHANUMERIC[rand::thread_rng().gen_range(0..ALPHANUMERIC.len())] as char)
        .collect()
}

// ===== 构建授权 URL =====

fn build_auth_url(
    config: &OAuthProvider,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
) -> String {
    let sep = if config.auth_url.contains('?') { "&" } else { "?" };
    format!(
        "{}{}response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        config.auth_url,
        sep,
        urlencode(&config.client_id),
        urlencode(redirect_uri),
        urlencode(&config.scopes),
        urlencode(state),
        urlencode(code_challenge),
    )
}

// ===== 回调服务器 =====

/// 等待 OAuth 回调，提取授权码（使用非阻塞 + 轮询实现超时）
fn wait_for_callback(listener: &TcpListener, expected_state: &str) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .ok();

    let deadline = Instant::now() + Duration::from_secs(300);
    let mut stream: Option<TcpStream> = None;

    while Instant::now() < deadline {
        match listener.accept() {
            Ok((s, _)) => {
                stream = Some(s);
                break;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(e) => return Err(format!("接受连接失败: {}", e)),
        }
    }

    let mut stream = stream.ok_or_else(|| "等待回调超时（5分钟）".to_string())?;

    // 恢复为阻塞模式以便读取
    stream.set_nonblocking(false).ok();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok();

    let mut buffer = [0u8; 4096];
    let size = stream
        .read(&mut buffer)
        .map_err(|e| format!("读取回调请求失败: {}", e))?;

    let request = String::from_utf8_lossy(&buffer[..size]);

    // 解析 GET 请求行: GET /callback?code=xxx&state=xxx HTTP/1.1
    let query = request
        .lines()
        .next()
        .and_then(|line| {
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                // 提取路径中的查询参数
                let path = parts[1];
                path.split('?').nth(1)
            } else {
                None
            }
        })
        .ok_or_else(|| "无法解析回调请求".to_string())?;

    // 解析查询参数
    let params = parse_query_params(query);
    let code = params
        .get("code")
        .ok_or_else(|| "回调中未找到授权码".to_string())?;

    // 验证 state（防 CSRF）
    if let Some(state_param) = params.get("state") {
        if state_param != expected_state {
            send_http_response(&mut stream, 400, "State 不匹配");
            return Err("State 校验失败，可能存在 CSRF 攻击".to_string());
        }
    }

    // 如果有 error 参数，说明用户拒绝了授权
    if let Some(error) = params.get("error") {
        send_http_response(&mut stream, 400, &format!("授权被拒绝: {}", error));
        return Err(format!("授权被拒绝: {}", error));
    }

    // 发送成功响应
    send_http_response(
        &mut stream,
        200,
        "<html><body><h2>✅ 认证成功</h2><p>你可以关闭此页面返回应用。</p></body></html>",
    );

    Ok(code.to_string())
}

fn send_http_response(stream: &mut TcpStream, status: u16, body: &str) {
    let status_line = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        _ => "500 Internal Server Error",
    };
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_line,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

// ===== Token 交换 =====

fn exchange_code(
    config: &OAuthProvider,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthTokens, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", &config.client_id),
        ("client_secret", &config.client_secret),
        ("code_verifier", code_verifier),
    ];

    let resp = client
        .post(&config.token_url)
        .form(&params)
        .send()
        .map_err(|e| format!("请求 Token 端点失败: {}", e))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .map_err(|e| format!("解析 Token 响应失败: {}", e))?;

    if !status.is_success() {
        let error = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(format!("Token 交换失败: {}", error));
    }

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "响应中缺少 access_token".to_string())?
        .to_string();

    let refresh_token = body.get("refresh_token").and_then(|v| v.as_str()).map(String::from);

    let expires_in = body.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600);
    let expires_at = unix_now() + expires_in;

    let token_type = body
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();

    Ok(OAuthTokens {
        access_token,
        refresh_token,
        expires_at,
        token_type,
    })
}

// ===== 用户信息获取 =====

fn fetch_user_info(config: &OAuthProvider, access_token: &str) -> Result<OAuthUser, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .get(&config.userinfo_url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .map_err(|e| format!("请求用户信息失败: {}", e))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .map_err(|e| format!("解析用户信息响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("获取用户信息失败 (HTTP {})", status));
    }

    // 尝试多种常见字段名来提取用户信息
    let id = body
        .get("sub")
        .or_else(|| body.get("id"))
        .or_else(|| body.get("login"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            body.get("id")
                .and_then(|v| v.as_u64())
                .map(|n| n.to_string())
        })
        .unwrap_or_default();

    let name = body
        .get("name")
        .or_else(|| body.get("nickname"))
        .or_else(|| body.get("login"))
        .or_else(|| body.get("preferred_username"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let avatar_url = body
        .get("avatar_url")
        .or_else(|| body.get("picture"))
        .or_else(|| body.get("avatar"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(OAuthUser {
        id,
        name,
        avatar_url,
        email,
        provider: config.name.clone(),
    })
}

// ===== Token 加密存储 =====

fn encryption_key() -> [u8; 32] {
    // 使用固定的应用密钥（派生自应用名）
    // 在正式生产环境中，应使用更安全的密钥管理方案
    let mut hasher = Sha256::new();
    hasher.update(b"mc-link-oauth-key-v1");
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

fn save_encrypted_tokens(path: &PathBuf, tokens: &OAuthTokens) {
    let key = encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).unwrap();

    // 生成随机 nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    if let Ok(json) = serde_json::to_string(tokens) {
        if let Ok(ciphertext) = cipher.encrypt(nonce, json.as_bytes()) {
            // 存储格式: nonce(12字节) + ciphertext
            let mut data = Vec::with_capacity(12 + ciphertext.len());
            data.extend_from_slice(&nonce_bytes);
            data.extend_from_slice(&ciphertext);
            let encoded = BASE64.encode(&data);
            let _ = std::fs::write(path, &encoded);
        }
    }
}

fn load_encrypted_tokens(path: &PathBuf) -> Result<OAuthTokens, String> {
    let encoded = std::fs::read_to_string(path)
        .map_err(|_| "Token 文件不存在".to_string())?;

    let data = BASE64
        .decode(encoded.trim())
        .map_err(|_| "Token 文件格式错误".to_string())?;

    if data.len() < 12 {
        return Err("Token 文件数据不完整".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let key = encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Token 解密失败（密钥不匹配或数据损坏）".to_string())?;

    serde_json::from_slice(&plaintext)
        .map_err(|e| format!("Token 数据解析失败: {}", e))
}

// ===== 工具函数 =====

/// 简单 URL 编码（只编码必要字符）
fn urlencode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push_str("%20"),
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

/// 简单 URL 解码
fn urldecode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }
    result
}

fn parse_query_params(query: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in query.split('&') {
        if let Some(idx) = pair.find('=') {
            let key = urldecode(&pair[..idx]);
            let value = urldecode(&pair[idx + 1..]);
            map.insert(key, value);
        }
    }
    map
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn open_browser(url: &str) -> Result<(), String> {
    // 使用系统默认浏览器打开（Windows）
    std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn()
        .map_err(|e| format!("打开浏览器失败: {}", e))?;
    Ok(())
}
