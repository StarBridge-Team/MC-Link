use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize, Debug)]
struct DeepLinkEvent {
    action: String,
    params: Vec<String>,
}

/// 解析 mclink:// 深度链接并向前端发送事件。
///
/// # URL 形态与 host/path 的坑
///
/// 约定格式是 `mclink://<action>/<param1>/<param2>/...`，例如 `mclink://join/ABCD`。
/// 但按 WHATWG/RFC 3986 的解析规则，**`mclink:` 是"特殊/授权型" scheme 时
/// 第一段会被当作 host**：
///
/// ```text
/// mclink://join/ABCD   →  host = "join",   path = "/ABCD"
/// mclink://plugin/a/b  →  host = "plugin", path = "/a/b"
/// ```
///
/// 所以**不能只看 `path()`**：那样会把 `action` 判成 `ABCD`、`params` 判成空，
/// 与前端契约（`action === "join"` 且 `params[0]` 是邀请码）正好相反，
/// 整条深链功能都是坏的。这里把 `host` 拼回路径最前面，两种写法都能正确解析：
///
/// ```text
/// mclink://join/ABCD      → action = "join",   params = ["ABCD"]
/// mclink:///join/ABCD     → 同上（兼容"三段斜杠"的写法）
/// ```
///
/// 顺便对每段做一次百分号解码：邀请码本身可能含 `/`（陶瓦会给出
/// `U/A90T-7XHQ-9T98-ECQV`），前端分享时会 `encodeURIComponent`。
pub fn handle_deep_link(app: &AppHandle, urls: &[tauri::Url]) {
    for url in urls {
        if url.scheme() != "mclink" {
            continue;
        }

        // host 与 path 都要取，且 host 要拼在最前面（见上方说明）
        let mut segments: Vec<String> = Vec::new();
        if let Some(host) = url.host_str() {
            if !host.is_empty() {
                segments.push(host.to_string());
            }
        }
        for part in url.path().split('/') {
            if !part.is_empty() {
                segments.push(part.to_string());
            }
        }

        if segments.is_empty() {
            eprintln!("[深链接] 忽略无法解析的链接: {}", url);
            continue;
        }

        // 百分号解码（失败就按原文处理：邀请码里出现孤立的 `%` 也不该丢掉整条链接）
        let decoded: Vec<String> = segments
            .iter()
            .map(|s| percent_decode(s).unwrap_or_else(|| s.clone()))
            .collect();

        let action = decoded[0].clone();
        let params = decoded[1..].to_vec();

        if params.is_empty() {
            eprintln!("[深链接] {} 没有携带参数，已忽略", action);
            continue;
        }

        let event = DeepLinkEvent { action, params };
        if let Err(e) = app.emit("deep-link", &event) {
            eprintln!("[深链接] 事件发送失败: {}", e);
        }
    }
}

