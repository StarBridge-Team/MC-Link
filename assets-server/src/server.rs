//! HTTP 服务器主循环

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::config::AssetsConfig;
use crate::meta::SettingMeta;

/// 启动资产服务器（阻塞当前线程）
pub fn run_server(config: AssetsConfig) {
    let cfg = Arc::new(config);
    let listen = cfg.listen_addr();
    let server = match Server::http(&listen) {
        Ok(s) => {
            println!();
            println!("===========================================");
            println!("  MC Link 资产服务器 v0.1");
            println!("===========================================");
            println!("  监听: http://{}", listen);
            println!("  根目录: {}", cfg.root_dir.display());
            println!("===========================================");
            println!();
            s
        }
        Err(e) => {
            eprintln!("[启动] 服务器绑定 {} 失败: {}", listen, e);
            return;
        }
    };

    loop {
        let mut request = match server.recv() {
            Ok(r) => r,
            Err(_) => continue,
        };

        if cfg.access_log {
            println!("[{}] {}", request.method(), request.url());
        }

        let response = route(&mut request, &cfg);

        let _ = request.respond(response);
    }
}

fn route(req: &mut Request, cfg: &Arc<AssetsConfig>) -> Response<std::io::Cursor<Vec<u8>>> {
    let method = req.method().clone();
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or(&url);
    let query = url.splitn(2, '?').nth(1).unwrap_or("");

    // 上传接口：仅允许 POST /upload（文件写入 assets_dir()/<path>，与 GET 布局一致）
    if path == "/upload" {
        if method != Method::Post {
            return method_not_allowed();
        }
        return handle_upload(req, cfg, query);
    }

    // 仅允许 GET / HEAD / OPTIONS
    if method != Method::Get && method != Method::Head && method != Method::Options {
        return method_not_allowed();
    }

    // CORS 预检
    if method == Method::Options {
        return cors_preflight(&cfg.cors_origin);
    }

    match path {
        "/health" => json_response(r#"{"status":"ok"}"#, &cfg.cors_origin),

        // 资源清单（客户端据此判断是否需要重新下载）
        "/manifest.json" => {
            serve_file(&cfg.assets_dir(), "manifest.json", &cfg.cors_origin)
        }

        // Bootstrap Icons
        p if p.starts_with("/bootstrap-icons/") => {
            let rel = p.trim_start_matches("/bootstrap-icons/");
            serve_file(&cfg.assets_dir().join("bootstrap-icons"), rel, &cfg.cors_origin)
        }

        // 字体
        p if p.starts_with("/fonts/") => {
            let rel = p.trim_start_matches("/fonts/");
            serve_file(&cfg.assets_dir().join("fonts"), rel, &cfg.cors_origin)
        }

        // 图标（应用图标、托盘图标等）
        p if p.starts_with("/icons/") => {
            let rel = p.trim_start_matches("/icons/");
            serve_file(&cfg.assets_dir().join("icons"), rel, &cfg.cors_origin)
        }

        // 页面清单
        "/pages/manifest.json" => {
            serve_file(&cfg.pages_dir(), "manifest.json", &cfg.cors_origin)
        }

        // 单个页面内容
        p if p.starts_with("/pages/") => {
            let rel = p.trim_start_matches("/pages/");
            // 若调用方未带后缀，默认补 .html
            let rel = if Path::new(rel).extension().is_some() {
                rel.to_string()
            } else {
                format!("{}.html", rel)
            };
            serve_file(&cfg.pages_dir(), &rel, &cfg.cors_origin)
        }

        // 更新清单
        "/update/latest.json" | "/updates/latest.json" => {
            serve_file(&cfg.updates_dir(), "latest.json", &cfg.cors_origin)
        }

        // 更新包文件
        p if p.starts_with("/update/") || p.starts_with("/updates/") => {
            let rel = p.trim_start_matches("/update/").trim_start_matches("/updates/");
            serve_file(&cfg.updates_dir(), rel, &cfg.cors_origin)
        }

        // 设置项清单（列出所有可用的设置分区）
        "/settings/manifest.json" => {
            serve_file(&cfg.setting_meta_dir(), "manifest.json", &cfg.cors_origin)
        }

        // 设置项元配置
        p if p.starts_with("/settings/meta/") => {
            let section = p.trim_start_matches("/settings/meta/");
            let safe = sanitize(section);
            if safe.is_empty() {
                return bad_request("无效的 section");
            }
            let file = format!("{}.yml", safe);
            serve_meta(&cfg.setting_meta_dir(), &file, &cfg.cors_origin)
        }

        // 整个 SettingMeta 目录索引（列出可用的 section）
        "/settings/meta" | "/settings/meta/" => {
            serve_meta_index(&cfg.setting_meta_dir(), &cfg.cors_origin)
        }

        _ => not_found(),
    }
}

/// 提供普通静态文件
fn serve_file(dir: &Path, rel: &str, cors: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let safe = sanitize_path(rel);
    if safe.as_os_str().is_empty() {
        return bad_request("无效路径");
    }
    let full = dir.join(&safe);
    match std::fs::read(&full) {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&full)
                .first_or_octet_stream()
                .to_string();
            file_response(bytes, &mime, cors)
        }
        Err(_) => not_found(),
    }
}

