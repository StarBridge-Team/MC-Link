//! 响应类型与构造辅助。
//!
//! 单独成模块的理由：这里集中了"响应体怎么装箱、长度怎么表达、CORS 与错误响应
//! 怎么写"这些**容易被各处重复实现**的细节——例如 `Content-Length` 与
//! `data_length` 在 `tiny_http` 里是互斥的，写错一处就会让客户端探测不到大小
//! （进而永远启用不了分片与续传）。集中一处才好守住。

use tiny_http::{Header, Request, Response, StatusCode};

/// 响应体类型。
///
/// 用 boxed reader 而不是 `Cursor<Vec<u8>>`：更新包有 30MB 量级，
/// 整份读进内存再发，既吃内存又拖垮并发（连 Range 请求也要先把整包读满）。
/// 文件类响应因此直接从磁盘流式发出，只把请求到的那一段读出来。
pub(super) type Body = Box<dyn std::io::Read + Send>;

/// 本服务统一的响应类型。
pub(super) type Resp = Response<Body>;

/// 用内存数据构造响应（装箱成流式体，并显式声明长度）。
///
/// `tiny_http::Response` 没有 `map`，所以这里必须直接构造；
/// 显式给 `data_length` 是为了让响应一定带 `Content-Length`
/// （客户端据此探测大小与续传，见 `files::file_stream_response` 的注释）。
pub(super) fn data_response(data: Vec<u8>, status: u16) -> Resp {
    let len = data.len();
    Response::new(
        StatusCode(status),
        Vec::new(),
        Box::new(std::io::Cursor::new(data)) as Body,
        Some(len),
        None,
    )
}

pub(super) fn json_response(body: &str, cors: &str) -> Resp {
    text_response(body, "application/json; charset=utf-8", cors)
}

pub(super) fn text_response(body: &str, mime: &str, cors: &str) -> Resp {
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

pub(super) fn cors_preflight(cors: &str) -> Resp {
    let mut resp = data_response(Vec::new(), 204);
    add_cors(&mut resp, cors);
    if let Ok(h) = Header::from_bytes(
        "Access-Control-Allow-Headers",
        "Authorization, X-Token, Content-Type",
    ) {
        resp.add_header(h);
    }
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS") {
        resp.add_header(h);
    }
    resp
}

pub(super) fn add_cors(resp: &mut Resp, cors: &str) {
    if cors.is_empty() {
        return;
    }
    if let Ok(h) = Header::from_bytes("Access-Control-Allow-Origin", cors.as_bytes()) {
        resp.add_header(h);
    }
}

pub(super) fn not_found() -> Resp {
    data_response(b"404 Not Found".to_vec(), 404)
}

pub(super) fn bad_request(msg: &str) -> Resp {
    data_response(msg.as_bytes().to_vec(), 400)
}

pub(super) fn method_not_allowed() -> Resp {
    data_response(b"405 Method Not Allowed".to_vec(), 405)
}

/// 返回 JSON 状态响应（写入类接口用，始终带 `Content-Type`）。
pub(super) fn json_status(status: u16, body: &str) -> Resp {
    let mut resp = data_response(body.as_bytes().to_vec(), status);
    if let Ok(h) = Header::from_bytes("Content-Type", "application/json; charset=utf-8".as_bytes())
    {
        resp.add_header(h);
    }
    resp
}

/// 读取请求头（大小写不敏感）。
///
/// 不用 `HeaderField::equiv`：它要求传入 `&'static str`，而这里的头名来自运行时参数。
pub(super) fn header_value(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}
