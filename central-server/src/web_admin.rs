//! Web 管理面板 — HTTP 服务器 + REST API
//!
//! 路由:
//!   /               → 重定向到 /admin
//!   /admin          → 管理面板仪表盘（需 Token）
//!   /admin/login    → 管理 Token 登录页
//!   /auth/login     → 客户端 Token 获取页（邮箱 + OAuth）
//!   /auth/{p}/login → OAuth 登录重定向
//!   /auth/{p}/callback → OAuth 回调 → 展示客户端 Token
//!   /auth/token-success → Token 获取成功页
//!   /api/auth/*     → 认证 API
//!   /api/admin/*    → 管理 API（需 Token）

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tiny_http::{Header, Method, Response, Server, StatusCode};

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::now_secs;
use crate::config::SmtpConfig;
use crate::email::EmailVerifyState;
use crate::identity::{IdentityEntry, IdentityStore};
use crate::oauth::{urlencode, OAuthState, Provider};
use crate::session::SessionStore;
use crate::types::*;
use crate::token;

// ===== 桌面登录会话存储 =====

/// 桌面登录会话
#[derive(Clone)]
struct DesktopSession {
    session_id: String,
    app_id: String,
    status: String, // "pending" or "authorized"
    encrypted_token: Option<String>,
    account_token: Option<String>,
    created_at: std::time::Instant,
    expires_in: u64, // 秒
}

impl DesktopSession {
    fn is_expired(&self) -> bool {
        std::time::Instant::now().duration_since(self.created_at).as_secs() > self.expires_in
    }
}

/// 桌面登录会话管理器
struct DesktopStore {
    sessions: std::sync::Mutex<HashMap<String, DesktopSession>>,
}

impl DesktopStore {
    fn new() -> Self {
        Self { sessions: std::sync::Mutex::new(HashMap::new()) }
    }

    fn create(&self, app_id: &str) -> String {
        let session_id = uuid::Uuid::new_v4().to_string().replace('-', "");
        let session = DesktopSession {
            session_id: session_id.clone(),
            app_id: app_id.to_string(),
            status: "pending".to_string(),
            encrypted_token: None,
            account_token: None,
            created_at: std::time::Instant::now(),
            expires_in: 300, // 5 分钟
        };
        if let Ok(mut sessions) = self.sessions.lock() {
            // 清理过期
            sessions.retain(|_, s| !s.is_expired());
            sessions.insert(session_id.clone(), session);
        }
        session_id
    }

    fn get(&self, session_id: &str) -> Option<DesktopSession> {
        let sessions = self.sessions.lock().ok()?;
        sessions.get(session_id).filter(|s| !s.is_expired()).cloned()
    }

    fn authorize(&self, session_id: &str, account_token: &str) -> bool {
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                if session.is_expired() { return false; }
                // AES-256-CBC 加密
                session.encrypted_token = Some(encrypt_desktop_token(session_id, account_token));
                session.account_token = Some(account_token.to_string());
                session.status = "authorized".to_string();
                return true;
            }
        }
        false
    }

    fn consume_token(&self, session_id: &str) -> Option<String> {
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                if session.status != "authorized" { return None; }
                let token = session.encrypted_token.clone();
                // 取出后清除明文
                session.account_token = None;
                return token;
            }
        }
        None
    }
}

/// AES-256-CBC 加密 token
/// key = sha256(session_id), iv = 随机 16 字节
/// 输出格式: hex(iv):hex(ciphertext)
fn encrypt_desktop_token(session_id: &str, token: &str) -> String {
    use aes::Aes256;
    use cbc::Encryptor;
    use cipher::{BlockEncryptMut, KeyIvInit};
    use rand::Rng;
    use sha2::{Sha256, Digest};

    let key = Sha256::digest(session_id.as_bytes());
    let mut iv = [0u8; 16];
    rand::thread_rng().fill(&mut iv);

    type Aes256CbcEnc = Encryptor<Aes256>;

    let mut buf = token.as_bytes().to_vec();
    // PKCS7 填充
    let block_size = 16;
    let pad_len = block_size - (buf.len() % block_size);
    buf.extend(std::iter::repeat(pad_len as u8).take(pad_len));

    let cipher = Aes256CbcEnc::new_from_slices(&key, &iv).expect("无效密钥/IV");
    let encrypted = cipher.encrypt_padded_mut::<cipher::block_padding::Pkcs7>(&mut buf, token.len())
        .expect("加密失败");

    let iv_hex = hex::encode(&iv);
    let ct_hex = hex::encode(encrypted);
    format!("{}:{}", iv_hex, ct_hex)
}

const ADMIN_HTML: &str = include_str!("web_admin.html");
const LOGIN_HTML: &str = include_str!("admin_login.html");
const CLIENT_LOGIN_HTML: &str = include_str!("client_login.html");

fn json_response<T: serde::Serialize>(data: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response_with_status(data, StatusCode(200))
}