/// 提供元配置文件（同时返回 YAML 与说明 CORS 头）
fn serve_meta(dir: &Path, file: &str, cors: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let safe = sanitize_path(file);
    if safe.as_os_str().is_empty() {
        return bad_request("无效路径");
    }
    let full = dir.join(&safe);
    match std::fs::read_to_string(&full) {
        Ok(text) => {
            // 校验 YAML 是否能解析为 SettingMeta，提前暴露配置错误
            match SettingMeta::parse(&text) {
                Ok(_) => text_response(&text, "text/yaml; charset=utf-8", cors),
                Err(e) => {
                    eprintln!("[元配置] {} 解析失败: {}", full.display(), e);
                    text_response(&text, "text/yaml; charset=utf-8", cors)
                }
            }
        }
        Err(_) => not_found(),
    }
}

/// 列出 SettingMeta 目录下所有 section
fn serve_meta_index(dir: &Path, cors: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut sections: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".yml").or_else(|| name.strip_suffix(".yaml")) {
                    sections.push(stem.to_string());
                }
            }
        }
    }
    sections.sort();
    let body = serde_json::to_string(&serde_json::json!({ "sections": sections }))
        .unwrap_or_else(|_| r#"{"sections":[]}"#.to_string());
    json_response(&body, cors)
}

fn file_response(bytes: Vec<u8>, mime: &str, cors: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let len = bytes.len();
    let mut resp = Response::from_data(bytes).with_status_code(StatusCode(200));
    if let Ok(h) = Header::from_bytes("Content-Type", mime.as_bytes()) {
        resp.add_header(h);
    }
    if let Ok(h) = Header::from_bytes("Content-Length", len.to_string().as_bytes()) {
        resp.add_header(h);
    }
    add_cors(&mut resp, cors);
    resp
}

fn json_response(body: &str, cors: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    text_response(body, "application/json; charset=utf-8", cors)
}

fn text_response(body: &str, mime: &str, cors: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let bytes = body.as_bytes().to_vec();
    let len = bytes.len();
    let mut resp = Response::from_data(bytes).with_status_code(StatusCode(200));
    if let Ok(h) = Header::from_bytes("Content-Type", mime.as_bytes()) {
        resp.add_header(h);
    }
    if let Ok(h) = Header::from_bytes("Content-Length", len.to_string().as_bytes()) {
        resp.add_header(h);
    }
    add_cors(&mut resp, cors);
    resp
}

fn cors_preflight(cors: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string("").with_status_code(StatusCode(204));
    add_cors(&mut resp, cors);
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Headers", "Authorization, X-Token, Content-Type") {
        resp.add_header(h);
    }
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS") {
        resp.add_header(h);
    }
    resp
}

fn add_cors(resp: &mut Response<std::io::Cursor<Vec<u8>>>, cors: &str) {
    if cors.is_empty() {
        return;
    }
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Origin", cors.as_bytes()) {
        resp.add_header(h);
    }
}

fn not_found() -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string("404 Not Found").with_status_code(StatusCode(404))
}

fn bad_request(msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(msg).with_status_code(StatusCode(400))
}

fn method_not_allowed() -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string("405 Method Not Allowed").with_status_code(StatusCode(405))
}

/// 清理路径，仅保留安全字符
fn sanitize(input: &str) -> String {
    input.chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

/// 防止路径穿越：去除 `..`、绝对前缀、反斜杠
fn sanitize_path(input: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in input.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        out.push(part);
    }
    out
}

/// 处理文件上传：`POST /upload?path=<Assets相对路径>&token=<可选>`
///
/// 文件写入 `assets_dir()/path`（即 `root_dir/Assets/path`），
/// 与 GET 端点的 `/<path>` 布局完全一致，客户端无需任何改动即可命中。
fn handle_upload(
    req: &mut Request,
    cfg: &Arc<AssetsConfig>,
    query: &str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    // 鉴权：配置了 upload_token 时必须提供匹配的 ?token=
    if !cfg.upload_token.is_empty() {
        let provided = query_param(query, "token").unwrap_or_default();
        if provided != cfg.upload_token {
            return json_status(401, r#"{"ok":false,"error":"unauthorized"}"#);
        }
    }

    let rel = match query_param(query, "path") {
        Some(p) if !p.is_empty() => p,
        _ => return json_status(400, r#"{"ok":false,"error":"missing path"}"#),
    };

    let target = match safe_join(&cfg.assets_dir(), &rel) {
        Some(t) => t,
        None => return json_status(400, r#"{"ok":false,"error":"invalid path"}"#),
    };

    // 读取请求体
    let body = {
        let mut reader = req.as_reader();
        let mut buf = Vec::new();
        match Read::read_to_end(&mut reader, &mut buf) {
            Ok(_) => buf,
            Err(e) => {
                return json_status(500, &format!("{{\"ok\":false,\"error\":\"read body: {}\"}}", e));
            }
        }
    };

    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::write(&target, &body) {
        Ok(_) => {
            eprintln!("[upload] 已写入 {} ({}B)", target.display(), body.len());
            json_status(200, r#"{"ok":true}"#)
        }
        Err(e) => json_status(500, &format!("{{\"ok\":false,\"error\":\"write: {}\"}}", e)),
    }
}

/// 解析查询参数中的某个 key
fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        if k == key {
            return Some(v.to_string());
        }
    }
    None
}

/// 防止路径穿越：仅允许在 base 目录内写入，拒绝 `..`、绝对路径、反斜杠
fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.replace('\\', "/");
    if rel.starts_with('/') || rel.contains("..") {
        return None;
    }
    let base = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let mut out = base.clone();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        out.push(part);
    }
    if !out.starts_with(&base) {
        return None;
    }
    Some(out)
}

/// 返回 JSON 状态响应
fn json_status(status: u16, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string(body).with_status_code(StatusCode(status));
    if let Ok(h) =
        Header::from_bytes("Content-Type", "application/json; charset=utf-8".as_bytes())
    {
        resp.add_header(h);
    }
    resp
}
