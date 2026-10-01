//! 插件清单（`plugin.json`）的结构、解析与校验。
//!
//! 清单是插件与核心之间唯一的静态契约。核心**只信任清单**，不信任插件进程
//! 的任何自述：插件通过网络声明自己是哪个插件，核心用清单里记录的 ID、版本
//! 与预共享密钥来核验。因此清单校验失败等于拒绝加载。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::plugin::permission::Permission;

/// 清单文件名。
pub const MANIFEST_FILE: &str = "plugin.json";

/// 插件种类。对应需求中的三类插件能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// 适配类：负责 NAT 打洞 / 直连，向上层屏蔽网络拓扑差异。
    Adapter,
    /// 检测类：扫描本机游戏实例，回传连接信息。
    Detector,
    /// 耦合类：把具体游戏接入 MC Link 的统一调度流程。
    Coupler,
}

impl PluginKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginKind::Adapter => "adapter",
            PluginKind::Detector => "detector",
            PluginKind::Coupler => "coupler",
        }
    }

    /// 该种类插件默认需要的权限集合。
    ///
    /// 清单里声明得更少是允许的（插件可能只实现子功能），声明得更多则需要
    /// 用户额外授权。
    pub fn default_permissions(&self) -> &'static [Permission] {
        match self {
            PluginKind::Adapter => &[
                Permission::NetListenLocal,
                Permission::NetConnectLocal,
                Permission::NetConnectAny,
                Permission::NetUdp,
            ],
            PluginKind::Detector => &[
                Permission::GameScan,
                Permission::ProcInspect,
                Permission::NetUdp,
                Permission::FsReadGameDirs,
            ],
            PluginKind::Coupler => &[
                Permission::NetConnectLocal,
                Permission::ProcSpawnGame,
                Permission::FsReadGameDirs,
            ],
        }
    }
}

/// 运行时形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    /// 内置于核心进程，不经过 WebSocket（官方适配器走这条路）。
    Builtin,
    /// 独立子进程，通过回环 WebSocket 与核心通信。
    Process,
}

/// 启动描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSpec {
    pub kind: RuntimeKind,
    /// 相对于插件目录的可执行文件路径（`Process` 必填）。
    #[serde(default)]
    pub entry: Option<String>,
    /// 启动参数。支持占位符 `{endpoint}` 与 `{token_file}`。
    #[serde(default)]
    pub args: Vec<String>,
    /// 工作目录，相对于插件目录。
    #[serde(default)]
    pub workdir: Option<String>,
    /// 附加环境变量。
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl Default for RuntimeSpec {
    fn default() -> Self {
        Self {
            kind: RuntimeKind::Builtin,
            entry: None,
            args: Vec::new(),
            workdir: None,
            env: BTreeMap::new(),
        }
    }
}

/// 资源与速率限制。超限即断开，防止恶意插件耗尽核心资源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsSpec {
    /// 每秒允许的 RPC 次数。
    #[serde(default = "default_rpc_per_sec")]
    pub max_rpc_per_sec: u32,
    /// 单帧最大字节数。
    #[serde(default = "default_frame_bytes")]
    pub max_frame_bytes: usize,
    /// 心跳间隔（毫秒）。
    #[serde(default = "default_heartbeat_ms")]
    pub heartbeat_ms: u64,
    /// 心跳丢失多少毫秒后判定失联。
    #[serde(default = "default_idle_timeout_ms")]
    pub idle_timeout_ms: u64,
    /// 单次 RPC 超时（毫秒）。
    #[serde(default = "default_rpc_timeout_ms")]
    pub rpc_timeout_ms: u64,
    /// 连续握手失败多少次后拉黑（进程内计数）。
    #[serde(default = "default_max_auth_failures")]
    pub max_auth_failures: u32,
}

fn default_rpc_per_sec() -> u32 {
    32
}
fn default_frame_bytes() -> usize {
    1024 * 1024
}
fn default_heartbeat_ms() -> u64 {
    5_000
}
fn default_idle_timeout_ms() -> u64 {
    20_000
}
fn default_rpc_timeout_ms() -> u64 {
    30_000
}
fn default_max_auth_failures() -> u32 {
    5
}

impl Default for LimitsSpec {
    fn default() -> Self {
        Self {
            max_rpc_per_sec: default_rpc_per_sec(),
            max_frame_bytes: default_frame_bytes(),
            heartbeat_ms: default_heartbeat_ms(),
            idle_timeout_ms: default_idle_timeout_ms(),
            rpc_timeout_ms: default_rpc_timeout_ms(),
            max_auth_failures: default_max_auth_failures(),
        }
    }
}

