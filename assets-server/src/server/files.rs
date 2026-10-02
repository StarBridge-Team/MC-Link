//! 静态文件响应：MIME 推断、`Range` 断点续传、元配置与目录清单。
//!
//! # 为什么续传要自己实现
//!
//! `tiny_http` 不处理 `Range` 头（实测带 `Range` 的请求仍返回 200 + chunked），
//! 而客户端要能分片下载与断点续传，就必须有人返回 206 + `Content-Range`。

use std::io::Read;
use std::path::Path;

use tiny_http::{Header, Response, StatusCode};

use crate::config::AssetsConfig;
use crate::meta::SettingMeta;

use super::paths::{resolve_within, sanitize_path};
use super::response::{
    add_cors, bad_request, data_response, json_response, not_found, text_response, Body, Resp,
};

/// 一次文件请求中与传输相关的上下文。
///
/// 把 `Range` 与 CORS 一起往下传，是因为续传必须由我们实现（见模块头注释）。
pub(super) struct ServeCtx<'a> {
    pub(super) cors: &'a str,
    /// `Range` 头原文；`None` 表示请求整个文件
    pub(super) range: Option<String>,
}

/// 提供普通静态文件（相对 `dir`，已做穿越防护）。
pub(super) fn serve_file(dir: &Path, rel: &str, ctx: &ServeCtx) -> Resp {
    let safe = sanitize_path(rel);
    if safe.as_os_str().is_empty() {
        return bad_request("无效路径");
    }
    // 先算安全路径再读：`dir.join(&safe)` 单独用是不够的（见 sanitize_path 的注释）
    match resolve_within(dir, &safe) {
        Some(full) => serve_abs(&full, ctx),
        None => not_found(),
    }
}

/// 提供更新包文件。
///
/// 优先从 `Assets/update/` 读取——**与上传端点 `POST /upload?path=update/...` 的布局一致**，
/// 否则会出现"上传成功但下载 404"的静默不一致（历史实现读的是 `Updates/` 目录）。
/// 同时兼容旧的 `Updates/` 目录，便于平滑迁移已部署的服务器。
pub(super) fn serve_update(cfg: &AssetsConfig, rel: &str, ctx: &ServeCtx) -> Resp {
    let safe = sanitize_path(rel);
    if safe.as_os_str().is_empty() {
        return bad_request("无效路径");
    }

    if let Some(full) = resolve_within(&cfg.assets_dir().join("update"), &safe) {
        return serve_abs(&full, ctx);
    }
    if let Some(full) = resolve_within(&cfg.updates_dir(), &safe) {
        return serve_abs(&full, ctx);
    }

    not_found()
}

/// 读取一个已知安全的绝对路径并按扩展名推断 MIME，支持 Range 断点续传。
///
/// 响应约定：
/// - 无 `Range`（或该头无效/多段）→ 200 + 全量，但**仍带 `Accept-Ranges: bytes`**，
///   客户端据此判断这份资源将来能否续传；
/// - 合法且可满足的 `Range` → 206 + `Content-Range` + 对应切片；
/// - 起点超出文件长度 → 416（RFC 9110 要求），客户端会退回整体下载。
fn serve_abs(full: &Path, ctx: &ServeCtx) -> Resp {
    // 只取元数据、不读内容：文件可能几十 MB，而我们要按区间从磁盘流式发出。
    // （此前无论请求哪一段都先把整份读进内存，连 Range 请求也躲不掉。）
    let total = match std::fs::metadata(full) {
        Ok(m) if m.is_file() => m.len(),
        _ => return not_found(),
    };
    let mime = mime_guess::from_path(full)
        .first_or_octet_stream()
        .to_string();

    match ctx.range.as_deref().map(|h| parse_range(h, total)) {
        Some(RangeSpec::Partial(start, end)) => file_stream_response(
            full,
            &mime,
            206,
            Some(format!("bytes {}-{}/{}", start, end, total)),
            (start, end - start + 1),
            ctx.cors,
        ),
        Some(RangeSpec::Unsatisfiable) => {
            let mut resp = data_response(b"416 Range Not Satisfiable".to_vec(), 416);
            // 416 必须带 `bytes */<总长>`，客户端据此知道该怎么调整请求
            if let Ok(h) =
                Header::from_bytes("Content-Range", format!("bytes */{}", total).as_bytes())
            {
                resp.add_header(h);
            }
            if let Ok(h) = Header::from_bytes("Accept-Ranges", "bytes") {
                resp.add_header(h);
            }
            add_cors(&mut resp, ctx.cors);
            resp
        }
        // 无 Range 或该头无效：按整体响应（RFC 允许忽略无法解析的 Range）
        _ => file_stream_response(full, &mime, 200, None, (0, total), ctx.cors),
    }
}