fn json_response_with_status<T: serde::Serialize>(data: &T, status: StatusCode) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_string(data).unwrap_or_default();
    let mut resp = Response::from_string(body).with_status_code(status);
    // "Content-Type" 和 "application/json" 是硬编码 ASCII 字符串，不会失败
    if let Ok(content_type) = Header::from_bytes("Content-Type", "application/json") {
        resp.add_header(content_type);
    }
    resp
}

fn html_response(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string(body);
    if let Ok(content_type) = Header::from_bytes("Content-Type", "text/html; charset=utf-8") {
        resp.add_header(content_type);
    }
    if let Ok(cors) = Header::from_bytes("Access-Control-Allow-Origin", "*") {
        resp.add_header(cors);
    }
    resp
}

fn redirect(url: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string("").with_status_code(StatusCode(302));
    if let Ok(loc) = Header::from_bytes("Location", url) {
        resp.add_header(loc);
    }
    resp
}

fn not_found() -> Response<std::io::Cursor<Vec<u8>>> {
    json_response_with_status(&serde_json::json!({"code": 404, "error": "not found"}), StatusCode(404))
}

fn unauthorized() -> Response<std::io::Cursor<Vec<u8>>> {
    json_response_with_status(&serde_json::json!({"code": 401, "error": "unauthorized"}), StatusCode(401))
}