// 这里曾有一个 `SignatureSpec`（`algorithm` / `public_key` / `signature`），
// 让清单自带"我已被签名"的声明。那等于把"自述即可信"写进权限模型：任何人填一对
// 假公钥假签名就能拿到 `Verified` 的权限上限。
//
// 信任必须来自**外部**：客户端内置发布者公钥，对**整个插件包**验签，见
// [`crate::plugin::trust`]。清单里的任何字段都不再影响信任等级。
// 注意 serde 默认忽略未知字段，因此旧清单里残留的 `signature` 字段会被静默忽略，
// 既不会报错也不会产生任何效果——这正是我们想要的。

/// 插件清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// 唯一 ID，反向域名风格，如 `dev.mclink.terracotta`。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 语义化版本。
    pub version: String,
    /// 插件种类。
    pub kind: PluginKind,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// 覆盖的游戏 ID 列表；`["*"]` 表示与具体游戏无关（适配器通常如此）。
    #[serde(default)]
    pub games: Vec<String>,
    /// 声明需要的权限。
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub runtime: RuntimeSpec,
    #[serde(default)]
    pub limits: LimitsSpec,
    /// 优先级，数值越大越先被路由选中。
    #[serde(default)]
    pub priority: i32,
    /// 同种类回退链，供路由在首选插件不可用时降级。
    #[serde(default)]
    pub fallback: Vec<String>,
    /// 清单要求的协议版本。
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u16,
}

fn default_protocol_version() -> u16 {
    crate::plugin::protocol::PROTOCOL_VERSION
}

impl PluginManifest {
    /// 解析并校验清单。
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut manifest: PluginManifest =
            serde_json::from_str(text).map_err(|e| format!("清单解析失败: {}", e))?;
        manifest.normalize();
        manifest.validate()?;
        Ok(manifest)
    }

    /// 从插件目录读取 `plugin.json`。
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
        Self::parse(&text)
    }

    /// 规范化：填充默认值、去重。
    pub fn normalize(&mut self) {
        if self.games.is_empty() {
            self.games.push("*".to_string());
        }
        self.games.iter_mut().for_each(|g| *g = g.trim().to_lowercase());
        self.games.dedup();
        self.permissions.sort();
        self.permissions.dedup();
    }

    /// 结构性校验。任何一条不通过都视为不可信清单。
    pub fn validate(&self) -> Result<(), String> {
        validate_plugin_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err("清单缺少 name".to_string());
        }
        if self.version.trim().is_empty() || self.version.len() > 32 {
            return Err("清单 version 非法".to_string());
        }
        if self.protocol_version != crate::plugin::protocol::PROTOCOL_VERSION {
            return Err(format!(
                "协议版本不匹配：插件 {}，核心 {}",
                self.protocol_version,
                crate::plugin::protocol::PROTOCOL_VERSION
            ));
        }
        if self.runtime.kind == RuntimeKind::Process {
            let entry = self
                .runtime
                .entry
                .as_deref()
                .ok_or_else(|| "process 型插件必须在 runtime.entry 指定可执行文件".to_string())?;
            if !is_safe_relative_path(entry) {
                return Err(format!("runtime.entry 必须是不越界的相对路径: {}", entry));
            }
        }
        if let Some(wd) = self.runtime.workdir.as_deref() {
            if !is_safe_relative_path(wd) {
                return Err(format!("runtime.workdir 必须是不越界的相对路径: {}", wd));
            }
        }
        if self.limits.max_frame_bytes == 0 || self.limits.max_frame_bytes > 64 * 1024 * 1024 {
            return Err("limits.max_frame_bytes 超出允许区间 (1B ~ 64MB)".to_string());
        }
        for target in &self.fallback {
            validate_plugin_id(target)?;
        }
        Ok(())
    }

    /// 该插件是否覆盖指定游戏。
    pub fn covers_game(&self, game_id: &str) -> bool {
        self.games.iter().any(|g| g == "*" || g.eq_ignore_ascii_case(game_id))
    }

    /// 解析出绝对工作目录。
    pub fn resolve_workdir(&self, plugin_dir: &Path) -> PathBuf {
        match self.runtime.workdir.as_deref() {
            Some(wd) if !wd.is_empty() => plugin_dir.join(wd),
            _ => plugin_dir.to_path_buf(),
        }
    }

    /// 解析出可执行文件的绝对路径。
    pub fn resolve_entry(&self, plugin_dir: &Path) -> Option<PathBuf> {
        self.runtime
            .entry
            .as_deref()
            .map(|e| plugin_dir.join(e))
    }
}