/// 从磁盘流式返回 `[start, start + len)` 这一段（200 全量或 206 分片）。
///
/// 数据源是文件 + `take(len)`，由 tiny_http 边读边发：30MB 的更新包不会整份驻留内存。
/// 两个容易踩的点（改这段前先读一遍）：
/// - **不要手工设置 `Content-Length`**：`tiny_http` 按数据源长度自行处理，
///   手工设置反而可能让该头不出现（文档明确二者互斥）。这里通过 `data_length` 表达。
/// - **关掉 chunked 阈值**：默认超过 32KB 就改用 chunked 传输，那样响应里没有
///   `Content-Length`，客户端探测不到文件大小，分片与续传就永远启用不了。
fn file_stream_response(
    full: &Path,
    mime: &str,
    status: u16,
    content_range: Option<String>,
    range: (u64, u64),
    cors: &str,
) -> Resp {
    let (start, len) = range;
    let mut file = match std::fs::File::open(full) {
        Ok(f) => f,
        Err(_) => return not_found(),
    };
    if start > 0 {
        use std::io::Seek;
        if file.seek(std::io::SeekFrom::Start(start)).is_err() {
            return not_found();
        }
    }

    let body: Body = Box::new(file.take(len));
    let mut resp = Response::new(
        StatusCode(status),
        Vec::new(),
        body,
        Some(len as usize),
        None,
    )
    .with_chunked_threshold(usize::MAX);

    if let Ok(h) = Header::from_bytes("Content-Type", mime.as_bytes()) {
        resp.add_header(h);
    }
    // 无论 200 还是 206 都声明支持范围请求：客户端据此决定这份资源能否续传
    if let Ok(h) = Header::from_bytes("Accept-Ranges", "bytes") {
        resp.add_header(h);
    }
    if let Some(cr) = content_range {
        if let Ok(h) = Header::from_bytes("Content-Range", cr.as_bytes()) {
            resp.add_header(h);
        }
    }
    add_cors(&mut resp, cors);
    resp
}

/// 解析 `Range` 头的结果。
#[derive(Debug, PartialEq, Eq)]
pub(super) enum RangeSpec {
    /// 不是我们支持的形态（非 `bytes=`、多段、语法错误）→ 按整体响应处理
    Ignore,
    /// 起点越界或零长度后缀 → 416
    Unsatisfiable,
    /// 命中的闭区间 `[start, end]`
    Partial(u64, u64),
}

/// 解析单段 `Range: bytes=start-end`。
///
/// 只实现单段：多段（`bytes=0-1,5-6`）需要 `multipart/byteranges` 响应体，
/// 而我们的客户端只会请求单段，遇到多段直接整体响应即可。
pub(super) fn parse_range(header: &str, total: u64) -> RangeSpec {
    let Some(spec) = header.trim().strip_prefix("bytes=") else {
        return RangeSpec::Ignore;
    };
    if spec.contains(',') {
        return RangeSpec::Ignore;
    }
    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return RangeSpec::Ignore;
    };
    let (start_raw, end_raw) = (start_raw.trim(), end_raw.trim());

    // `-N`：最后 N 字节
    if start_raw.is_empty() {
        let Ok(n) = end_raw.parse::<u64>() else {
            return RangeSpec::Ignore;
        };
        if n == 0 || total == 0 {
            return RangeSpec::Unsatisfiable;
        }
        let n = n.min(total);
        return RangeSpec::Partial(total - n, total - 1);
    }

    let Ok(start) = start_raw.parse::<u64>() else {
        return RangeSpec::Ignore;
    };
    if start >= total {
        return RangeSpec::Unsatisfiable;
    }

    let end = if end_raw.is_empty() {
        total - 1
    } else {
        match end_raw.parse::<u64>() {
            // `end` 超出末尾时按末尾截断（RFC 允许）
            Ok(end) => end.min(total - 1),
            Err(_) => return RangeSpec::Ignore,
        }
    };

    if end < start {
        // `bytes=5-2` 属于非法字段，按忽略处理
        return RangeSpec::Ignore;
    }
    RangeSpec::Partial(start, end)
}

/// 提供元配置文件（同时返回 YAML 与 CORS 头）
pub(super) fn serve_meta(dir: &Path, file: &str, cors: &str) -> Resp {
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
pub(super) fn serve_meta_index(dir: &Path, cors: &str) -> Resp {
    let mut sections: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name
                    .strip_suffix(".yml")
                    .or_else(|| name.strip_suffix(".yaml"))
                {
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

/// 列出更新目录下的文件名（`GET /update/list`）。
///
/// 发布脚本据此回收旧版本包：它必须知道服务器上实际存在哪些文件，
/// 而服务器没有目录浏览能力，所以单开一个只读清单端点。
pub(super) fn list_update_files(cfg: &AssetsConfig) -> Resp {
    let dir = cfg.assets_dir().join("update");
    let mut files: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    files.push(name.to_string());
                }
            }
        }
    }
    files.sort();

    let body = serde_json::to_string(&serde_json::json!({ "files": files }))
        .unwrap_or_else(|_| r#"{"files":[]}"#.to_string());
    json_response(&body, &cfg.cors_origin)
}