/// 从请求中提取 Token 或 Session
fn extract_token(request: &tiny_http::Request) -> Option<String> {
    for header in request.headers() {
        let field_lower = header.field.as_str().as_str().to_lowercase();
        if field_lower == "authorization" {
            let val = header.value.as_str();
            if let Some(t) = val.strip_prefix("Bearer ") {
                return Some(t.to_string());
            }
        }
        if field_lower == "x-token" {
            return Some(header.value.as_str().to_string());
        }
        if field_lower == "cookie" {
            for part in header.value.as_str().split(';') {
                let trimmed = part.trim();
                if let Some(v) = trimmed.strip_prefix("mc_link_token=") {
                    return Some(v.to_string());
                }
                if let Some(v) = trimmed.strip_prefix("mc_link_session=") {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 检查请求是否已认证（admin_token 或 session）
fn is_authenticated(request: &tiny_http::Request, admin_token: &str, sessions: &SessionStore) -> bool {
    if let Some(t) = extract_token(request) {
        if token::validate(&t, admin_token) {
            return true;
        }
        if sessions.get(&t).is_some() {
            return true;
        }
    }
    false
}

/// 启动 Web 管理 HTTP 服务器
pub fn start_web_admin(
    state: Arc<CentralState>,
    bind_addr: SocketAddr,
    admin_token: String,
    oauth_state: Arc<OAuthState>,
    sessions: Arc<SessionStore>,
    email_state: Arc<EmailVerifyState>,
    smtp_config: Option<SmtpConfig>,
    identity_store: Arc<IdentityStore>,
) {
    let desktop_store = Arc::new(DesktopStore::new());
    let server = match Server::http(&bind_addr) {
        Ok(s) => {
            log(LogLevel::Info, &format!("Web 服务器已启动在 http://{}", bind_addr));
            log(LogLevel::Info, &format!("  管理面板: http://{}/admin", bind_addr));
            s
        }
        Err(e) => {
            log(LogLevel::Error, &format!("Web 服务器启动失败: {}", e));
            return;
        }
    };

    loop {
        let mut request = match server.recv() {
            Ok(r) => r,
            Err(_) => continue,
        };

        let url = request.url().to_string();
        let method = request.method().clone();
        let path = url.split('?').next().unwrap_or(&url).to_string();

        let response: Response<std::io::Cursor<Vec<u8>>> = match path.as_str() {
            // 公开页面
            "/" | "/index.html" => redirect("/auth/login"),

            "/admin/login" => html_response(LOGIN_HTML),

            // 管理面板
            "/admin" | "/admin/" => {
                if is_authenticated(&request, &admin_token, &sessions) {
                    html_response(ADMIN_HTML)
                } else if let Some(t) = url.split('?').nth(1).and_then(|q| {
                    q.split('&').find_map(|p| {
                        let mut kv = p.splitn(2, '=');
                        match (kv.next(), kv.next()) {
                            (Some("token"), Some(v)) => Some(v.to_string()),
                            _ => None,
                        }
                    })
                }) {
                    if token::validate(&t, &admin_token) || sessions.get(&t).is_some() {
                        // 通过 query token 验证成功，设置会话并刷新页面（不带 token）
                        let mut resp = redirect("/admin");
                        let cookie_val = format!("mc_link_token={}; Path=/; Max-Age=604800; SameSite=Lax", t);
                        if let Ok(h) = Header::from_bytes("Set-Cookie", cookie_val.as_bytes()) {
                            resp.add_header(h);
                        }
                        resp
                    } else {
                        redirect("/admin/login")
                    }
                } else {
                    redirect("/admin/login")
                }
            }

            // ===== 客户端 Token 获取/注册/忘记密码页 =====
            "/auth/login" | "/auth/" => html_response(CLIENT_LOGIN_HTML),
            "/auth/register" => html_response(CLIENT_LOGIN_HTML),
            "/auth/forgot" => html_response(CLIENT_LOGIN_HTML),

            // ===== OAuth 登录入口 =====
            url if url.starts_with("/auth/") && url.ends_with("/login") => {
                handle_oauth_login(url, &admin_token, &oauth_state, &request)
            }

            // ===== OAuth 回调 =====
            url if url.starts_with("/auth/") && url.ends_with("/callback") => {
                handle_oauth_callback(url, &mut request, &oauth_state, &admin_token, &identity_store)
            }

            // ===== 认证 API =====
            "/api/auth/check" => {
                handle_auth_check(&mut request, &admin_token)
            }

            "/api/auth/session" => {
                handle_session_info(&request, &admin_token, &sessions)
            }

            "/api/auth/logout" => {
                handle_logout(&request, &admin_token, &sessions)
            }

            // ===== 密码登录 =====
            "/api/auth/password-login" => {
                handle_password_login(&mut request)
            }

            // ===== 邮箱验证身份（用于注册流程）=====
            "/api/auth/verify-identity" => {
                handle_verify_identity(&mut request, &email_state, &identity_store)
            }

            // ===== 注册（用身份令牌 + 用户名 + 密码）=====
            "/api/auth/register" => {
                handle_register(&mut request, &identity_store)
            }

            // ===== 直接注册（用户名 + 密码，不用身份令牌）=====
            "/api/auth/direct-register" => {
                handle_direct_register(&mut request)
            }

            // ===== Token 验证 =====
            "/api/auth/verify-token" => {
                handle_verify_token(&mut request)
            }

            "/api/auth/ping" => {
                json_response(&serde_json::json!({"code": 0, "message": "ok"}))
            }

            // ===== 忘记密码（邮箱验证码 → 重置密码）=====
            "/api/auth/forgot-password" => {
                handle_forgot_password(&mut request, &email_state)
            }

            // ===== 邮箱验证码 API =====
            "/api/auth/send-code" => {
                handle_send_code(&mut request, &email_state, &smtp_config)
            }

            "/api/auth/verify-code" => {
                handle_verify_code(&mut request, &email_state)
            }

            // ===== 桌面登录 API =====
            "/api/desktop/init" => {
                handle_desktop_init(&mut request, &desktop_store)
            }

            "/api/desktop/poll" => {
                handle_desktop_poll(&url, &desktop_store)
            }

            "/api/auth/desktop/authorize" => {
                handle_desktop_authorize(&mut request, &desktop_store)
            }

            // 桌面登录授权页
            "/login" => html_response(CLIENT_LOGIN_HTML),

            // ===== 管理 API =====
            _ if url.starts_with("/api/admin/") => {
                if is_authenticated(&request, &admin_token, &sessions) {
                    handle_admin_api(&url, &method, &state)
                } else {
                    unauthorized()
                }
            }

            // ===== OPTIONS 预检 =====
            _ if method == Method::Options => {
                let mut resp = Response::from_string("").with_status_code(StatusCode(204));
                if let Ok(h) = Header::from_bytes("Access-Control-Allow-Origin", "*") {
                    resp.add_header(h);
                }
                if let Ok(h) = Header::from_bytes("Access-Control-Allow-Headers", "Authorization, X-Token, Content-Type") {
                    resp.add_header(h);
                }
                if let Ok(h) = Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS") {
                    resp.add_header(h);
                }
                resp
            }

            _ => not_found(),
        };

        if let Err(e) = request.respond(response) {
            log(LogLevel::Warn, &format!("Web 响应失败: {}", e));
        }
    }
}

// ===== 邮箱验证码 API =====

fn handle_send_code(request: &mut tiny_http::Request, email_state: &EmailVerifyState, smtp_config: &Option<SmtpConfig>) -> Response<std::io::Cursor<Vec<u8>>> {
    let smtp = match smtp_config {
        Some(c) => c,
        None => return json_response(&serde_json::json!({"code": 1, "error": "SMTP not configured"})),
    };

    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let email = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("email").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_default();

    if email.is_empty() || !email.contains('@') {
        return json_response(&serde_json::json!({"code": 1, "error": "请输入有效邮箱"}));
    }

    match email_state.send_code(&email, smtp) {
        Ok(_) => json_response(&serde_json::json!({"code": 0, "message": "验证码已发送"})),
        Err(e) => json_response(&serde_json::json!({"code": 1, "error": e})),
    }
}

fn handle_verify_code(request: &mut tiny_http::Request, email_state: &EmailVerifyState) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let (email, code) = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => (
            v.get("email").and_then(|e| e.as_str()).unwrap_or("").to_string(),
            v.get("code").and_then(|c| c.as_str()).unwrap_or("").to_string(),
        ),
        Err(_) => (String::new(), String::new()),
    };

    if email.is_empty() || code.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    if email_state.verify_code(&email, &code) {
        let account_result = crate::account_api::email_login_or_register(&email);

        match account_result {
            Ok(acct) => {
                log(LogLevel::Info, &format!("邮箱登录成功: {} -> {}", email, acct.username));
                json_response(&serde_json::json!({
                    "code": 0,
                    "message": "验证成功",
                    "token": acct.token,
                    "name": acct.username,
                }))
            }
            Err(e) => {
                log(LogLevel::Warn, &format!("邮箱登录失败 ({}): {}", email, e));
                json_response(&serde_json::json!({"code": 1, "error": format!("账号服务错误: {}", e)}))
            }
        }
    } else {
        json_response(&serde_json::json!({"code": 1, "error": "验证码无效或已过期"}))
    }
}

// ===== 密码登录 =====

fn handle_password_login(request: &mut tiny_http::Request) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let (username, password_hashed) = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => (
            v.get("username").and_then(|u| u.as_str()).unwrap_or("").to_string(),
            v.get("password_hashed").and_then(|p| p.as_str()).unwrap_or("").to_string(),
        ),
        Err(_) => (String::new(), String::new()),
    };

    if username.is_empty() || password_hashed.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    match crate::account_api::login(&username, &password_hashed) {
        Ok(acct) => {
            log(LogLevel::Info, &format!("密码登录成功: {}", username));
            json_response(&serde_json::json!({
                "code": 0,
                "token": acct.token,
                "name": acct.username,
            }))
        }
        Err(e) => {
            log(LogLevel::Warn, &format!("密码登录失败 ({}): {}", username, e));
            json_response(&serde_json::json!({"code": 1, "error": e}))
        }
    }
}

