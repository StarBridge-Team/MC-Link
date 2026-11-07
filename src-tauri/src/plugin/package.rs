//! 插件包（`.mclink-plugin`，本质是 tar.gz）的解包与**交付形态校验**。
//!
//! # 为什么解包与校验必须在一起
//!
//! 验签只覆盖"文件字节"，不覆盖"这些文件能不能被安全执行"。一个签名有效、但入口
//! 指向目录外的包，仍然不该装。因此这里把两件事绑在一起：
//!
//! 1. **解包前的探针**：统计展开体积与条目数，拒绝解压炸弹与符号链接；
//! 2. **解包后的形态校验**：入口存在且不越界、没有 `node_modules`、总体积不超预算。
//!
//! 市场安装与本地包安装共用这一份，避免"网络装的走一套校验、本地拖的走另一套"。

use std::path::{Path, PathBuf};

use crate::plugin::manifest::PluginManifest;

/// 解压总量上限（防解压炸弹；与市场安装同口径）。
pub const MAX_EXTRACT_BYTES: u64 = 512 * 1024 * 1024;
/// 解压条目数上限。
pub const MAX_EXTRACT_ENTRIES: usize = 4096;

/// 单个插件交付物的体积上限（含随包资源，不含共享运行时）。
///
/// 与 `插件开发规范.md` 的 C2 一致：10 MB。超限的多数情况是**把大资源打进包里**——
/// 正确做法是首次运行时下载到 `PluginData/`，包内只留校验和。
pub const MAX_PLUGIN_BYTES: u64 = 10 * 1024 * 1024;

/// 插件包的体积上限（压缩后）。比展开上限小得多：压缩包不该比它的内容还夸张。
pub const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

/// 解压到 `target`，带展开体积、条目数与条目类型校验。
pub fn extract_tar_gz(archive_path: &Path, target: &Path) -> Result<(), String> {
    // 先探一遍：只读，不落盘。
    {
        let file = std::fs::File::open(archive_path).map_err(|e| format!("打开插件包失败: {e}"))?;
        let mut probe = tar::Archive::new(flate2::read::GzDecoder::new(file));
        let mut total: u64 = 0;
        let mut count: usize = 0;
        for entry in probe.entries().map_err(|e| format!("读取插件包失败: {e}"))? {
            let entry = entry.map_err(|e| format!("读取插件包条目失败: {e}"))?;
            // 只允许普通文件与目录：tar 里的符号链接/硬链接可以指向包外的任意路径，
            // 解包后再被核心执行就等于绕过了 `is_within`。
            let kind = entry.header().entry_type();
            if !(kind.is_file() || kind.is_dir()) {
                return Err(format!(
                    "插件包含不允许的条目类型（{:?}），已中止安装",
                    kind
                ));
            }
            total = total.saturating_add(entry.header().size().unwrap_or(0));
            count += 1;
            if total > MAX_EXTRACT_BYTES || count > MAX_EXTRACT_ENTRIES {
                return Err(format!(
                    "插件包展开后过大或条目过多（累计 {total} 字节 / {count} 项），已中止安装"
                ));
            }
        }
    }

    std::fs::create_dir_all(target).map_err(|e| format!("创建插件目录失败: {e}"))?;
    let file = std::fs::File::open(archive_path).map_err(|e| format!("打开插件包失败: {e}"))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    // tar crate 的 `unpack` 会拒绝绝对路径与 `..`（路径穿越）。
    archive
        .unpack(target)
        .map_err(|e| format!("解压插件包失败: {e}"))?;
    Ok(())
}

