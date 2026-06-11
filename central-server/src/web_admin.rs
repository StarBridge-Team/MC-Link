//! Web 管理面板 — HTTP 服务器 + REST API
//!
//! 路由:
//!   /               → 重定向到 /admin
//!   /admin          → 管理面板仪表盘（需 Token 或 OAuth 会话）
//!   /admin/login    → 登录页（OAuth 按钮 + Token 输入）
//!   /api/auth/*     → 认证 API
//!   /auth/*         → OAuth 登录/回调
//!   /api/admin/*    → 管理 API（需 Token 或 OAuth 会话）

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tiny_http::{Header, Method, Response, Server, StatusCode};

use mc_link_common::log::{log, LogLevel};
use mc_link_common::utils::now_secs;
use crate::oauth::{OAuthState, Provider};
use crate::session::SessionStore;
use crate::types::*;
use crate::token;

const ADMIN_HTML: &str = include_str!("web_admin.html");
const LOGIN_HTML: &str = include_str!("admin_login.html");

fn json_response<T: serde::Serialize>(data: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response_with_status(data, StatusCode(200))
}

fn json_response_with_status<T: serde::Serialize>(data: &T, status: StatusCode) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_string(data).unwrap_or_default();
    let mut resp = Response::from_string(body).with_status_code(status);
    let content_type = Header::from_bytes("Content-Type", "application/json").unwrap();
    resp.add_header(content_type);
    resp
}

fn html_response(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string(body);
    let content_type = Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap();
    resp.add_header(content_type);
    let cors = Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap();
    resp.add_header(cors);
    resp
}

fn redirect(url: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string("").with_status_code(StatusCode(302));
    let loc = Header::from_bytes("Location", url).unwrap();
    resp.add_header(loc);
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
) {
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

        let response: Response<std::io::Cursor<Vec<u8>>> = match url.as_str() {
            // 公开页面
            "/" | "/index.html" => redirect("/admin"),

            "/admin/login" => html_response(LOGIN_HTML),

            // 管理面板
            "/admin" | "/admin/" => {
                if is_authenticated(&request, &admin_token, &sessions) {
                    html_response(ADMIN_HTML)
                } else {
                    redirect("/admin/login")
                }
            }

            // ===== OAuth 登录入口 =====
            url if url.starts_with("/auth/") && url.ends_with("/login") => {
                handle_oauth_login(url, &admin_token, &oauth_state, &request)
            }

            // ===== OAuth 回调 =====
            url if url.starts_with("/auth/") && url.ends_with("/callback") => {
                handle_oauth_callback(url, &mut request, &oauth_state, &sessions, &admin_token)
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
                let h = Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap();
                resp.add_header(h);
                resp.add_header(Header::from_bytes("Access-Control-Allow-Headers", "Authorization, X-Token, Content-Type").unwrap());
                resp.add_header(Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap());
                resp
            }

            _ => not_found(),
        };

        if let Err(e) = request.respond(response) {
            log(LogLevel::Warn, &format!("Web 响应失败: {}", e));
        }
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

    // 可选的 redirect_base — 使用请求的 Host header 确定回调地址
    // 默认使用服务器绑定地址
    let redirect_base = "http://localhost";

    match oauth_state.start_login(provider, redirect_base) {
        Ok((_state, auth_url)) => redirect(&auth_url),
        Err(e) => html_response(&format!("<html><body><h2>配置错误</h2><p>{}</p></body></html>", e)),
    }
}

/// OAuth 回调 — 处理授权码交换
fn handle_oauth_callback(
    url: &str,
    request: &mut tiny_http::Request,
    oauth_state: &OAuthState,
    sessions: &SessionStore,
    _admin_token: &str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    // 提取 query 参数
    let query = url.split('?').nth(1).unwrap_or("");
    let params = parse_query_params(query);

    // 检查 error
    if let Some(error) = params.get("error") {
        return html_response(&format!(
            "<html><body><h2>授权失败</h2><p>{}</p><p><a href='/admin/login'>返回登录页</a></p></body></html>",
            error
        ));
    }

    let code = match params.get("code") {
        Some(c) => c.clone(),
        None => return html_response("<html><body><h2>缺少授权码</h2><p><a href='/admin/login'>返回登录页</a></p></body></html>"),
    };

    let state = match params.get("state") {
        Some(s) => s.clone(),
        None => return html_response("<html><body><h2>缺少 state 参数</h2><p><a href='/admin/login'>返回登录页</a></p></body></html>"),
    };

    // 验证 state
    let provider = match oauth_state.verify_state(&state) {
        Some(p) => p,
        None => return html_response("<html><body><h2>state 无效</h2><p><a href='/admin/login'>返回登录页</a></p></body></html>"),
    };

    // 回调地址需与请求一致
    let redirect_base = "http://localhost";

    // 交换 code → token → user info
    match oauth_state.exchange_code(provider, &code, redirect_base) {
        Ok(user) => {
            log(LogLevel::Info, &format!("OAuth 登录成功: {} ({})", user.name, user.provider));

            // TODO: 调用账号 API 注册/登录
            // 当 account_api_url 配置后，此处应调用:
            // POST {account_api_url}/oauth_login { provider, provider_id, name, email }
            // 获取返回的 token 和 username
            //
            // 当前直接使用 OAuth 用户信息创建会话

            // 创建会话
            let session_id = sessions.create(user);

            // 设置会话 Cookie 并重定向到 /admin
            let mut resp = redirect("/admin");
            let cookie = Header::from_bytes(
                "Set-Cookie",
                format!("mc_link_session={}; Path=/; Max-Age=86400; SameSite=Lax", session_id).as_bytes()
            ).unwrap();
            resp.add_header(cookie);
            resp
        }
        Err(e) => {
            log(LogLevel::Warn, &format!("OAuth 登录失败: {}", e));
            html_response(&format!(
                "<html><body><h2>登录失败</h2><p>{}</p><p><a href='/admin/login'>返回登录页</a></p></body></html>",
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
    resp.add_header(Header::from_bytes("Set-Cookie", "mc_link_token=; Path=/; Max-Age=0").unwrap());
    resp.add_header(Header::from_bytes("Set-Cookie", "mc_link_session=; Path=/; Max-Age=0").unwrap());
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
    let topo = state.topology.lock().unwrap_or_else(|e| e.into_inner());
    let nodes: Vec<serde_json::Value> = topo.nodes.values().map(|n| serde_json::json!({"id": n.id, "name": n.name, "address": n.address})).collect();
    let edges: Vec<serde_json::Value> = topo.edges.values().map(|e| serde_json::json!({"node_a": e.node_a, "node_b": e.node_b, "latency_ms": e.latency_ms, "packet_loss": e.packet_loss})).collect();
    json_response(&serde_json::json!({"nodes": nodes, "edges": edges, "timestamp": now_secs()}))
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