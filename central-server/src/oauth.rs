//! OAuth2 客户端模块 — 支持 GitHub / Microsoft / LittleSkin 登录
//!
//! 流程：授权码模式（无 PKCE，使用 client_secret 验证）
//! 1. 用户点击登录按钮 → 重定向到提供商授权页
//! 2. 提供商回调到 /auth/{provider}/callback?code=xxx&state=yyy
//! 3. 用 code + client_secret 交换 access_token
//! 4. 用 access_token 获取用户信息
//! 5. 返回 OAuthUser 供上层调用（后续调用账号接口注册/登录）

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// OAuth 提供商标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    GitHub,
    LittleSkin,
    MslCenter,
}

impl Provider {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "github" => Some(Self::GitHub),
            "littleskin" => Some(Self::LittleSkin),
            "msl" | "mslcenter" => Some(Self::MslCenter),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::LittleSkin => "littleskin",
            Self::MslCenter => "msl",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::LittleSkin => "LittleSkin",
            Self::MslCenter => "MSL 用户中心",
        }
    }

    /// 授权端点 URL
    pub fn auth_url(&self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/oauth/authorize",
            Self::LittleSkin => "https://little.skin/api/oauth/authorize",
            Self::MslCenter => "", // 需配置
        }
    }

    /// Token 端点 URL
    pub fn token_url(&self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/oauth/access_token",
            Self::LittleSkin => "https://little.skin/api/oauth/token",
            Self::MslCenter => "",
        }
    }

    /// 用户信息端点 URL
    pub fn userinfo_url(&self) -> &'static str {
        match self {
            Self::GitHub => "https://api.github.com/user",
            Self::LittleSkin => "https://little.skin/api/oauth/user",
            Self::MslCenter => "",
        }
    }

    /// scope 列表
    pub fn scopes(&self) -> &'static str {
        match self {
            Self::GitHub => "read:user openid email",
            Self::LittleSkin => "",
            Self::MslCenter => "",
        }
    }
}

/// OAuth 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUser {
    pub provider: String,
    pub provider_id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
}

/// OAuth 状态（CSRF state 存储）
struct PendingAuth {
    state: String,
    provider: Provider,
    redirect_uri: String,
    created_at: std::time::Instant,
}

// 用 Mutex 保护的状态存储
use std::sync::Mutex;

pub struct OAuthState {
    /// 提供商 Client ID（从配置加载）
    pub client_ids: HashMap<Provider, String>,
    /// 提供商 Client Secret（从配置加载）
    pub client_secrets: HashMap<Provider, String>,
    /// 账号服务 URL（从配置加载）
    pub account_api_url: String,
    /// 待处理的 OAuth state（防 CSRF）
    pending: Mutex<Vec<PendingAuth>>,
}

impl OAuthState {
    pub fn new(client_ids: HashMap<Provider, String>, client_secrets: HashMap<Provider, String>, account_api_url: String) -> Self {
        Self {
            client_ids,
            client_secrets,
            account_api_url,
            pending: Mutex::new(Vec::new()),
        }
    }

    /// 生成一个 CSRF state 并存储，返回 (state, 跳转URL)
    pub fn start_login(&self, provider: Provider, redirect_base: &str, client_redirect_uri: &str) -> Result<(String, String), String> {
        let client_id = self.client_ids.get(&provider)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("{} 尚未配置 Client ID", provider.display_name()))?;

        let state = uuid::Uuid::new_v4().to_string().replace('-', "");
        let redirect_uri = format!("{}/auth/{}/callback", redirect_base, provider.as_str());

        // 清理过期 state
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let now = std::time::Instant::now();
        pending.retain(|p| now.duration_since(p.created_at) < Duration::from_secs(300));
        pending.push(PendingAuth {
            state: state.clone(),
            provider,
            redirect_uri: client_redirect_uri.to_string(),
            created_at: now,
        });

