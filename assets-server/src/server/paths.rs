//! 路径安全检查。
//!
//! 这是**整个服务最需要被单独审视的一块**：它决定了"某个 URL 能读到/写到磁盘上的
//! 哪个文件"。混在 HTTP 管道代码中间时，评审很容易只看个大概就放过。
//!
//! 两层防线，缺一不可：
//! 1. [`sanitize`] / [`sanitize_path`] 做粗筛（丢掉分隔符、盘符、`..` 段）；
//! 2. [`resolve_within`] / [`safe_join`] 做 `canonicalize` + 包含性校验——
//!    字符串前缀比较挡不住"目录内放一个指向外面的符号链接"。

use std::path::{Path, PathBuf};

/// 清理路径，仅保留安全字符（用于单段名字，如 `section`、`legal` 文件名）。
pub(super) fn sanitize(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

/// 防止路径穿越：把相对路径规整为安全的分段序列。
///
/// 必须同时处理三类逃逸（历史实现只按 `/` 切分，注释却写着"已处理反斜杠"）：
/// - Windows 上 `\` 也是分隔符：`..\..\Windows\win.ini` 不会被 `/` 切开；
/// - 盘符/前缀：`C:\Windows\win.ini` 经 `PathBuf::push` 会**整体替换**已有路径；
/// - 因此还叠一层 [`resolve_within`] 做 canonicalize + 包含性校验，本函数只做粗筛。
pub(super) fn sanitize_path(input: &str) -> PathBuf {
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
pub(super) fn resolve_within(base: &Path, rel: &Path) -> Option<PathBuf> {
    let real_base = base.canonicalize().ok()?;
    let real = real_base.join(rel).canonicalize().ok()?;
    if real.starts_with(&real_base) {
        Some(real)
    } else {
        None
    }
}

/// 防止路径穿越：仅允许在 base 目录内写入，拒绝 `..`、绝对路径、反斜杠。
///
/// 与 [`resolve_within`] 的分工：写入时目标文件**还不存在**，`canonicalize` 会失败，
/// 所以这里退回"规整 + 前缀比较"（并已先拒掉 `..` 与绝对路径）。
pub(super) fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
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