// ===== 邮箱验证身份（用于注册流程）=====

fn handle_verify_identity(
    request: &mut tiny_http::Request,
    email_state: &EmailVerifyState,
    identity_store: &IdentityStore,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let (email, code) = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => (
            v.get("email").and_then(|e| e.as_str()).unwrap_or("").to_string(),
            v.get("code").and_then(|c| c.as_str()).unwrap_or("").to_string(),
        ),
        Err(_) => (String::new(), String::new()),
    };

    if email.is_empty() || code.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    if email_state.verify_code(&email, &code) {
        let identity_token = identity_store.create(IdentityEntry {
            email: Some(email.clone()),
            provider: None,
            provider_id: None,
            name: None,
            created_at: std::time::Instant::now(),
        });
        log(LogLevel::Info, &format!("邮箱验证通过，创建身份令牌: {}", email));
        json_response(&serde_json::json!({
            "code": 0,
            "identity_token": identity_token,
            "email": email,
        }))
    } else {
        json_response(&serde_json::json!({"code": 1, "error": "验证码无效或已过期"}))
    }
}

// ===== 注册 =====

fn handle_register(
    request: &mut tiny_http::Request,
    identity_store: &IdentityStore,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let (identity_token, username, password_hashed) = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => (
            v.get("identity_token").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            v.get("username").and_then(|u| u.as_str()).unwrap_or("").to_string(),
            v.get("password_hashed").and_then(|p| p.as_str()).unwrap_or("").to_string(),
        ),
        Err(_) => (String::new(), String::new(), String::new()),
    };

    if identity_token.is_empty() || username.is_empty() || password_hashed.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    // 验证身份令牌
    let entry = match identity_store.consume(&identity_token) {
        Some(e) => e,
        None => return json_response(&serde_json::json!({"code": 1, "error": "身份验证已过期，请重新验证"})),
    };

    // 调用账号 API 注册
    match crate::account_api::register(&username, &password_hashed) {
        Ok(_acct) => {
            log(LogLevel::Info, &format!("注册成功: {} (邮箱: {:?})", username, entry.email));
            // 注册后调用登录获取 token
            match crate::account_api::login(&username, &password_hashed) {
                Ok(login_acct) => {
                    json_response(&serde_json::json!({
                        "code": 0,
                        "token": login_acct.token,
                        "name": login_acct.username,
                    }))
                }
                Err(e) => {
                    json_response(&serde_json::json!({"code": 1, "error": format!("注册成功但登录失败: {}", e)}))
                }
            }
        }
        Err(e) => {
            log(LogLevel::Warn, &format!("注册失败 ({}): {}", username, e));
            json_response(&serde_json::json!({"code": 1, "error": e}))
        }
    }
}

// ===== 直接注册 =====

