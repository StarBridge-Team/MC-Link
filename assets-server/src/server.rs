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
            // 启动时把写入模式说清楚：否则"上传一直 503"会被当成 bug 排查半天
            if cfg.upload_token.is_empty() {
                println!("  写入: 已禁用（未配置 upload_token，/upload 与 /delete 一律拒绝）");
            } else {
                println!("  写入: 已启用（需要 Authorization: Bearer <token>）");
            }
            println!("===========================================");
            println!();
            s
        }
        Err(e) => {
            eprintln!("[启动] 服务器绑定 {} 失败: {}", listen, e);
            eprintln!(
                "       最常见的原因是有另一个实例仍在运行：先结束它，或换个端口（--port <端口>）"
            );
            // 必须以非零码退出：否则 `concurrently` / CI 会把"端口被占"当成正常结束，
            // 前台只剩一句 vite 的端口冲突，真正的原因被淹没（实际排查时踩过）。
            std::process::exit(1);
        }
    };

    // 多工作线程：单线程时一个慢客户端（或一次大文件传输）就能阻塞**全部**请求，
    // 是现成的慢速 DoS。`tiny_http::Server` 是 Sync 的，多线程同时 recv 即可。
    let server = Arc::new(server);
    let mut workers = Vec::new();
    for _ in 0..HTTP_WORKERS {
        let server = server.clone();
        let cfg = cfg.clone();
        workers.push(std::thread::spawn(move || loop {
            let mut request = match server.recv() {
                Ok(r) => r,
                // 连接中断之类不是错误，继续等下一个请求
                Err(_) => continue,
            };

            if cfg.access_log {
                // 脱敏后再打：URL 里的 `?token=` 会被写进 stdout、反代日志与 Referer
                println!(
                    "[{}] {}",
                    request.method(),
                    redact_token(&request.url().to_string())
                );
            }

            let response = route(&mut request, &cfg);

            let _ = request.respond(response);
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
}

fn route(req: &mut Request, cfg: &Arc<AssetsConfig>) -> Resp {
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

    // 删除接口：仅允许 POST /delete（与上传同样需要 token）。
    // 存在的理由：资源服务器只有上传没有删除时，旧版本的更新包会永久堆积
    //（每个版本约 30MB），且没有任何办法回收。
    if path == "/delete" {
        if method != Method::Post {
            return method_not_allowed();
        }
        return handle_delete(req, cfg, query);
    }

    // 仅允许 GET / HEAD / OPTIONS
    if method != Method::Get && method != Method::Head && method != Method::Options {
        return method_not_allowed();
    }

    // CORS 预检
    if method == Method::Options {
        return cors_preflight(&cfg.cors_origin);
    }

    // 把 Range 与 CORS 一起传给文件响应函数：tiny_http 不处理 Range，
    // 断点续传完全由我们实现（详见 serve_abs）
    let ctx = ServeCtx {
        cors: &cfg.cors_origin,
        range: header_value(req, "Range"),
    };

    match path {
        "/health" => json_response(r#"{"status":"ok"}"#, &cfg.cors_origin),

        // 资源清单（客户端据此判断是否需要重新下载）
        "/manifest.json" => {
            serve_file(&cfg.assets_dir(), "manifest.json", &ctx)
        }

        // Bootstrap Icons
        p if p.starts_with("/bootstrap-icons/") => {
            let rel = p.trim_start_matches("/bootstrap-icons/");
            serve_file(&cfg.assets_dir().join("bootstrap-icons"), rel, &ctx)
        }

        // 字体
        p if p.starts_with("/fonts/") => {
            let rel = p.trim_start_matches("/fonts/");
            serve_file(&cfg.assets_dir().join("fonts"), rel, &ctx)
        }

        // 图标（应用图标、托盘图标等）
        p if p.starts_with("/icons/") => {
            let rel = p.trim_start_matches("/icons/");
            serve_file(&cfg.assets_dir().join("icons"), rel, &ctx)
        }

        // 适配器校验清单（客户端据此校验适配器包的 SHA256）
        "/adapter/manifest.json" => {
            serve_file(&cfg.assets_dir().join("adapter"), "manifest.json", &ctx)
        }

        // 适配器包文件（存放于 Assets/adapter/，与上传端点布局一致）
        p if p.starts_with("/adapter/") => {
            let rel = p.trim_start_matches("/adapter/");
            serve_file(&cfg.assets_dir().join("adapter"), rel, &ctx)
        }

        // 页面清单
        "/pages/manifest.json" => {
            serve_file(&cfg.pages_dir(), "manifest.json", &ctx)
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
            serve_file(&cfg.pages_dir(), &rel, &ctx)
        }

        // 更新目录文件清单（供发布脚本回收旧版本包；必须排在下面的前缀匹配之前）
        "/update/list" | "/updates/list" => list_update_files(&cfg),

        // 更新清单
        "/update/latest.json" | "/updates/latest.json" => {
            serve_update(&cfg, "latest.json", &ctx)
        }

        // 更新包文件
        p if p.starts_with("/update/") || p.starts_with("/updates/") => {
            let rel = p
                .strip_prefix("/update/")
                .or_else(|| p.strip_prefix("/updates/"))
                .unwrap_or("");
            serve_update(&cfg, rel, &ctx)
        }

        // 设置项清单（列出所有可用的设置分区）
        "/settings/manifest.json" => {
            serve_file(&cfg.setting_meta_dir(), "manifest.json", &ctx)
        }

        // 设置项元配置
        p if p.starts_with("/settings/meta/") => {
            let section = p.trim_start_matches("/settings/meta/");
            let safe = sanitize(section);
            if safe.is_empty() {
                return bad_request("无效的 section");
            }
            let file = format!("{}.yml", safe);
            serve_meta(&cfg.setting_meta_dir(), &file, ctx.cors)
        }

        // 整个 SettingMeta 目录索引（列出可用的 section）
        "/settings/meta" | "/settings/meta/" => {
            serve_meta_index(&cfg.setting_meta_dir(), &cfg.cors_origin)
        }

        _ => not_found(),
    }
}

/// 响应体类型。
///
/// 用 boxed reader 而不是 `Cursor<Vec<u8>>`：更新包有 30MB 量级，
/// 整份读进内存再发，既吃内存又拖垮并发（连 Range 请求也要先把整包读满）。
/// 文件类响应现在直接从磁盘流式发出，只把请求到的那一段读出来。
type Body = Box<dyn std::io::Read + Send>;
/// 本模块统一的响应类型。
type Resp = Response<Body>;

/// 用内存数据构造响应（装箱成流式体，并显式声明长度）。
///
/// `tiny_http::Response` 没有 `map`，所以这里必须直接构造；
/// 显式给 `data_length` 是为了让响应一定带 `Content-Length`
/// （客户端据此探测大小与续传，见 `file_response` 的注释）。
fn data_response(data: Vec<u8>, status: u16) -> Resp {
    let len = data.len();
    Response::new(
        StatusCode(status),
        Vec::new(),
        Box::new(std::io::Cursor::new(data)) as Body,
        Some(len),
        None,
    )
}

/// 处理请求的工作线程数。
///
/// 单线程时一个慢客户端就能阻塞全部请求（慢速 DoS），而下载大包时这种阻塞很常见。
const HTTP_WORKERS: usize = 4;

/// 单次上传的体积上限。
///
/// 更新包约 30MB，留出余量；这里的意义是给"任何能连上的人"设一个天花板，
/// 避免一次请求把内存与磁盘同时打满。
const MAX_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;

/// 一次文件请求中与传输相关的上下文。
///
/// 把 `Range` 与 CORS 一起往下传，是因为**续传必须由我们实现**：
/// `tiny_http` 不会处理 `Range` 头（实测带 `Range` 的请求仍返回 200 + chunked），
/// 客户端要能分片/续传，就得有人返回 206 + `Content-Range`。
struct ServeCtx<'a> {
    cors: &'a str,
    /// `Range` 头原文；`None` 表示请求整个文件
    range: Option<String>,
}

/// 提供普通静态文件
fn serve_file(dir: &Path, rel: &str, ctx: &ServeCtx) -> Resp {
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
fn serve_update(cfg: &AssetsConfig, rel: &str, ctx: &ServeCtx) -> Resp {
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
            if let Ok(h) = Header::from_bytes(
                "Content-Range",
                format!("bytes */{}", total).as_bytes(),
            ) {
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
enum RangeSpec {
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
fn parse_range(header: &str, total: u64) -> RangeSpec {
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

/// 读取请求头（大小写不敏感）。
///
/// 不用 `HeaderField::equiv`：它要求传入 `&'static str`，而我们的头名来自运行时参数。
fn header_value(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

/// 提供元配置文件（同时返回 YAML 与说明 CORS 头）
fn serve_meta(dir: &Path, file: &str, cors: &str) -> Resp {
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
fn serve_meta_index(dir: &Path, cors: &str) -> Resp {
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


fn json_response(body: &str, cors: &str) -> Resp {
    text_response(body, "application/json; charset=utf-8", cors)
}

fn text_response(body: &str, mime: &str, cors: &str) -> Resp {
    let bytes = body.as_bytes().to_vec();
    // 长度由 `data_response` 通过 data_length 表达：手工设置 Content-Length
    // 反而可能让该头不出现（tiny_http 文档明确二者互斥）
    let mut resp = data_response(bytes, 200);
    if let Ok(h) = Header::from_bytes("Content-Type", mime.as_bytes()) {
        resp.add_header(h);
    }
    add_cors(&mut resp, cors);
    resp
}

fn cors_preflight(cors: &str) -> Resp {
    let mut resp = data_response(Vec::new(), 204);
    add_cors(&mut resp, cors);
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Headers", "Authorization, X-Token, Content-Type") {
        resp.add_header(h);
    }
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS") {
        resp.add_header(h);
    }
    resp
}

fn add_cors(resp: &mut Resp, cors: &str) {
    if cors.is_empty() {
        return;
    }
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Origin", cors.as_bytes()) {
        resp.add_header(h);
    }
}

fn not_found() -> Resp {
    data_response(b"404 Not Found".to_vec(), 404)
}

fn bad_request(msg: &str) -> Resp {
    data_response(msg.as_bytes().to_vec(), 400)
}

fn method_not_allowed() -> Resp {
    data_response(b"405 Method Not Allowed".to_vec(), 405)
}

/// 清理路径，仅保留安全字符
fn sanitize(input: &str) -> String {
    input.chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

/// 防止路径穿越：把相对路径规整为安全的分段序列。
///
/// 必须同时处理三类逃逸（历史实现只按 `/` 切分，注释却写着"已处理反斜杠"）：
/// - Windows 上 `\` 也是分隔符：`..\..\Windows\win.ini` 不会被 `/` 切开；
/// - 盘符/前缀：`C:\Windows\win.ini` 经 `PathBuf::push` 会**整体替换**已有路径；
/// - 因此还叠一层 [`resolve_within`] 做 canonicalize + 包含性校验，本函数只做粗筛。
fn sanitize_path(input: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in input.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            continue;
        }
        out.push(part);
    }
    out
}

/// 把 `rel` 解析到 `base` 之内，并确认结果**确实落在 base 里**。
///
/// 返回 `None` 表示越界或不存在，调用方一律按 404 处理（不区分二者，
/// 免得把目录结构变成探测信号）。
///
/// 用 `canonicalize` 而非字符串前缀判断，是为了连符号链接一起挡掉：
/// 光比字符串挡不住"在 base 内放一个指向 `C:\Windows` 的软链接"。
fn resolve_within(base: &Path, rel: &Path) -> Option<PathBuf> {
    let real_base = base.canonicalize().ok()?;
    let real = real_base.join(rel).canonicalize().ok()?;
    if real.starts_with(&real_base) {
        Some(real)
    } else {
        None
    }
}

/// 处理文件上传：`POST /upload?path=<Assets相对路径>&token=<可选>`
///
/// 文件写入 `assets_dir()/path`（即 `root_dir/Assets/path`），
/// 与 GET 端点的 `/<path>` 布局完全一致，客户端无需任何改动即可命中。
fn handle_upload(
    req: &mut Request,
    cfg: &Arc<AssetsConfig>,
    query: &str,
) -> Resp {
    // 鉴权：必须提供匹配的令牌（未配置 token 的服务器一律拒绝写入）
    if let Some(resp) = require_token(req, cfg, query) {
        return resp;
    }

    // 必须解码：`%2F` 不解码会被当成字面量写进文件名（详见 percent_decode 注释）
    let rel = match query_path(query, "path") {
        Some(p) if !p.is_empty() => p,
        _ => return json_status(400, r#"{"ok":false,"error":"missing path"}"#),
    };

    let target = match safe_join(&cfg.assets_dir(), &rel) {
        Some(t) => t,
        None => return json_status(400, r#"{"ok":false,"error":"invalid path"}"#),
    };

    // 先按 Content-Length 快速拒绝，再**边读边限长**（这个头本身不可信）
    if let Some(len) = header_value(req, "Content-Length")
        .and_then(|v| v.trim().parse::<u64>().ok())
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
                return json_status(500, &format!("{{\"ok\":false,\"error\":\"read body: {}\"}}", e));
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

/// 列出更新目录下的文件名（`GET /update/list`）。
///
/// 发布脚本据此回收旧版本包：它必须知道服务器上实际存在哪些文件，
/// 而服务器没有目录浏览能力，所以单开一个只读清单端点。
fn list_update_files(cfg: &AssetsConfig) -> Resp {
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

/// 删除一个已上传的文件（`POST /delete?path=<Assets 相对路径>&token=<token>`）。
///
/// 存在的理由：只有上传没有删除时，旧版本更新包会永久堆积（每版约 30MB）且无法回收。
/// 与上传同样受 token 保护；`safe_join` 保证只能删 `Assets/` 目录内的文件。
/// 文件不存在返回 404——调用方应把它当作"已经清理过"，而不是错误。
fn handle_delete(
    req: &Request,
    cfg: &Arc<AssetsConfig>,
    query: &str,
) -> Resp {
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
        Err(e) => json_status(500, &format!("{{\"ok\":false,\"error\":\"delete: {}\"}}", e)),
    }
}

/// 写入类接口的鉴权。
///
/// **未配置 token 时直接拒绝**（fail-closed）。历史行为是"token 为空 = 关闭鉴权"，
/// 于是按仓库自带配置启动的服务器允许任何人覆盖 `update/latest.json` 与更新包
/// —— 等于把"给全体客户端投毒"的能力开放给整个网络。
fn require_token(
    req: &Request,
    cfg: &AssetsConfig,
    query: &str,
) -> Option<Resp> {
    if cfg.upload_token.is_empty() {
        eprintln!("[鉴权] 拒绝写入：服务器未配置 upload_token（用 --token 或 ASSET_UPLOAD_TOKEN 设置）");
        return Some(json_status(
            503,
            r#"{"ok":false,"error":"upload disabled: no upload_token configured"}"#,
        ));
    }
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
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
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
fn redact_token(url: &str) -> String {
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

/// 百分号解码（`%XX` → 对应字节）。
///
/// 发布脚本用 `encodeURIComponent` 编过路径再拼进 URL，服务端必须解回来：
/// 不解码时 `bootstrap-icons%2Fbootstrap-icons.css` 里的 `%2F` 是**字面量**，
/// 会被当成"一个文件名"直接写盘，而不是放进 `bootstrap-icons/` 目录。
/// 后果是资源永远 404，而上传日志一片成功——现象与原因隔得很远，极难排查。
///
/// 只解 `%XX`，**不**把 `+` 当空格：`+` 在路径里是合法字符，那是 form 编码的
/// 约定，不适用此处（客户端用的是 `encodeURIComponent`）。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 取查询参数**并解码**（用于 `path` 这类由客户端编码过的取值）。
///
/// 令牌**不能**走这条路径：令牌里也可能含 `%`，解码会把它破坏掉；
/// `require_token` 用的是未解码的 `query_param`，这是刻意的。
fn query_path(query: &str, key: &str) -> Option<String> {
    query_param(query, key).map(|v| percent_decode(&v))
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

#[cfg(test)]
mod tests {
    use super::RangeSpec::{Ignore, Partial, Unsatisfiable};
    use super::*;

    #[test]
    fn parses_plain_and_open_ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Partial(0, 9));
        assert_eq!(parse_range("bytes=90-", 100), Partial(90, 99));
        // 末尾越界按末尾截断（RFC 允许）
        assert_eq!(parse_range("bytes=50-999", 100), Partial(50, 99));
        // 容忍空白
        assert_eq!(parse_range("bytes= 10 - 20 ", 100), Partial(10, 20));
    }

    #[test]
    fn parses_suffix_ranges() {
        assert_eq!(parse_range("bytes=-10", 100), Partial(90, 99));
        // 后缀长度超过文件长度 → 整个文件
        assert_eq!(parse_range("bytes=-500", 100), Partial(0, 99));
    }

    /// 上传路径是 `encodeURIComponent` 编过的，必须解回目录分隔符。
    ///
    /// 不解时 `%2F` 是字面量 → 文件被写成 `bootstrap-icons%2F...css` 这种名字，
    /// 服务端随后一律 404，而上传日志全部成功。
    #[test]
    fn percent_decode_restores_directory_separators() {
        assert_eq!(
            percent_decode("bootstrap-icons%2Fbootstrap-icons.css"),
            "bootstrap-icons/bootstrap-icons.css"
        );
        assert_eq!(percent_decode("fonts/poppins.css"), "fonts/poppins.css");
        assert_eq!(percent_decode("a%20b.txt"), "a b.txt");
        // 非法转义原样保留，不吞掉字符
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%2"), "%2");
    }

    /// 解码必须在 `safe_join` **之前**发生，否则 `%2e%2e%2f` 不会被识别为穿越。
    #[test]
    fn decoded_traversal_is_still_rejected() {
        let base = std::path::Path::new(if cfg!(windows) {
            "C:\\tmp\\mc-link-assets-test"
        } else {
            "/tmp/mc-link-assets-test"
        });
        assert!(safe_join(base, &percent_decode("..%2F..%2FWindows%2Fwin.ini")).is_none());
        assert!(safe_join(base, &percent_decode("%2Fetc%2Fpasswd")).is_none());
        // 正常的带目录路径必须放行（这正是修复前被误伤的场景）
        assert!(safe_join(base, &percent_decode("bootstrap-icons%2Ficons.css")).is_some());
    }

    /// 路径穿越：只按 `/` 切分的旧实现会把 `..\..\Windows\win.ini` 原样放行。
    #[test]
    fn path_sanitization_blocks_windows_traversal() {
        let p = sanitize_path(r"..\..\..\Windows\win.ini");
        assert!(
            !p.to_string_lossy().contains(".."),
            "反斜杠穿越未被拦下: {:?}",
            p
        );
        assert!(!p.is_absolute());

        let p2 = sanitize_path(r"C:\Windows\win.ini");
        assert!(!p2.is_absolute(), "盘符未被拦下: {:?}", p2);
        assert!(!p2.to_string_lossy().contains(':'));

        assert_eq!(
            sanitize_path("fonts/a.woff2"),
            PathBuf::from("fonts").join("a.woff2")
        );
    }

    /// 包含性校验：越界与不存在都要被拦，正常路径要放行。
    #[test]
    fn resolve_within_rejects_escape_and_missing() {
        let tmp = std::env::temp_dir().join(format!("mclink-assets-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        std::fs::write(tmp.join("sub/ok.txt"), b"hi").unwrap();

        assert!(resolve_within(&tmp, Path::new("sub/ok.txt")).is_some());
        assert!(resolve_within(&tmp, Path::new("sub/none.txt")).is_none());

        // 目录外真实存在的文件也必须被拦（这才是真的逃逸测试）
        let outside = tmp
            .parent()
            .unwrap()
            .join(format!("mclink-outside-{}.txt", std::process::id()));
        std::fs::write(&outside, b"x").unwrap();
        let rel = PathBuf::from("..").join(outside.file_name().unwrap());
        assert!(
            resolve_within(&tmp, &rel).is_none(),
            "越界路径未被拦下: {:?}",
            rel
        );

        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn token_compare_is_constant_time() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn access_log_redacts_token() {
        let red = redact_token("/upload?path=update/latest.json&token=SECRET");
        assert!(!red.contains("SECRET"), "日志里仍带令牌: {}", red);
        assert!(red.contains("token=***"));
        // 无关查询参数原样保留
        assert_eq!(
            redact_token("/upload?path=a&x=1"),
            "/upload?path=a&x=1"
        );
        // 无查询串时不改动
        assert_eq!(redact_token("/fonts/a.woff2"), "/fonts/a.woff2");
    }

    #[test]
    fn unsatisfiable_ranges_are_reported() {
        assert_eq!(parse_range("bytes=100-", 100), Unsatisfiable);
        assert_eq!(parse_range("bytes=200-300", 100), Unsatisfiable);
        assert_eq!(parse_range("bytes=-0", 100), Unsatisfiable);
        // 空文件上任何范围都不可满足
        assert_eq!(parse_range("bytes=0-", 0), Unsatisfiable);
    }

    #[test]
    fn malformed_ranges_are_ignored_not_fatal() {
        // 语法错误按 RFC 忽略（回退整体响应），不能因此让下载失败
        assert_eq!(parse_range("items=0-9", 100), Ignore);
        assert_eq!(parse_range("bytes=abc", 100), Ignore);
        assert_eq!(parse_range("bytes=0-1,5-6", 100), Ignore);
        assert_eq!(parse_range("bytes=5-2", 100), Ignore);
        assert_eq!(parse_range("bytes=", 100), Ignore);
    }
}

/// 返回 JSON 状态响应
fn json_status(status: u16, body: &str) -> Resp {
    let mut resp = data_response(body.as_bytes().to_vec(), status);
    if let Ok(h) =
        Header::from_bytes("Content-Type", "application/json; charset=utf-8".as_bytes())
    {
        resp.add_header(h);
    }
    resp
}
