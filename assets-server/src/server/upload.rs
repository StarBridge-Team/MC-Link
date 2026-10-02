//! 写入类接口（上传 / 删除）与其鉴权。
//!
//! 这是**第二个需要被单独审视的块**：它决定了"谁能改服务器上的资源"。
//! 历史行为是"token 为空 = 关闭鉴权"，于是按仓库自带配置启动的服务器允许任何人
//! 覆盖 `update/latest.json` 与更新包——等于把"给全体客户端投毒"的能力开放给整个网络。
//! 现在一律 fail-closed（未配置令牌时拒绝一切写入）。

use std::io::Read;
use std::sync::Arc;

use tiny_http::Request;

use crate::config::AssetsConfig;

use super::paths::safe_join;
use super::query::{query_param, query_path};
use super::response::{header_value, json_status, Resp};

/// 单次上传的体积上限。
///
/// 更新包约 30MB，留出余量；这里的意义是给"任何能连上的人"设一个天花板，
/// 避免一次请求把内存与磁盘同时打满。
const MAX_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;

/// 处理文件上传：`POST /upload?path=<Assets相对路径>&token=<可选>`
///
/// 文件写入 `assets_dir()/path`（即 `root_dir/Assets/path`），
/// 与 GET 端点的 `/<path>` 布局完全一致，客户端无需任何改动即可命中。
pub(super) fn handle_upload(req: &mut Request, cfg: &Arc<AssetsConfig>, query: &str) -> Resp {
    // 鉴权：必须提供匹配的令牌（未配置 token 的服务器一律拒绝写入）
    if let Some(resp) = require_token(req, cfg, query) {
        return resp;
    }

    // 必须解码：`%2F` 不解码会被当成字面量写进文件名（详见 query::percent_decode 注释）
    let rel = match query_path(query, "path") {
        Some(p) if !p.is_empty() => p,
        _ => return json_status(400, r#"{"ok":false,"error":"missing path"}"#),
    };

    let target = match safe_join(&cfg.assets_dir(), &rel) {
        Some(t) => t,
        None => return json_status(400, r#"{"ok":false,"error":"invalid path"}"#),
    };

    // 先按 Content-Length 快速拒绝，再**边读边限长**（这个头本身不可信）
    if let Some(len) =
        header_value(req, "Content-Length").and_then(|v| v.trim().parse::<u64>().ok())
    {
        if len > MAX_UPLOAD_BYTES {
            return json_status(413, r#"{"ok":false,"error":"payload too large"}"#);
        }
    }

    // 读取请求体（带硬上限：此前是 read_to_end 全量入内存，配上"任何能连上的人
    // 都能上传"就是一条现成的内存/磁盘打满路径）
    let body = {
        let mut reader = req.as_reader().take(MAX_UPLOAD_BYTES + 1);
        let mut buf = Vec::new();
        match Read::read_to_end(&mut reader, &mut buf) {
            Ok(_) => buf,
            Err(e) => {
                return json_status(
                    500,
                    &format!("{{\"ok\":false,\"error\":\"read body: {}\"}}", e),
                );
            }
        }
    };
    if body.len() as u64 > MAX_UPLOAD_BYTES {
        return json_status(413, r#"{"ok":false,"error":"payload too large"}"#);
    }

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

/// 删除一个已上传的文件（`POST /delete?path=<Assets 相对路径>&token=<token>`）。
///
/// 存在的理由：只有上传没有删除时，旧版本更新包会永久堆积（每版约 30MB）且无法回收。
/// 与上传同样受 token 保护；`safe_join` 保证只能删 `Assets/` 目录内的文件。
/// 文件不存在返回 404——调用方应把它当作"已经清理过"，而不是错误。
pub(super) fn handle_delete(req: &Request, cfg: &Arc<AssetsConfig>, query: &str) -> Resp {
    if let Some(resp) = require_token(req, cfg, query) {
        return resp;
    }

    // 与上传同理：路径是编码过的，不解码就删不到任何带目录的文件
    let rel = match query_path(query, "path") {
        Some(p) if !p.is_empty() => p,
        _ => return json_status(400, r#"{"ok":false,"error":"missing path"}"#),
    };

    let target = match safe_join(&cfg.assets_dir(), &rel) {
        Some(t) => t,
        None => return json_status(400, r#"{"ok":false,"error":"invalid path"}"#),
    };

    if !target.is_file() {
        return json_status(404, r#"{"ok":false,"error":"not found"}"#);
    }

    match std::fs::remove_file(&target) {
        Ok(_) => {
            eprintln!("[delete] 已删除 {}", target.display());
            json_status(200, r#"{"ok":true}"#)
        }
        Err(e) => json_status(
            500,
            &format!("{{\"ok\":false,\"error\":\"delete: {}\"}}", e),
        ),
    }
}

/// 写入类接口的鉴权。
///
/// **未配置 token 时直接拒绝**（fail-closed），理由见模块头注释。
pub(super) fn require_token(req: &Request, cfg: &AssetsConfig, query: &str) -> Option<Resp> {
    if cfg.upload_token.is_empty() {
        eprintln!(
            "[鉴权] 拒绝写入：服务器未配置 upload_token（用 --token 或 ASSET_UPLOAD_TOKEN 设置）"
        );
        return Some(json_status(
            503,
            r#"{"ok":false,"error":"upload disabled: no upload_token configured"}"#,
        ));
    }
    // 令牌走**未解码**的 query_param：令牌里可能含 `%`，解码会把它破坏掉
    let provided = token_from_header(req).or_else(|| query_param(query, "token"));
    match provided {
        Some(t) if constant_time_eq(t.as_bytes(), cfg.upload_token.as_bytes()) => None,
        _ => Some(json_status(401, r#"{"ok":false,"error":"unauthorized"}"#)),
    }
}

/// 从请求头取令牌：`Authorization: Bearer <t>` 优先，其次 `X-Token: <t>`。
///
/// 令牌走 URL 会进访问日志、反向代理日志与 Referer，头传递才是正确姿势；
/// query 形式仅为兼容既有发布脚本而保留（日志里会脱敏）。
fn token_from_header(req: &Request) -> Option<String> {
    for name in ["Authorization", "X-Token"] {
        if let Some(h) = req.headers().iter().find(|h| h.field.equiv(name)) {
            let v = h.value.as_str().trim();
            let v = v.strip_prefix("Bearer ").unwrap_or(v).trim();
            return Some(v.to_string());
        }
    }
    None
}

/// 常量时间比较，避免用 `!=` 逐字节短路泄漏令牌前缀。
pub(super) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 访问日志脱敏：把 `token=...` 换成 `token=***`。
pub(super) fn redact_token(url: &str) -> String {
    match url.split_once('?') {
        None => url.to_string(),
        Some((path, query)) => {
            let cleaned: Vec<String> = query
                .split('&')
                .map(|pair| match pair.split_once('=') {
                    Some((k, _)) if k.eq_ignore_ascii_case("token") => "token=***".to_string(),
                    _ => pair.to_string(),
                })
                .collect();
            format!("{}?{}", path, cleaned.join("&"))
        }
    }
}