fn handle_direct_register(request: &mut tiny_http::Request) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let (username, password_hashed) = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => (
            v.get("username").and_then(|u| u.as_str()).unwrap_or("").to_string(),
            v.get("password_hashed").and_then(|p| p.as_str()).unwrap_or("").to_string(),
        ),
        Err(_) => (String::new(), String::new()),
    };

    if username.is_empty() || password_hashed.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    // 先尝试登录（可能已有账号）
    match crate::account_api::login(&username, &password_hashed) {
        Ok(acct) => {
            log(LogLevel::Info, &format!("直接注册 — 已有账号，登录成功: {}", username));
            return json_response(&serde_json::json!({
                "code": 0,
                "token": acct.token,
                "name": acct.username,
                "message": "登录成功",
            }));
        }
        Err(_) => {}
    }

    // 不存在则注册
    match crate::account_api::register(&username, &password_hashed) {
        Ok(_) => {
            log(LogLevel::Info, &format!("直接注册 — 注册成功: {}", username));
            match crate::account_api::login(&username, &password_hashed) {
                Ok(acct) => json_response(&serde_json::json!({
                    "code": 0,
                    "token": acct.token,
                    "name": acct.username,
                    "message": "注册成功",
                })),
                Err(e) => json_response(&serde_json::json!({"code": 1, "error": format!("注册成功但登录失败: {}", e)})),
            }
        }
        Err(e) => {
            log(LogLevel::Warn, &format!("直接注册 — 注册失败 ({}): {}", username, e));
            json_response(&serde_json::json!({"code": 1, "error": e}))
        }
    }
}

// ===== Token 验证 =====

fn handle_verify_token(request: &mut tiny_http::Request) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let token = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_default();

    if token.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    match crate::account_api::verify(&token) {
        Ok(acct) => {
            json_response(&serde_json::json!({
                "code": 0,
                "token": acct.token,
                "name": acct.username,
            }))
        }
        Err(e) => {
            json_response(&serde_json::json!({"code": 1, "error": e}))
        }
    }
}

// ===== 忘记密码 =====

fn handle_forgot_password(
    request: &mut tiny_http::Request,
    email_state: &EmailVerifyState,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let (email, code, _new_password_hashed) = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => (
            v.get("email").and_then(|e| e.as_str()).unwrap_or("").to_string(),
            v.get("code").and_then(|c| c.as_str()).unwrap_or("").to_string(),
            v.get("new_password_hashed").and_then(|p| p.as_str()).unwrap_or("").to_string(),
        ),
        Err(_) => (String::new(), String::new(), String::new()),
    };

    if email.is_empty() || code.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    if !email_state.verify_code(&email, &code) {
        return json_response(&serde_json::json!({"code": 1, "error": "验证码无效或已过期"}));
    }

    // 账号 API 暂未提供重置密码接口
    json_response(&serde_json::json!({"code": 1, "error": "功能开发中，请在账号服务中添加重置密码接口后使用"}))
}

// ===== 桌面登录 API =====

fn handle_desktop_init(
    request: &mut tiny_http::Request,
    desktop_store: &DesktopStore,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let app_id = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("app_id").and_then(|a| a.as_str()).map(String::from))
        .unwrap_or_default();

    if app_id.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "缺少 app_id"}));
    }

    let session = desktop_store.create(&app_id);
    log(LogLevel::Info, &format!("桌面登录初始化: app_id={}, session={}", app_id, &session[..8]));
    json_response(&serde_json::json!({
        "code": 0,
        "session": session,
        "expires_in": 300,
    }))
}

fn handle_desktop_poll(
    url: &str,
    desktop_store: &DesktopStore,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let query = url.split('?').nth(1).unwrap_or("");
    let params = parse_query_params(query);
    let app_id = params.get("app_id").cloned().unwrap_or_default();
    let session = params.get("session").cloned().unwrap_or_default();

    if app_id.is_empty() || session.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    let session_data = desktop_store.get(&session);
    match session_data {
        Some(data) if data.status == "authorized" => {
            let encrypted_token = desktop_store.consume_token(&session);
            match encrypted_token {
                Some(enc_token) => {
                    json_response(&serde_json::json!({
                        "code": 0,
                        "status": "authorized",
                        "encrypted_token": enc_token,
                    }))
                }
                None => {
                    json_response(&serde_json::json!({
                        "code": 0,
                        "status": "authorized",
                        "encrypted_token": null,
                    }))
                }
            }
        }
        Some(_) => {
            json_response(&serde_json::json!({
                "code": 0,
                "status": "pending",
            }))
        }
        None => {
            json_response(&serde_json::json!({
                "code": 1,
                "error": "会话无效或已过期",
            }))
        }
    }
}

fn handle_desktop_authorize(
    request: &mut tiny_http::Request,
    desktop_store: &DesktopStore,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let (app_id, session, token) = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => (
            v.get("app_id").and_then(|a| a.as_str()).unwrap_or("").to_string(),
            v.get("session").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            v.get("token").and_then(|t| t.as_str()).unwrap_or("").to_string(),
        ),
        Err(_) => (String::new(), String::new(), String::new()),
    };

    if app_id.is_empty() || session.is_empty() || token.is_empty() {
        return json_response(&serde_json::json!({"code": 1, "error": "参数不完整"}));
    }

    if desktop_store.authorize(&session, &token) {
        log(LogLevel::Info, &format!("桌面登录授权成功: app_id={}", app_id));
        json_response(&serde_json::json!({"code": 0, "message": "授权成功"}))
    } else {
        json_response(&serde_json::json!({"code": 1, "error": "会话无效或已过期"}))
    }
}