/// 校验插件 ID：小写字母、数字、点、短横线、下划线；至少两段。
pub fn validate_plugin_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 96 {
        return Err("插件 ID 长度非法".to_string());
    }
    if !id.contains('.') {
        return Err(format!("插件 ID 必须是反向域名风格（至少两段）: {}", id));
    }
    let ok = id.chars().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_')
    });
    if !ok {
        return Err(format!("插件 ID 含非法字符: {}", id));
    }
    if id.starts_with('.') || id.ends_with('.') || id.contains("..") {
        return Err(format!("插件 ID 点号位置非法: {}", id));
    }
    Ok(())
}

/// 判断是否为不越界的相对路径。
///
/// 拒绝绝对路径、盘符前缀、`..` 以及路径分隔符混合等一切可能逃逸插件目录的写法。
pub fn is_safe_relative_path(p: &str) -> bool {
    if p.trim().is_empty() {
        return false;
    }
    let path = Path::new(p);
    if path.is_absolute() {
        return false;
    }
    for comp in path.components() {
        match comp {
            Component::Normal(_) => {}
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::permission::TrustLevel;

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "id": "dev.example.adapter",
            "name": "示例适配器",
            "version": "1.0.0",
            "kind": "adapter",
            "permissions": ["net_listen_local", "net_udp"],
            "runtime": { "kind": "process", "entry": "bin/adapter.exe" }
        })
    }

    fn parse(mut value: serde_json::Value, patch: impl FnOnce(&mut serde_json::Value)) -> Result<PluginManifest, String> {
        patch(&mut value);
        PluginManifest::parse(&value.to_string())
    }

    #[test]
    fn parses_minimal_manifest() {
        let m = PluginManifest::parse(&sample().to_string()).unwrap();
        assert_eq!(m.kind, PluginKind::Adapter);
        assert_eq!(m.games, vec!["*"]);
    }

    /// 回归保护：清单里写什么都不能影响信任。
    ///
    /// 早期版本让清单自带 `signature` 字段并据此提权，那是"自述即可信"。现在
    /// 该字段已从结构体移除，serde 会静默忽略它——本用例锁死这个行为：即使插件
    /// 声明了 `permissions` 里的高权限、又伪造了签名字段，也只能拿到 `Unsigned`
    /// 的权限上限。
    #[test]
    fn manifest_cannot_self_declare_trust() {
        let m = parse(sample(), |v| {
            v["permissions"] = serde_json::json!(["net_listen_public", "proc_spawn_game"]);
            v["signature"] = serde_json::json!({
                "algorithm": "ed25519",
                "public_key": "deadbeef",
                "signature": "cafebabe"
            });
        })
        .expect("未知字段应被静默忽略，而不是解析失败");

        let set = crate::plugin::permission::PermissionSet::resolve(
            &m.permissions,
            TrustLevel::Unsigned,
            None,
        );
        assert!(!set.contains(crate::plugin::permission::Permission::ProcSpawnGame));
        assert!(!set.contains(crate::plugin::permission::Permission::NetListenPublic));
    }

    #[test]
    fn rejects_non_dotted_id() {
        assert!(parse(sample(), |v| v["id"] = "adapter".into()).is_err());
    }

    #[test]
    fn rejects_escaping_entry() {
        assert!(parse(sample(), |v| v["runtime"]["entry"] = "../../evil.exe".into()).is_err());
        assert!(parse(sample(), |v| v["runtime"]["entry"] = "C:/evil.exe".into()).is_err());
        assert!(parse(sample(), |v| v["runtime"]["entry"] = "/etc/passwd".into()).is_err());
    }

    #[test]
    fn process_runtime_requires_entry() {
        assert!(parse(sample(), |v| v["runtime"]["entry"] = serde_json::Value::Null).is_err());
    }

    #[test]
    fn covers_game_matches_wildcard_and_id() {
        let m = PluginManifest::parse(&sample().to_string()).unwrap();
        assert!(m.covers_game("minecraft-java"));

        let m = parse(sample(), |v| {
            v["kind"] = "detector".into();
            v["games"] = serde_json::json!(["terraria"]);
        })
        .unwrap();
        assert!(m.covers_game("Terraria"));
        assert!(!m.covers_game("minecraft-java"));
    }

    #[test]
    fn rejects_unknown_protocol_version() {
        assert!(parse(sample(), |v| v["protocol_version"] = 99.into()).is_err());
    }

    #[test]
    fn rejects_absurd_frame_limit() {
        assert!(parse(sample(), |v| v["limits"]["max_frame_bytes"] = 0.into()).is_err());
    }
}
