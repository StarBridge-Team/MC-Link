//! 插件数据目录：每个插件一份**可写**目录。
//!
//! # 为什么必须独立于插件目录
//!
//! 插件目录是被验签的：`integrity.txt` 覆盖了目录内每个文件，核心**每次扫描都复核**
//! （见 [`crate::plugin::trust::verify_directory`]）。插件把运行期状态写进自己的目录，
//! 下一次扫描就会因为"出现清单外文件 / 哈希不符"被整体忽略——表现为"插件昨天还能用，
//! 今天整个消失了"，而且没有任何错误提示。
//!
//! 因此可写状态一律放这里：
//!
//! ```text
//! <data_dir>/PluginData/<plugin_id>/
//! ```
//!
//! # 边界
//!
//! 核心只负责**创建目录并把路径告诉插件**（会话文件的 `data_dir` 字段），
//! 不代它读写、不清理。原因是这层"谁的数据谁负责"能避免核心在卸载/更新时误删
//! 插件还需要的状态；清理策略由插件自己在卸载流程里决定（后续安装器会带一条
//! "是否保留数据"的询问）。

use std::path::{Path, PathBuf};

use crate::plugin::manifest::validate_plugin_id;

/// 数据目录的顶层名。
pub const DIR: &str = "PluginData";

/// 解析（不创建）某个插件的数据目录。
///
/// 插件 ID 先过 [`validate_plugin_id`]：它只允许小写字母/数字/`.`/`-`/`_` 且不含
/// `..`，因此拼出来的路径不可能逃出 `PluginData/`。
pub fn plugin_data_dir(data_dir: &Path, plugin_id: &str) -> Result<PathBuf, String> {
    validate_plugin_id(plugin_id)?;
    Ok(data_dir.join(DIR).join(plugin_id))
}

/// 确保数据目录存在，返回其路径。
pub fn ensure(data_dir: &Path, plugin_id: &str) -> Result<PathBuf, String> {
    let dir = plugin_data_dir(data_dir, plugin_id)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("创建插件数据目录失败 {}: {}", dir.display(), e))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "mclink-pdata-{}",
            crate::plugin::crypto::random_hex(6)
        ))
    }

    #[test]
    fn ensure_creates_a_per_plugin_directory() {
        let root = temp_dir();
        let dir = ensure(&root, "dev.example.tool").unwrap();
        assert!(dir.is_dir());
        assert_eq!(dir, root.join("PluginData").join("dev.example.tool"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_ids_that_could_escape() {
        let root = temp_dir();
        for bad in ["../evil", "no-dot", "Upper.Case", "", "a..b"] {
            assert!(
                plugin_data_dir(&root, bad).is_err(),
                "{bad:?} 不应产生路径"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