/// OAuth 登录 — 重定向到提供商授权页
fn handle_oauth_login(
    url: &str,
    _admin_token: &str,
    oauth_state: &OAuthState,
    _request: &tiny_http::Request,
) -> Response<std::io::Cursor<Vec<u8>>> {
    // 提取 provider 名称: /auth/{provider}/login
    let provider_str = url
        .strip_prefix("/auth/")
        .and_then(|s| s.strip_suffix("/login"))
        .unwrap_or("");

    let provider = match Provider::from_str(provider_str) {
        Some(p) => p,
        None => return not_found(),
    };

    // 从 query 中提取客户端回调地址
    let query = url.split('?').nth(1).unwrap_or("");
    let params = parse_query_params(query);
    let client_redirect = params.get("redirect_uri").map(|s| s.as_str()).unwrap_or("");

    let redirect_base = "http://localhost";

    match oauth_state.start_login(provider, redirect_base, client_redirect) {
        Ok((_state, auth_url)) => redirect(&auth_url),
        Err(e) => html_response(&format!("<html><body><h2>配置错误</h2><p>{}</p></body></html>", e)),
    }
}

/// OAuth 回调 — 处理授权码交换，重定向回客户端或注册页
fn handle_oauth_callback(
    url: &str,
    _request: &mut tiny_http::Request,
    oauth_state: &OAuthState,
    _admin_token: &str,
    identity_store: &IdentityStore,
) -> Response<std::io::Cursor<Vec<u8>>> {
    // 提取 query 参数
    let query = url.split('?').nth(1).unwrap_or("");
    let params = parse_query_params(query);

    // 检查 error
    if let Some(error) = params.get("error") {
        return html_response(&format!(
            "<html><body><h2>授权失败</h2><p>{}</p><p><a href='/auth/login'>返回登录页</a></p></body></html>",
            error
        ));
    }

    let code = match params.get("code") {
        Some(c) => c.clone(),
        None => return html_response("<html><body><h2>缺少授权码</h2><p><a href='/auth/login'>返回登录页</a></p></body></html>"),
    };

    let state = match params.get("state") {
        Some(s) => s.clone(),
        None => return html_response("<html><body><h2>缺少 state 参数</h2><p><a href='/auth/login'>返回登录页</a></p></body></html>"),
    };

    // 验证 state 并取回客户端回调地址
    let (provider, client_redirect_uri) = match oauth_state.verify_state(&state) {
        Some((p, uri)) => (p, uri),
        None => return html_response("<html><body><h2>state 无效</h2><p><a href='/auth/login'>返回登录页</a></p></body></html>"),
    };

    // 回调地址需与请求一致
    let redirect_base = "http://localhost";

    // 交换 code → token → user info
    match oauth_state.exchange_code(provider, &code, redirect_base) {
        Ok(user) => {
            log(LogLevel::Info, &format!("OAuth 验证成功: {} ({})", user.name, user.provider));

            // 先尝试登录（已有账号）
            match crate::account_api::oauth_login(&user.provider, &user.provider_id) {
                Ok(acct) => {
                    // 登录成功 → 已有账号，直接返回 token
                    log(LogLevel::Info, &format!("OAuth 已有账号: {}", acct.username));
                    if client_redirect_uri.is_empty() {
                        return html_response(&format!(
                            "<html><body><h2>登录成功</h2><p>欢迎 {}！</p></body></html>", acct.username
                        ));
                    }
                    let sep = if client_redirect_uri.contains('?') { '&' } else { '?' };
                    let redirect_url = format!("{}{}token={}&name={}",
                        client_redirect_uri, sep, urlencode(&acct.token), urlencode(&acct.username));
                    return redirect(&redirect_url);
                }
                Err(_) => {
                    // 登录失败 → 新用户，跳转到注册页
                    log(LogLevel::Info, &format!("OAuth 新用户，跳转到注册页: {} ({})", user.name, user.provider));
                    let identity_token = identity_store.create(IdentityEntry {
                        email: None,
                        provider: Some(user.provider.clone()),
                        provider_id: Some(user.provider_id.clone()),
                        name: Some(user.name.clone()),
                        created_at: std::time::Instant::now(),
                    });

                    let redirect_target = if client_redirect_uri.is_empty() {
                        format!("/auth/register?identity_token={}", identity_token)
                    } else {
                        format!("/auth/register?identity_token={}&redirect_uri={}",
                            identity_token, urlencode(&client_redirect_uri))
                    };
                    return redirect(&redirect_target);
                }
            }
        }
        Err(e) => {
            log(LogLevel::Warn, &format!("OAuth 登录失败: {}", e));
            html_response(&format!(
                "<html><body><h2>登录失败</h2><p>{}</p><p><a href='/auth/login'>返回登录页</a></p></body></html>",
                e
            ))
        }
    }
}

