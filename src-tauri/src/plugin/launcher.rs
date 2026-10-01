//! 外部插件进程的启动与会话文件分发。
//!
//! # 为什么不把令牌放在命令行里
//!
//! 进程命令行在同一台机器上是可以被读到的（`wmic process get commandline`、
//! `/proc/<pid>/cmdline`、部分任务管理器视图）。把一次性启动令牌放进 argv
//! 等于把它公开。因此这里把令牌写进**权限受限的会话文件**，命令行只传文件路径。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::plugin::protocol::PROTOCOL_VERSION;
use crate::plugin::registry::{PluginRecord, PluginSource};

/// 会话文件内容：外部插件启动后读取它来知道该连哪里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescriptor {
    /// 协议版本。
    pub protocol: u16,
    /// 插件 ID。
    pub plugin_id: String,
    /// 网关地址（不含令牌与插件参数）。
    pub endpoint: String,
    /// 一次性启动令牌。
    pub token: String,
    /// 核心期望的心跳间隔（毫秒）。
    pub heartbeat_ms: u64,
}

/// 会话文件路径。
pub fn session_file(runtime_dir: &Path, plugin_id: &str) -> PathBuf {
    runtime_dir.join(format!("{}.session.json", plugin_id))
}

/// 写入会话文件并返回路径。
pub fn write_session_file(
    runtime_dir: &Path,
    plugin_id: &str,
    endpoint: &str,
    token: &str,
    heartbeat_ms: u64,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(runtime_dir)
        .map_err(|e| format!("创建插件运行时目录失败: {}", e))?;
    let path = session_file(runtime_dir, plugin_id);
    let desc = SessionDescriptor {
        protocol: PROTOCOL_VERSION,
        plugin_id: plugin_id.to_string(),
        endpoint: endpoint.to_string(),
        token: token.to_string(),
        heartbeat_ms,
    };
    let text = serde_json::to_string_pretty(&desc)
        .map_err(|e| format!("序列化会话描述失败: {}", e))?;
    std::fs::write(&path, text).map_err(|e| format!("写入会话文件失败: {}", e))?;
    // 尽力而为地收紧权限；Windows 上退化为隐藏属性。
    restrict(&path);
    Ok(path)
}

/// 启动外部插件进程，返回进程 ID 与会话文件路径。
pub fn spawn(
    record: &PluginRecord,
    endpoint: &str,
    token: &str,
    runtime_dir: &Path,
) -> Result<(std::process::Child, PathBuf), String> {
    if record.source != PluginSource::External {
        return Err(format!("{} 不是外部插件，无需启动进程", record.id()));
    }

    let manifest = &record.manifest;
    let exe = manifest
        .resolve_entry(&record.dir)
        .ok_or_else(|| format!("插件 {} 未声明可执行文件", record.id()))?;

    // 纵深防御：即使清单校验被绕过，也不允许启动插件目录之外的文件。
    if !is_within(&record.dir, &exe) {
        return Err(format!(
            "拒绝启动插件目录之外的可执行文件: {}",
            exe.display()
        ));
    }
    if !exe.is_file() {
        return Err(format!("插件可执行文件不存在: {}", exe.display()));
    }

    let workdir = manifest.resolve_workdir(&record.dir);
    if !is_within(&record.dir, &workdir) {
        return Err(format!("拒绝在插件目录之外设置工作目录: {}", workdir.display()));
    }

    let session = write_session_file(
        runtime_dir,
        record.id(),
        endpoint,
        token,
        manifest.limits.heartbeat_ms,
    )?;

    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(&workdir);

    for arg in &manifest.runtime.args {
        cmd.arg(substitute(arg, endpoint, &session));
    }
    // 若清单没有显式声明会话参数，默认追加一条，保证插件能拿到连接信息。
    if !manifest
        .runtime
        .args
        .iter()
        .any(|a| a.contains("{token_file}"))
    {
        cmd.arg("--mclink-session").arg(&session);
    }

    // 只透传清单声明的环境变量，避免插件继承核心进程的全部环境（可能含敏感信息）。
    cmd.env_clear();
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    for key in ["PATH", "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "LANG", "HOME"] {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.to_string(), value);
        }
    }
    for (k, v) in &manifest.runtime.env {
        env.insert(k.clone(), v.clone());
    }
    cmd.envs(env);

    let child = cmd
        .spawn()
        .map_err(|e| format!("启动插件进程失败 {}: {}", exe.display(), e))?;
    // 把 `Child` 交回调用方保管，而不是只给一个 pid：句柄被丢弃后
    // 既无法 `wait` 回收（Unix 上留下僵尸进程），也无法在超时/退出时 `kill`
    // （Windows 上应用退出后插件进程会继续残留）。见 manager 的 `children` 表。
    Ok((child, session))
}

fn substitute(template: &str, endpoint: &str, session_file: &Path) -> String {
    template
        .replace("{endpoint}", endpoint)
        .replace("{token_file}", &session_file.to_string_lossy())
}

/// `child` 是否位于 `parent` 之内（用于阻止路径逃逸）。
fn is_within(parent: &Path, child: &Path) -> bool {
    let Ok(parent) = parent.canonicalize() else {
        return false;
    };
    let Ok(child) = child.canonicalize() else {
        // 目标尚不存在时，退化为对父目录与文件名的判断。
        return match (child.parent(), child.file_name()) {
            (Some(dir), Some(name)) => dir
                .canonicalize()
                .map(|d| d == parent && !name.is_empty())
                .unwrap_or(false),
            _ => false,
        };
    };
    child.starts_with(&parent)
}

#[cfg(windows)]
fn restrict(path: &Path) {
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    use std::os::windows::fs::OpenOptionsExt;
    let _ = std::fs::OpenOptions::new()
        .write(true)
        .attributes(FILE_ATTRIBUTE_HIDDEN)
        .open(path);
}

#[cfg(not(windows))]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_descriptor_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "mclink-session-{}",
            crate::plugin::crypto::random_hex(5)
        ));
        let path =
            write_session_file(&dir, "dev.example.a", "ws://127.0.0.1:1/plugin", "tok", 5000)
                .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let desc: SessionDescriptor = serde_json::from_str(&text).unwrap();
        assert_eq!(desc.plugin_id, "dev.example.a");
        assert_eq!(desc.token, "tok");
        assert_eq!(desc.protocol, PROTOCOL_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn substitute_replaces_placeholders() {
        let out = substitute(
            "--s {token_file} --e {endpoint}",
            "ws://127.0.0.1:9/plugin",
            Path::new("C:/x/s.json"),
        );
        assert!(out.contains("ws://127.0.0.1:9/plugin"));
        assert!(out.contains("s.json"));
    }

    #[test]
    fn is_within_rejects_escaping_paths() {
        let dir = std::env::temp_dir();
        assert!(is_within(&dir, &dir.join("child.txt")));
        assert!(!is_within(&dir.join("a"), &dir.join("b.txt")));
    }
}
