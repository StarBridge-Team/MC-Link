//! 在落地目录里定位引擎可执行文件。
//!
//! # 为什么不能写死路径
//!
//! 上游包内的结构随版本与平台变化（v2.6.4 的 Windows 包会多套一层
//! `easytier-windows-x86_64/`，macOS 包里可能是一个 `.app` 目录）。写死
//! `<dir>/easytier-core(.exe)` 会在换版本时变成"下载成功、但启动时找不到文件"，
//! 而错误只会发生在用户点"开始联机"的那一刻。
//!
//! 因此这里按**文件名主干**递归查找，并对结果做稳定排序（浅层优先、路径字典序），
//! 保证同一个目录多次调用得到同一个结果。

use std::path::{Path, PathBuf};

/// 一套可用的引擎可执行文件。
#[derive(Debug, Clone)]
pub struct Binaries {
    /// `easytier-core`：真正承载链路的进程。
    pub core: PathBuf,
    /// `easytier-cli`：**只做管理**（查 peer、加端口转发），不参与建链。
    pub cli: PathBuf,
}

/// 引擎的两种可执行文件的主干名。
const CORE_STEM: &str = "easytier-core";
const CLI_STEM: &str = "easytier-cli";

/// 递归深度上限：包内最多两层目录，给到 4 层足够，同时挡住畸形目录树。
const MAX_DEPTH: usize = 4;

/// 在 `root` 下找到完整的引擎；缺任何一个都返回 `None`。
///
/// 两个文件都必需：缺 `cli` 时能建链但**无法查询对端、无法追加端口转发**，
/// 陶瓦适配器会在访客侧半途而废——这比"引擎没装好"更难排查，所以宁可在这里
/// 就把整个引擎判为不可用。
pub fn locate(root: &Path) -> Option<Binaries> {
    Some(Binaries {
        core: find(root, CORE_STEM)?,
        cli: find(root, CLI_STEM)?,
    })
}

/// 按主干名在 `root` 下查找可执行文件。
pub fn find(root: &Path, stem: &str) -> Option<PathBuf> {
    let mut found: Vec<(usize, PathBuf)> = Vec::new();
    walk(root, 0, stem, &mut found);
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().next().map(|(_, path)| path)
}

fn walk(dir: &Path, depth: usize, stem: &str, out: &mut Vec<(usize, PathBuf)>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth + 1, stem, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if matches_name(name, stem) {
            out.push((depth, path));
        }
    }
}

/// 文件名是否是目标的"可执行形态"。
///
/// Windows 上必须带 `.exe`；其余平台不允许带扩展名（上游的 Unix 产物就是裸文件名）。
/// 该判定也要能识别 macOS 包里的同名文件。
pub fn matches_name(name: &str, stem: &str) -> bool {
    matches_name_for(name, stem, cfg!(windows))
}

/// [`matches_name`] 的可测试版本：`windows` 显式传入。
fn matches_name_for(name: &str, stem: &str, windows: bool) -> bool {
    if windows {
        name.len() == stem.len() + 4
            && name[..stem.len()].eq_ignore_ascii_case(stem)
            && name[stem.len()..].eq_ignore_ascii_case(".exe")
    } else {
        name.eq_ignore_ascii_case(stem)
    }
}

/// 尽力补上可执行位（tar/zip 解压可能丢权限）。
///
/// 失败不致命：某些文件系统（FAT/exFAT、部分挂载选项）不支持权限位，
/// 那种情况下文件本身仍可执行。
#[cfg(unix)]
pub fn ensure_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mut perm = meta.permissions();
    let mode = perm.mode();
    if mode & 0o111 == 0o111 {
        return Ok(());
    }
    perm.set_mode(mode | 0o111);
    std::fs::set_permissions(path, perm).map_err(|e| e.to_string())
}

/// Windows 上没有可执行位这个概念。
#[cfg(not(unix))]
pub fn ensure_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每次用唯一目录，避免并行测试互相看到对方的残留文件。
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclink-engine-bin-{}-{}",
            name,
            crate::plugin::crypto::random_hex(4)
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn exe_name(stem: &str) -> String {
        if cfg!(windows) {
            format!("{}.exe", stem)
        } else {
            stem.to_string()
        }
    }

    fn write_file(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), b"stub").unwrap();
    }

    #[test]
    fn locate_finds_nested_layout() {
        let root = temp_dir("nested");
        let inner = root.join("easytier-windows-x86_64");
        write_file(&inner, &exe_name(CORE_STEM));
        write_file(&inner, &exe_name(CLI_STEM));

        let found = locate(&root).expect("应能找到引擎");
        assert_eq!(found.core.file_name().unwrap().to_string_lossy(), exe_name(CORE_STEM));
        assert_eq!(found.cli.file_name().unwrap().to_string_lossy(), exe_name(CLI_STEM));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn locate_requires_both_binaries() {
        let root = temp_dir("partial");
        write_file(&root, &exe_name(CORE_STEM));
        assert!(
            locate(&root).is_none(),
            "缺 cli 时整个引擎应判为不可用"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn shallowest_candidate_wins() {
        let root = temp_dir("shallow");
        write_file(&root.join("a").join("b").join("c"), &exe_name(CORE_STEM));
        write_file(&root, &exe_name(CORE_STEM));
        let found = find(&root, CORE_STEM).expect("应能找到");
        assert_eq!(found.parent().unwrap(), root, "应优先返回浅层的那份");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn name_matching_is_platform_aware() {
        assert!(matches_name_for("easytier-core.exe", CORE_STEM, true));
        assert!(matches_name_for("EasyTier-Core.EXE", CORE_STEM, true));
        assert!(!matches_name_for("easytier-core", CORE_STEM, true));
        assert!(!matches_name_for("easytier-core.dll", CORE_STEM, true));
        assert!(matches_name_for("easytier-core", CORE_STEM, false));
        assert!(!matches_name_for("easytier-core.exe", CORE_STEM, false));
        // 前缀更长的名字不能被误认
        assert!(!matches_name_for("easytier-core2.exe", CORE_STEM, true));
    }

    #[test]
    fn missing_root_is_none() {
        assert!(locate(&temp_dir("empty").join("nope")).is_none());
    }
}