/// Token 验证
fn handle_auth_check(request: &mut tiny_http::Request, admin_token: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let token_val = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(String::from));

    match token_val {
        Some(ref t) if token::validate(t, admin_token) => {
            let mut resp = json_response(&serde_json::json!({"code": 0, "message": "ok"}));
            let cookie = Header::from_bytes(
                "Set-Cookie",
                format!("mc_link_token={}; Path=/; Max-Age=604800; SameSite=Lax", t).as_bytes()
            ).unwrap();
            resp.add_header(cookie);
            resp
        }
        _ => json_response_with_status(&serde_json::json!({"code": 401, "error": "invalid token"}), StatusCode(401)),
    }
}

/// 获取当前会话信息
fn handle_session_info(request: &tiny_http::Request, admin_token: &str, sessions: &SessionStore) -> Response<std::io::Cursor<Vec<u8>>> {
    let token = match extract_token(request) {
        Some(t) => t,
        None => return json_response(&serde_json::json!({"authenticated": false})),
    };

    // 检查是否是 admin_token
    if token::validate(&token, admin_token) {
        return json_response(&serde_json::json!({
            "authenticated": true,
            "user": {
                "name": "Administrator",
                "provider": "admin_token",
            }
        }));
    }

    // 检查是否是 session
    if let Some(session) = sessions.get(&token) {
        return json_response(&serde_json::json!({
            "authenticated": true,
            "user": {
                "name": session.user.name,
                "provider": session.user.provider,
                "avatar_url": session.user.avatar_url,
                "email": session.user.email,
            }
        }));
    }

    json_response(&serde_json::json!({"authenticated": false}))
}

/// 注销
fn handle_logout(request: &tiny_http::Request, _admin_token: &str, sessions: &SessionStore) -> Response<std::io::Cursor<Vec<u8>>> {
    if let Some(token) = extract_token(request) {
        sessions.remove(&token);
    }
    let mut resp = json_response(&serde_json::json!({"code": 0, "message": "logged out"}));
    // 清除所有 cookie
    if let Ok(h) = Header::from_bytes("Set-Cookie", "mc_link_token=; Path=/; Max-Age=0") {
        resp.add_header(h);
    }
    if let Ok(h) = Header::from_bytes("Set-Cookie", "mc_link_session=; Path=/; Max-Age=0") {
        resp.add_header(h);
    }
    resp
}

// ===== 管理 API 保持原样 =====

fn handle_admin_api(url: &str, method: &Method, state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    if method != &Method::Get { return not_found(); }
    match url {
        "/api/admin/stats" => handle_stats(state),
        "/api/admin/relays" => handle_relays(state),
        "/api/admin/rooms" => handle_rooms(state),
        "/api/admin/paths" => handle_paths(state),
        "/api/admin/topology" => handle_topology(state),
        "/api/admin/traffic" => handle_traffic(state),
        "/api/admin/history" => handle_history(state),
        _ => not_found(),
    }
}

fn handle_stats(state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
    let relay_total = relays.len();
    let rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());
    let room_count = rooms.len();
    let players = state.players.lock().unwrap_or_else(|e| e.into_inner());
    let player_count: usize = players.values().map(|v| v.len()).sum();
    let paths_count = state.active_paths.lock().unwrap_or_else(|e| e.into_inner()).len();
    let online_count = {
        let instants = state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner());
        let now = std::time::Instant::now();
        instants.values().filter(|t| now.duration_since(**t) < Duration::from_secs(120)).count()
    };
    let total_traffic_bytes: u64 = state.traffic_reports.lock().unwrap_or_else(|e| e.into_inner())
        .values().map(|s| s.bytes_sent_total.saturating_add(s.bytes_recv_total)).sum();
    let (avg_latency, avg_hops) = {
        let paths = state.active_paths.lock().unwrap_or_else(|e| e.into_inner());
        if paths.is_empty() { (0.0, 0.0) }
        else {
            let count = paths.len();
            (paths.values().map(|p| p.total_latency_ms).sum::<u64>() as f64 / count as f64,
             paths.values().map(|p| p.hops.len()).sum::<usize>() as f64 / count as f64)
        }
    };
    json_response(&serde_json::json!({
        "relays_online": online_count, "relays_total": relay_total, "rooms_active": room_count,
        "players_total": player_count, "paths_active": paths_count, "avg_latency_ms": avg_latency,
        "avg_hops": avg_hops, "total_traffic_bytes": total_traffic_bytes, "timestamp": now_secs(),
    }))
}

