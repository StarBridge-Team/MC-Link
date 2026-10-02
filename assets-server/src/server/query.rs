//! 查询串解析与百分号解码。
//!
//! 关键约定：**`path` 参数要解码，`token` 参数不要解码**。令牌里可能含 `%`，
//! 解码会把它破坏掉；而路径是客户端用 `encodeURIComponent` 编过的，不解码就会
//! 把 `%2F` 当成字面量写进文件名（详见 [`percent_decode`]）。

/// 解析查询参数中的某个 key（**不做解码**）。
pub(super) fn query_param(query: &str, key: &str) -> Option<String> {
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
pub(super) fn percent_decode(s: &str) -> String {
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
/// 令牌**不能**走这条路径，理由见模块头注释。
pub(super) fn query_path(query: &str, key: &str) -> Option<String> {
    query_param(query, key).map(|v| percent_decode(&v))
}