/// 校验解出来的目录"能不能被安全加载"，返回总体积（字节）。
///
/// 这些规则都不在 `PluginManifest::validate` 里——那里只校验清单**内容**，
/// 不碰磁盘（清单可以来自网络）。形态校验必须看真实文件。
pub fn validate_layout(dir: &Path, manifest: &PluginManifest) -> Result<u64, String> {
    let entry_path: PathBuf = manifest
        .resolve_entry(dir)
        .ok_or_else(|| "清单未声明入口文件".to_string())?;
    // 先判存在再判越界：`is_within` 对不存在的路径会 canonicalize 失败，
    // 顺序反了会把"文件缺失"误报成"路径越界"，作者会去查错地方。
    if !entry_path.is_file() {
        return Err(format!("入口文件不存在: {}", entry_path.display()));
    }
    if !is_within(dir, &entry_path) {
        return Err(format!("入口越出插件目录: {}", entry_path.display()));
    }

    let mut total: u64 = 0;
    let mut files: usize = 0;
    walk(dir, &mut |path| {
        // `node_modules` 是唯一现实中的超限途径，也是供应链失控的入口
        if path
            .components()
            .any(|c| c.as_os_str() == std::ffi::OsStr::new("node_modules"))
        {
            return Err("插件包含 node_modules（请用 bun build 打成单文件）".to_string());
        }
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                return Err(format!("插件包含符号链接: {}", path.display()));
            }
            if meta.is_file() {
                total = total.saturating_add(meta.len());
                files += 1;
            }
        }
        Ok(())
    })?;

    if files == 0 {
        return Err("插件包是空的".to_string());
    }
    if total > MAX_PLUGIN_BYTES {
        return Err(format!(
            "插件体积 {:.1} MB 超过上限 {:.0} MB（大资源请在运行时下载到 PluginData/）",
            total as f64 / 1024.0 / 1024.0,
            MAX_PLUGIN_BYTES as f64 / 1024.0 / 1024.0
        ));
    }
    Ok(total)
}

/// 递归遍历目录（不跟随符号链接）。
fn walk(dir: &Path, visit: &mut impl FnMut(&Path) -> Result<(), String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("读取 {} 失败: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("读取元数据失败: {e}"))?;
        visit(&path)?;
        if meta.is_dir() {
            walk(&path, visit)?;
        }
    }
    Ok(())
}

/// `child` 是否位于 `parent` 之内（与 `launcher::is_within` 同口径）。
fn is_within(parent: &Path, child: &Path) -> bool {
    let Ok(parent) = parent.canonicalize() else {
        return false;
    };
    let Ok(child) = child.canonicalize() else {
        return false;
    };
    child.starts_with(&parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "mclink-pkg-{}",
            crate::plugin::crypto::random_hex(6)
        ))
    }

    fn write(dir: &Path, rel: &str, bytes: &[u8]) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn manifest(dir: &Path) -> PluginManifest {
        write(
            dir,
            "plugin.json",
            br#"{"id":"dev.example.tool","name":"T","version":"1.0.0","kind":"utility",
                 "permissions":["fs_plugin_data"],
                 "runtime":{"kind":"process","entry":"bin/tool.exe"}}"#,
        );
        PluginManifest::load(dir).unwrap()
    }

    #[test]
    fn accepts_a_well_formed_layout() {
        let dir = temp_dir();
        let manifest = manifest(&dir);
        write(&dir, "bin/tool.exe", b"fake-binary");
        let size = validate_layout(&dir, &manifest).unwrap();
        assert!(size > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `node_modules` 是 JS 侧唯一现实的超限途径，必须在安装期挡住。
    #[test]
    fn rejects_node_modules() {
        let dir = temp_dir();
        let manifest = manifest(&dir);
        write(&dir, "bin/tool.exe", b"x");
        write(&dir, "node_modules/left-pad/index.js", b"module.exports=1");
        let err = validate_layout(&dir, &manifest).unwrap_err();
        assert!(err.contains("node_modules"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_entry() {
        let dir = temp_dir();
        let manifest = manifest(&dir);
        let err = validate_layout(&dir, &manifest).unwrap_err();
        assert!(err.contains("入口文件不存在"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 体积预算：超过 10 MB 必须在安装期拒绝，而不是等到运行时才发现。
    #[test]
    fn rejects_oversized_packages() {
        let dir = temp_dir();
        let manifest = manifest(&dir);
        write(&dir, "bin/tool.exe", b"x");
        // 稀疏文件：只声明大小，不真的写 11 MB 数据
        let big = dir.join("assets/big.bin");
        std::fs::create_dir_all(big.parent().unwrap()).unwrap();
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_PLUGIN_BYTES + 1024).unwrap();
        drop(file);
        let err = validate_layout(&dir, &manifest).unwrap_err();
        assert!(err.contains("超过上限"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
