//! 陶瓦可执行文件的发现。
//!
//! 单独成文件的原因：这一组函数只关心"落地目录里哪个文件才是陶瓦 exe"，
//! 与端口探测、状态落盘、生命周期都无关。拆开之后 [`crate::adapter`]
//! 只剩"进程与端口的管理"，可执行文件的判定规则可以单独演进。

use std::path::{Path, PathBuf};

/// 在落地目录里找出陶瓦可执行文件。
///
/// 上游包内的可执行文件名**随平台与版本变化**（Windows 形如
/// `terracotta-0.4.2-windows-x86_64.exe`，Unix 形如
/// `terracotta-0.4.2-linux-x86_64`），因此不能写死文件名，只能在这里发现。
///
/// 判定规则：
/// - 名称（忽略大小写）以 `terracotta` 开头；
/// - Windows：扩展名必须是 `exe`；Unix：没有扩展名；
/// - Unix 上还要有可执行位（`PermissionsExt`）；
/// - 名称里**含当前平台标签**的优先，没有就取第一个候选。
///
/// 找不到返回 `None`。
pub(super) fn find_terracotta_exe(dir: &Path, platform: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let platform_lower = platform.to_ascii_lowercase();

    let mut best: Option<(u8, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().starts_with("terracotta") {
            continue;
        }
        if !has_executable_name(name) {
            continue;
        }
        // 评分：平台标签命中 +2，有可执行位 +1。得分相同按名称排序，保证结果稳定。
        let mut score = 0u8;
        if !platform_lower.is_empty() && name.to_ascii_lowercase().contains(&platform_lower) {
            score += 2;
        }
        if is_executable_file(&path) {
            score += 1;
        }
        let replace = match &best {
            Some((best_score, best_path)) => {
                score > *best_score
                    || (score == *best_score
                        && path.file_name() < best_path.file_name())
            }
            None => true,
        };
        if replace {
            best = Some((score, path));
        }
    }
    best.map(|(_, path)| path)
}

/// 名称是否像"可执行文件"：Windows 必须是 `.exe`，Unix 不能有扩展名。
pub(super) fn has_executable_name(name: &str) -> bool {
    has_executable_name_for(name, cfg!(windows))
}

/// [`has_executable_name`] 的可测试版本：`windows` 显式传入。
fn has_executable_name_for(name: &str, windows: bool) -> bool {
    let ext = alpha_extension(name);
    if windows {
        ext.map(|e| e.eq_ignore_ascii_case("exe")).unwrap_or(false)
    } else {
        ext.is_none()
    }
}

/// 取"纯字母型"扩展名；不是扩展名（如版本号片段）返回 `None`。
fn alpha_extension(name: &str) -> Option<&str> {
    let (_, ext) = name.rsplit_once('.')?;
    if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(ext)
}

/// 文件是否带可执行位。Windows 上没有这个概念，一律视为可执行。
#[cfg(unix)]
pub(super) fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub(super) fn is_executable_file(_path: &Path) -> bool {
    true
}

/// 尽力给文件补上可执行位（tar 解压可能丢权限位）。
#[cfg(unix)]
pub(super) fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mut perm = meta.permissions();
    let mode = perm.mode();
    perm.set_mode(mode | 0o111);
    std::fs::set_permissions(path, perm).map_err(|e| e.to_string())
}

/// 列出目录下的文件名，用于"没找到可执行文件"时的日志。
pub(super) fn describe_dir(dir: &Path) -> String {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            if names.is_empty() {
                "（空目录）".to_string()
            } else {
                names.join(", ")
            }
        }
        Err(e) => format!("（无法读取目录: {}）", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每次调用都用唯一目录，避免并行测试互相看到对方的残留文件。
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mc-link-adapter-exe-{}-{}",
            name,
            crate::plugin::crypto::random_hex(4)
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// 按当前平台的命名规则造一个候选可执行文件。
    fn write_candidate(dir: &Path, base: &str) -> PathBuf {
        let name = if cfg!(windows) {
            format!("{}.exe", base)
        } else {
            base.to_string()
        };
        let path = dir.join(name);
        std::fs::write(&path, b"stub").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&path).unwrap().permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&path, perm).unwrap();
        }
        path
    }

    #[test]
    fn no_candidate_returns_none() {
        let dir = temp_dir("none");
        std::fs::write(dir.join("readme.txt"), b"x").unwrap();
        assert!(
            find_terracotta_exe(&dir, "windows-x86_64").is_none(),
            "目录里没有陶瓦可执行文件时应返回 None"
        );
    }

    #[test]
    fn candidate_matching_platform_tag_wins() {
        let dir = temp_dir("platform");
        write_candidate(&dir, "terracotta-0.4.2-linux-x86_64");
        let preferred = write_candidate(&dir, "terracotta-0.4.2-windows-x86_64");

        let found = find_terracotta_exe(&dir, "windows-x86_64")
            .expect("应能找到候选可执行文件");
        assert_eq!(
            found.file_name(),
            preferred.file_name(),
            "应优先选文件名含当前平台标签的那个"
        );
    }

    #[test]
    fn extra_terracotta_named_files_are_not_picked() {
        // 上游 macOS 包里同时有 `terracotta-0.4.2-macos-arm64` 与
        // `terracotta-0.4.2-macos-arm64.pkg`（后者是安装包，不是可执行文件）
        let dir = temp_dir("extra");
        let exe = write_candidate(&dir, "terracotta-0.4.2-macos-arm64");
        std::fs::write(dir.join("terracotta-0.4.2-macos-arm64.pkg"), b"stub").unwrap();

        let found = find_terracotta_exe(&dir, "macos-arm64")
            .expect("应能找到候选可执行文件");
        assert_eq!(
            found.file_name(),
            exe.file_name(),
            "应选中真正的可执行文件，而不是同名的 .pkg"
        );
    }

    #[test]
    fn versioned_name_without_extension_is_accepted() {
        // 上游文件名带版本号（0.4.2），Unix 上不能把它误判成"有扩展名"
        assert!(has_executable_name_for(
            "terracotta-0.4.2-linux-x86_64",
            false
        ));
        assert!(!has_executable_name_for(
            "terracotta-0.4.2-linux-x86_64.exe",
            false
        ));
        // Windows 上必须有 .exe
        assert!(has_executable_name_for(
            "terracotta-0.4.2-windows-x86_64.exe",
            true
        ));
        assert!(!has_executable_name_for(
            "terracotta-0.4.2-windows-x86_64",
            true
        ));
    }

    #[cfg(unix)]
    #[test]
    fn file_without_executable_bit_is_still_discovered() {
        // tar 解压可能丢权限位：文件仍需能被找到，随后由 make_executable 补权限
        let dir = temp_dir("noexec");
        let path = dir.join("terracotta-0.4.2-linux-x86_64");
        std::fs::write(&path, b"stub").unwrap();
        assert!(find_terracotta_exe(&dir, "linux-x86_64").is_some());
    }
}