        let auth_url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            provider.auth_url(),
            urlencode(client_id),
            urlencode(&redirect_uri),
            urlencode(provider.scopes()),
            urlencode(&state),
        );

        Ok((state, auth_url))
    }

    /// 验证 state 并返回对应 Provider 和 client_redirect_uri
    pub fn verify_state(&self, state: &str) -> Option<(Provider, String)> {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let idx = pending.iter().position(|p| p.state == state);
        idx.map(|i| {
            let p = pending.swap_remove(i);
            (p.provider, p.redirect_uri)
        })
    }

    /// 用授权码交换 Token，获取用户信息
    pub fn exchange_code(&self, provider: Provider, code: &str, redirect_base: &str) -> Result<OAuthUser, String> {
        let client_id = self.client_ids.get(&provider)
            .ok_or_else(|| format!("{} 未配置 Client ID", provider.display_name()))?;
        let client_secret = self.client_secrets.get(&provider)
            .ok_or_else(|| format!("{} 未配置 Client Secret", provider.display_name()))?;

        let redirect_uri = format!("{}/auth/{}/callback", redirect_base, provider.as_str());

        // 1. 交换 Token
        let access_token = get_access_token(provider, code, client_id, client_secret, &redirect_uri)?;

        // 2. 获取用户信息
        get_user_info(provider, &access_token)
    }

}

/// 用授权码交换 access_token
fn get_access_token(provider: Provider, code: &str, client_id: &str, client_secret: &str, redirect_uri: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("MC-Link-Central/1.0")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];

    let resp = client
        .post(provider.token_url())
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("请求 Token 失败: {}", e))?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().map_err(|e| format!("解析 Token 响应失败: {}", e))?;

    if !status.is_success() {
        let err = body.get("error_description")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(format!("Token 交换失败 ({}): {}", status, err));
    }

    body.get("access_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "响应中缺少 access_token".to_string())
}

/// 获取用户信息
fn get_user_info(provider: Provider, access_token: &str) -> Result<OAuthUser, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("MC-Link-Central/1.0")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .get(provider.userinfo_url())
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .map_err(|e| format!("请求用户信息失败: {}", e))?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().map_err(|e| format!("解析用户信息失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("获取用户信息失败 (HTTP {})", status));
    }

    // 各提供商的字段名不同，统一解析
    match provider {
        Provider::GitHub => {
            let id = body.get("id").and_then(|v| v.as_u64()).map(|n| n.to_string()).unwrap_or_default();
            Ok(OAuthUser {
                provider: "github".to_string(),
                provider_id: id,
                name: body.get("login").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
                avatar_url: body.get("avatar_url").and_then(|v| v.as_str()).map(String::from),
                email: body.get("email").and_then(|v| v.as_str()).map(String::from),
            })
        }
        Provider::LittleSkin => {
            let id = body.get("sub").or_else(|| body.get("id")).and_then(|v| v.as_str()).unwrap_or_default();
            Ok(OAuthUser {
                provider: "littleskin".to_string(),
                provider_id: id.to_string(),
                name: body.get("name").or_else(|| body.get("nickname")).and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
                avatar_url: None,
                email: body.get("email").and_then(|v| v.as_str()).map(String::from),
            })
        }
        Provider::MslCenter => {
            // MSL 用户中心格式未知，使用通用解析
            let id = body.get("sub").or_else(|| body.get("id")).or_else(|| body.get("uid")).and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| body.get("id").and_then(|v| v.as_u64()).map(|n| n.to_string()))
                .unwrap_or_default();
            Ok(OAuthUser {
                provider: "msl".to_string(),
                provider_id: id.to_string(),
                name: body.get("name").or_else(|| body.get("nickname")).or_else(|| body.get("username")).and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
                avatar_url: body.get("avatar").or_else(|| body.get("avatar_url")).and_then(|v| v.as_str()).map(String::from),
                email: body.get("email").and_then(|v| v.as_str()).map(String::from),
            })
        }
    }
}

pub(crate) fn urlencode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push_str("%20"),
            _ => result.push_str(&format!("%{:02X}", byte)),
        }
    }
    result
}