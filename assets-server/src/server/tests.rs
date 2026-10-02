//! 单元测试。
//!
//! 拆模块后这里不再能用 `use super::*` 一把捞：被测对象分散在子模块里，
//! 显式列出来反而让"每个用例在测哪一块"更清楚。

use std::path::{Path, PathBuf};

use super::files::parse_range;
use super::files::RangeSpec::{Ignore, Partial, Unsatisfiable};
use super::paths::{resolve_within, safe_join, sanitize_path};
use super::query::percent_decode;
use super::upload::{constant_time_eq, redact_token};

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
    assert_eq!(redact_token("/upload?path=a&x=1"), "/upload?path=a&x=1");
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
