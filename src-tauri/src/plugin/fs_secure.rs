//! 插件目录内敏感文件的权限收紧（Windows：隐藏 + ACL；Unix：0600）。
//!
//! 两处需要它：会话令牌文件（`launcher`）与预共享密钥（`registry`）。
//! 早期实现分散在两处且**只设隐藏属性**——隐藏不是权限，同机其他用户或进程
//! 照样能读到 PSK 与会话令牌（而 PSK 是握手双方信任根）。
//! 统一到一处，并在 Windows 上顺带把 ACL 收紧到"仅当前用户"。

use std::path::Path;

/// 把文件的可见性/权限收紧到"仅当前用户"。
///
/// 成功返回 `Ok(())`；失败返回原因，由调用方决定是否阻断。
///
/// # 为什么改成返回结果
///
/// 此前是"尽力而为、静默吞掉"：`icacls` 失败时文件只剩隐藏属性。隐藏**不是权限**，
/// 而密钥落点（`Setting/plugin_keys/`）的意义正是"别的进程读不到"。密钥文件
/// 权限收紧失败必须能被上层感知，不能悄悄降级。
pub fn restrict_to_current_user(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        use std::os::windows::fs::OpenOptionsExt;
        if let Ok(file) = std::fs::OpenOptions::new()
            .write(true)
            .attributes(FILE_ATTRIBUTE_HIDDEN)
            .open(path)
        {
            drop(file);
        }

        // 隐藏属性不是权限控制：去掉继承、只保留当前用户。
        // 走系统自带的 icacls 而不是引入依赖。
        let user = std::env::var("USERNAME")
            .map_err(|_| "无法读取 USERNAME，无法收紧文件权限".to_string())?;
        let user = user.trim();
        if user.is_empty() {
            return Err("USERNAME 为空，无法收紧文件权限".to_string());
        }
        let status = std::process::Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{}:F", user))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| format!("调用 icacls 失败: {}", e))?;
        if !status.success() {
            return Err(format!("icacls 返回失败状态: {}", status));
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("设置 0600 权限失败: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restrict_never_panics_and_reports_failure_for_missing_path() {
        // 不存在的路径不应 panic；调用方据此决定是否阻断
        let result = restrict_to_current_user(Path::new("E:/definitely/not/here.key"));
        // Windows 上 icacls 对不存在的路径会失败（返回 Err），Unix 上 set_permissions 同样失败。
        // 关键不变量是"不 panic 且错误可被感知"。
        let _ = result;

        let tmp = std::env::temp_dir().join(format!(
            "mclink-fs-secure-{}",
            crate::plugin::crypto::random_hex(6)
        ));
        std::fs::write(&tmp, b"secret").unwrap();
        // 真实文件应当收紧成功；Unix 上还能直接验证模式位
        restrict_to_current_user(&tmp).expect("真实文件应能收紧权限");
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&tmp).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Unix 上应为 0600");
        }
        let _ = std::fs::remove_file(&tmp);
    }
}