/// 百分号解码；不是合法编码时返回 `None`。
///
/// 按**字节**操作，不切片 `&str`：`%` 后面跟多字节字符时 `s[i+1..i+3]`
/// 不是合法 char 边界，会 panic。
fn percent_decode(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_string());
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None; // 截断的转义序列
            }
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            match (hi, lo) {
                (Some(hi), Some(lo)) => out.push((hi * 16 + lo) as u8),
                _ => return None, // 非法十六进制
            }
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// 是否需要由程序自己登记 `mclink://` 协议。
///
/// 登记动作交给官方插件（`DeepLinkExt::register_all`），这里只回答"该不该登记"。
/// 为什么必须显式判定形态：安装版由安装器登记协议，程序再自己写一遍会与安装器
/// 打架，且卸载或挪动安装目录后会**残留一个指向失效路径的协议项**。
///
/// - **Linux**：插件用 xdg-mime 写 `.desktop`，是唯一可行途径（AppImage 尤其需要）
/// - **Windows 便携版**：没有安装器可用，只能由程序自己登记
/// - **Windows 安装版 / macOS**：交给安装包与系统，程序不插手
pub fn should_register_scheme() -> bool {
    if cfg!(target_os = "linux") {
        return true;
    }
    #[cfg(windows)]
    if crate::datadir::install_mode() == crate::datadir::InstallMode::Portable {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 把一段 URL 拆成 `(action, params)`，与 `handle_deep_link` 内部逻辑一致。
    ///
    /// 抽出来是为了能脱离 `AppHandle` 直接测——这条解析曾经是**静默错的**
    /// （`mclink://join/CODE` 解析出 `action="CODE"`、`params=[]`），
    /// 而它只在用户点链接时才暴露，必须由测试钉住。
    fn parse(url: &str) -> Option<(String, Vec<String>)> {
        let parsed = tauri::Url::parse(url).ok()?;
        if parsed.scheme() != "mclink" {
            return None;
        }
        let mut segments: Vec<String> = Vec::new();
        if let Some(host) = parsed.host_str() {
            if !host.is_empty() {
                segments.push(host.to_string());
            }
        }
        for part in parsed.path().split('/') {
            if !part.is_empty() {
                segments.push(part.to_string());
            }
        }
        if segments.is_empty() {
            return None;
        }
        let decoded: Vec<String> = segments
            .iter()
            .map(|s| percent_decode(s).unwrap_or_else(|| s.clone()))
            .collect();
        let action = decoded[0].clone();
        let params = decoded[1..].to_vec();
        if params.is_empty() {
            return None;
        }
        Some((action, params))
    }

    /// 回归：`join` 落在 **host** 上，不能被漏掉。
    ///
    /// 这是整条深链功能的核心用例——`mclink://join/CODE` 必须解析成
    /// `action="join"`、`params=["CODE"]`，否则 `ConnectView` 收不到邀请码。
    #[test]
    fn host_is_part_of_the_action() {
        assert_eq!(
            parse("mclink://join/CODE"),
            Some(("join".to_string(), vec!["CODE".to_string()]))
        );
    }

    /// 三段斜杠的写法（`mclink:///join/CODE`）等价，兼容手写链接。
    #[test]
    fn triple_slash_form_is_equivalent() {
        assert_eq!(parse("mclink:///join/CODE"), parse("mclink://join/CODE"));
    }

    /// 多段参数（`mclink://plugin/<id>/download`）。
    #[test]
    fn multiple_params_are_kept_in_order() {
        assert_eq!(
            parse("mclink://plugin/abc123/download"),
            Some((
                "plugin".to_string(),
                vec!["abc123".to_string(), "download".to_string()]
            ))
        );
    }

    /// 邀请码里的 `/` 会被前端 `encodeURIComponent` 成 `%2F`，这里必须解回来。
    #[test]
    fn encoded_slash_in_invite_code_is_decoded() {
        let (action, params) = parse("mclink://join/U%2FA90T-7XHQ-9T98-ECQV").unwrap();
        assert_eq!(action, "join");
        assert_eq!(params[0], "U/A90T-7XHQ-9T98-ECQV");
    }

    /// 非 mclink scheme 必须忽略。
    #[test]
    fn other_schemes_are_rejected() {
        assert!(parse("https://example.com/join/CODE").is_none());
    }

    /// 只有 action、没有参数 → 视为无效（前端拿不到东西可做）。
    #[test]
    fn action_without_params_is_invalid() {
        assert!(parse("mclink://join").is_none());
    }

    /// 百分号解码不能 panic：`%` 后跟多字节字符或截断序列都要安全返回。
    #[test]
    fn percent_decode_never_panics() {
        assert_eq!(percent_decode("abc"), Some("abc".to_string()));
        assert_eq!(percent_decode("%41"), Some("A".to_string()));
        // 截断 / 非法十六进制 → None（调用方回退原文），绝不 panic
        assert_eq!(percent_decode("%4"), None);
        assert_eq!(percent_decode("%zz"), None);
        // 多字节字符紧跟 `%`：按字节处理，不切片 str
        assert_eq!(percent_decode("%中"), None);
    }
}