fn handle_relays(state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
    let instants = state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner());
    let traffic = state.traffic_reports.lock().unwrap_or_else(|e| e.into_inner());
    let now_instant = std::time::Instant::now();
    let list: Vec<serde_json::Value> = relays.values().map(|relay| {
        let online = instants.get(&relay.id).map(|t| now_instant.duration_since(*t) < Duration::from_secs(120)).unwrap_or(false);
        let ts = traffic.get(&relay.id);
        serde_json::json!({
            "id": relay.id, "name": relay.name, "address": relay.address,
            "service_type": relay.service_type, "private": relay.private, "transit": relay.transit,
            "online": online, "last_seen": relay.last_seen,
            "traffic_bytes_sent": ts.map(|t| t.bytes_sent_total).unwrap_or(0),
            "traffic_bytes_recv": ts.map(|t| t.bytes_recv_total).unwrap_or(0),
            "connections": ts.map(|t| t.current_connections).unwrap_or(0),
        })
    }).collect();
    json_response(&serde_json::json!({"relays": list, "timestamp": now_secs()}))
}

fn handle_rooms(state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    let rooms = state.rooms.lock().unwrap_or_else(|e| e.into_inner());
    let players_map = state.players.lock().unwrap_or_else(|e| e.into_inner());
    let room_paths = state.room_paths.lock().unwrap_or_else(|e| e.into_inner());
    let active_paths = state.active_paths.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = rooms.iter().map(|(name, room)| {
        let players = players_map.get(name).cloned().unwrap_or_default();
        let path = room_paths.get(name).and_then(|pid| active_paths.get(pid));
        serde_json::json!({
            "name": name, "host_relay_id": room.host_relay_id, "password_hash": room.password_hash,
            "created_at": room.created_at, "players": players,
            "path": path.map(|p| serde_json::json!({"path_id": p.path_id, "hops": p.hops, "total_latency_ms": p.total_latency_ms})),
        })
    }).collect();
    json_response(&serde_json::json!({"rooms": list, "timestamp": now_secs()}))
}

fn handle_paths(state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    let active_paths = state.active_paths.lock().unwrap_or_else(|e| e.into_inner());
    let room_paths = state.room_paths.lock().unwrap_or_else(|e| e.into_inner());
    let path_to_room: HashMap<&str, &str> = room_paths.iter().map(|(r, p)| (p.as_str(), r.as_str())).collect();
    let list: Vec<serde_json::Value> = active_paths.iter().map(|(path_id, path)| {
        serde_json::json!({"path_id": path_id, "room_name": path_to_room.get(path_id.as_str()).copied().unwrap_or("?"),
            "hops": path.hops, "total_latency_ms": path.total_latency_ms, "score": path.score})
    }).collect();
    json_response(&serde_json::json!({"paths": list, "timestamp": now_secs()}))
}

fn handle_topology(state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    let mgr = state.topology_manager.read().unwrap_or_else(|e| e.into_inner());
    let nodes: Vec<serde_json::Value> = mgr.ipv4.nodes.values().map(|n| serde_json::json!({"id": n.id, "name": n.name, "address": n.address})).collect();
    let edges: Vec<serde_json::Value> = mgr.ipv4.edges.values().map(|e| serde_json::json!({"node_a": e.node_a, "node_b": e.node_b, "latency_ms": e.latency_ms, "packet_loss": e.packet_loss})).collect();
    let mixed_nodes: Vec<serde_json::Value> = mgr.mixed.nodes.values().map(|n| serde_json::json!({"id": n.id, "name": n.name, "address": n.address, "address_v6": n.address_v6})).collect();
    let mixed_edges: Vec<serde_json::Value> = mgr.mixed.edges.values().map(|e| serde_json::json!({"node_a": e.node_a, "node_b": e.node_b, "latency_ms": e.latency_ms, "packet_loss": e.packet_loss})).collect();
    json_response(&serde_json::json!({
        "nodes": nodes, "edges": edges,
        "mixed_nodes": mixed_nodes, "mixed_edges": mixed_edges,
        "timestamp": now_secs()
    }))
}

fn handle_traffic(state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    let reports = state.traffic_reports.lock().unwrap_or_else(|e| e.into_inner());
    let relays = state.relays.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = reports.iter().map(|(relay_id, stats)| {
        let name = relays.get(relay_id).map(|r| r.name.as_str()).unwrap_or("?");
        serde_json::json!({"relay_id": relay_id, "name": name, "bytes_sent_total": stats.bytes_sent_total,
            "bytes_recv_total": stats.bytes_recv_total, "bytes_total": stats.bytes_sent_total.saturating_add(stats.bytes_recv_total),
            "current_connections": stats.current_connections, "last_report": stats.last_report})
    }).collect();
    json_response(&serde_json::json!({"traffic": list, "timestamp": now_secs()}))
}

fn handle_history(state: &CentralState) -> Response<std::io::Cursor<Vec<u8>>> {
    let history = state.stats_history.lock().unwrap_or_else(|e| e.into_inner());
    json_response(&serde_json::json!({"snapshots": history.iter().collect::<Vec<&StatsSnapshot>>(), "timestamp": now_secs()}))
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&') {
        if let Some(idx) = pair.find('=') {
            map.insert(pair[..idx].to_string(), pair[idx + 1..].to_string());
        }
    }
    map
}

