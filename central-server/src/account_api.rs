//! 账号 API 客户端 — 对接 account-mclink.cc.cd

use mc_link_common::log::{log, LogLevel};

const ACCOUNT_BASE: &str = "https://account-mclink.cc.cd";

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct AccountResponse {
    pub status: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub username: String,
}

/// 构建 HTTP 客户端（阻塞版，5s 超时）
fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("创建 HTTP 客户端失败")
}

/// POST /ping — 检查账号服务是否可用
pub fn ping() -> bool {
    let url = format!("{}/ping", ACCOUNT_BASE);
    match client().get(&url).send() {
        Ok(resp) => resp.status().is_success(),
        Err(e) => {
            log(LogLevel::Warn, &format!("账号服务 ping 失败: {}", e));
            false
        }
    }
}

/// POST /register — 注册账号
pub fn register(username: &str, password: &str) -> Result<AccountResponse, String> {
    let url = format!("{}/register", ACCOUNT_BASE);
    let body = serde_json::json!({ "username": username, "password": password });

    let resp = client()
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;

    let data: AccountResponse = serde_json::from_str(&text)
        .map_err(|e| format!("解析响应失败 ({}): {}", e, &text))?;

    if status.is_success() && data.status == "success" {
        Ok(data)
    } else {
        Err(data.message.clone())
    }
}

/// POST /login — 登录获取 token
pub fn login(username: &str, password: &str) -> Result<AccountResponse, String> {
    let url = format!("{}/login", ACCOUNT_BASE);
    let body = serde_json::json!({ "username": username, "password": password });

    let resp = client()
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;

    let data: AccountResponse = serde_json::from_str(&text)
        .map_err(|e| format!("解析响应失败 ({}): {}", e, &text))?;

    if status.is_success() && data.status == "success" {
        Ok(data)
    } else {
        Err(data.message)
    }
}

/// POST /verify — 验证 token 有效性
pub fn verify(token: &str) -> Result<AccountResponse, String> {
    let url = format!("{}/verify", ACCOUNT_BASE);
    let body = serde_json::json!({ "token": token });

    let resp = client()
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;

    let data: AccountResponse = serde_json::from_str(&text)
        .map_err(|e| format!("解析响应失败 ({}): {}", e, &text))?;

    if status.is_success() && data.status == "success" {
        Ok(data)
    } else {
        Err(data.message)
    }
}

/// POST /rename — 修改用户名
pub fn rename(token: &str, new_username: &str) -> Result<AccountResponse, String> {
    let url = format!("{}/rename", ACCOUNT_BASE);
    let body = serde_json::json!({ "token": token, "new_username": new_username });

    let resp = client()
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let status = resp.status();
    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;

    let data: AccountResponse = serde_json::from_str(&text)
        .map_err(|e| format!("解析响应失败 ({}): {}", e, &text))?;

    if status.is_success() && data.status == "success" {
        Ok(data)
    } else {
        Err(data.message)
    }
}

/// 对邮箱地址生成一致性密码（hash(email) 取前 16 位）
fn password_for_email(email: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(email.as_bytes());
    let result = hasher.finalize();
    hex_encode(&result[..8]) // 16 字符
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 邮箱验证码登录成功后：注册或登录账号 API
pub fn email_login_or_register(email: &str) -> Result<AccountResponse, String> {
    let username = email.split('@').next().unwrap_or(email);
    let password = password_for_email(email);

    // 先尝试登录
    match login(username, password.as_str()) {
        Ok(resp) => {
            log(LogLevel::Info, &format!("账号 {} 登录成功", username));
            return Ok(resp);
        }
        Err(_) => {
            // 登录失败则注册
            log(LogLevel::Info, &format!("账号 {} 不存在，正在注册...", username));
        }
    }

    match register(username, password.as_str()) {
        Ok(_) => {
            // 注册成功后立即登录获取 token
            log(LogLevel::Info, &format!("账号 {} 注册成功，正在登录...", username));
            login(username, password.as_str())
        }
        Err(e) => {
            // 注册失败再试一次登录（可能并发注册）
            log(LogLevel::Info, &format!("首次注册失败 ({}), 重试登录...", e));
            login(username, password.as_str())
        }
    }
}

/// OAuth 登录（仅登录，不自动注册）
pub fn oauth_login(provider: &str, provider_id: &str) -> Result<AccountResponse, String> {
    let username = format!("{}_{}", provider, &provider_id[..provider_id.len().min(12)]);
    let password = format!("oauth_{}_{}", provider, provider_id);
    login(&username, &password)
}

