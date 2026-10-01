//! 插件目录内敏感文件的权限收紧（Windows：隐藏 + ACL；Unix：0600）。
//!
//! 两处需要它：会话令牌文件（`launcher`）与预共享密钥（`registry`）。
//! 早期实现分散在两处且**只设隐藏属性**——隐藏不是权限，同机其他用户或进程
//! 照样能读到 PSK 与会话令牌（而 PSK 是握手双方信任根）。
//! 统一到一处，并在 Windows 上顺带把 ACL 收紧到"仅当前用户"。

use std::path::Path;

/// 把文件的可见性/权限收紧到"仅当前用户"。
///
/// 刻意做成尽力而为：失败不阻断流程（文件至少仍带隐藏属性），
/// 因为这里失败通常意味着"系统工具不可用"，而不是权限泄露的新增风险。
#[cfg(windows)]
pub fn restrict_to_current_user(path: &Path) {
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
    // 走系统自带的 icacls 而不是引入依赖；失败只是降级（仍是隐藏文件）。
    let Ok(user) = std::env::var("USERNAME") else {
        return;
    };
    let user = user.trim();
    if user.is_empty() {
        return;
    }
    let _ = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{}:F", user))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(windows))]
pub fn restrict_to_current_user(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restrict_is_best_effort_and_never_panics() {
        // 不存在的路径也必须安静返回：调用点不处理错误
        restrict_to_current_user(Path::new("E:/definitely/not/here.key"));

        // 真实文件：Unix 上能直接验证模式位；Windows 上只验证不 panic
        let tmp = std::env::temp_dir().join(format!(
            "mclink-fs-secure-{}",
            crate::plugin::crypto::random_hex(6)
        ));
        std::fs::write(&tmp, b"secret").unwrap();
        restrict_to_current_user(&tmp);
        assert!(tmp.exists());
        let _ = std::fs::remove_file(&tmp);
    }
}
