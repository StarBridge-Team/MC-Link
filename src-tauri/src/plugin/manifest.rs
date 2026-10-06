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

/// 工作方式：插件靠什么把局域网暴露给对端。
///
/// 与自由 `tags` 的区别在于**它必须可靠**——界面上的"按方式筛选"与 OOBE 的
/// 推荐展示都直接依赖它，所以这里是枚举而非字符串：写错的取值会在解析期被拒，
/// 不会变成筛选栏里第四个莫名的选项。
///
/// 刻意与"具体用什么软件"解耦：`bundled` 只说明"打包了第三方本体"，
/// 具体是哪个软件由插件自己声明（`tags` / 清单其它字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionMethod {
    /// 捆绑第三方软件本体（把别人家的整包带进来）。
    #[serde(rename = "bundled")]
    Bundled,
    /// P2P 打洞直连。
    #[serde(rename = "p2p")]
    P2p,
    /// 中继转发。
    #[serde(rename = "relay")]
    Relay,
    /// 内网映射 / 端口映射。
    #[serde(rename = "port-mapping")]
    PortMapping,
}

impl ConnectionMethod {
    /// 全部取值，供筛选界面与校验使用。
    pub const ALL: &'static [ConnectionMethod] = &[
        ConnectionMethod::Bundled,
        ConnectionMethod::P2p,
        ConnectionMethod::Relay,
        ConnectionMethod::PortMapping,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionMethod::Bundled => "bundled",
            ConnectionMethod::P2p => "p2p",
            ConnectionMethod::Relay => "relay",
            ConnectionMethod::PortMapping => "port-mapping",
        }
    }

    /// 从字符串解析（大小写不敏感，接受下划线写法）。
    pub fn parse(input: &str) -> Option<Self> {
        let normalized = input.trim().to_ascii_lowercase().replace('_', "-");
        ConnectionMethod::ALL
            .iter()
            .copied()
            .find(|m| m.as_str() == normalized)
    }
}

/// 已知平台取值。未知值在**展示层**被丢弃并留痕，但不会让插件加载失败——
/// 开发者写错一个平台名，不该让整个插件不可用。
pub const KNOWN_PLATFORMS: &[&str] = &["windows", "linux", "macos"];

/// 归一化平台取值：去空白、小写、去重，并丢掉不在 [`KNOWN_PLATFORMS`] 里的。
pub fn normalize_platforms(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let value = item.trim().to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        if !KNOWN_PLATFORMS.contains(&value.as_str()) {
            eprintln!("[插件] 未知平台「{value}」已忽略（可选：windows/linux/macos）");
            continue;
        }
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

/// 归一化工作方式：丢掉未知取值并留痕。
///
/// 与 [`normalize_platforms`] 同口径——写错的取值只影响这一项，不会让插件不可用。
pub fn normalize_methods(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        match ConnectionMethod::parse(item) {
            Some(method) => {
                let value = method.as_str().to_string();
                if !out.contains(&value) {
                    out.push(value);
                }
            }
            None => eprintln!(
                "[插件] 未知工作方式「{}」已忽略（可选：bundled/p2p/relay/port-mapping）",
                item.trim()
            ),
        }
    }
    out
}

/// 归一化自由标签：去空白、去重、限长限量。
///
/// **不做白名单**（开发者可自定义，中文标签是允许的），只做防脏数据的清洗：
/// 一个插件写 200 个标签会让筛选栏彻底不可用。
pub fn normalize_tags(raw: &[String]) -> Vec<String> {
    const MAX_TAGS: usize = 12;
    const MAX_LEN: usize = 24;
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let value = item.trim();
        if value.is_empty() {
            continue;
        }
        let value: String = value.chars().take(MAX_LEN).collect();
        if !out.contains(&value) {
            out.push(value);
        }
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
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

/// 核心侧的硬上限：插件清单只能在这些范围内"调参"。
const RPC_PER_SEC_MAX: u32 = 256;
const HEARTBEAT_MS_RANGE: (u64, u64) = (500, 60_000);
const IDLE_TIMEOUT_MS_RANGE: (u64, u64) = (2_000, 300_000);
const RPC_TIMEOUT_MS_RANGE: (u64, u64) = (1_000, 300_000);
const MAX_AUTH_FAILURES_RANGE: (u32, u32) = (1, 10);

impl LimitsSpec {
    /// 夹取到核心允许的范围后返回。
    ///
    /// 清单是插件**自述**的，完全照用等于把限流开关交给插件：
    /// 写 `max_rpc_per_sec: 1000000` 就能关掉核心侧限流（与"限流全部在核心侧
    /// 强制执行"的设计相矛盾），写 `rpc_timeout_ms: 10^12` 能让核心的 RPC
    /// 近乎永久挂起，写超大的 `idle_timeout_ms` 则让死连接永远判不出失联。
    pub fn clamped(&self) -> Self {
        let clamp = |v: u64, (lo, hi): (u64, u64)| v.clamp(lo, hi);
        Self {
            max_rpc_per_sec: self.max_rpc_per_sec.clamp(1, RPC_PER_SEC_MAX),
            // 帧上限另有 `protocol::MAX_FRAME_BYTES` 兜底（会话侧会再取一次 min），
            // 且 validate 已限制在 1B..64MB，这里不必重复夹取
            max_frame_bytes: self.max_frame_bytes,
            heartbeat_ms: clamp(self.heartbeat_ms, HEARTBEAT_MS_RANGE),
            idle_timeout_ms: clamp(self.idle_timeout_ms, IDLE_TIMEOUT_MS_RANGE),
            rpc_timeout_ms: clamp(self.rpc_timeout_ms, RPC_TIMEOUT_MS_RANGE),
            max_auth_failures: self
                .max_auth_failures
                .clamp(MAX_AUTH_FAILURES_RANGE.0, MAX_AUTH_FAILURES_RANGE.1),
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
    /// 可运行的平台（`windows` / `linux` / `macos`）；空表示不限。
    ///
    /// 独立于自由标签：这是界面用来给用户**屏蔽跑不了的插件**的可靠维度。
    #[serde(default)]
    pub platforms: Vec<String>,
    /// 工作方式（P2P / 中继 / 端口映射 / 捆绑第三方本体）；空表示未声明。
    ///
    /// 存字符串而非 [`ConnectionMethod`] 枚举：**枚举在反序列化期遇到未知取值会直接
    /// 报错，让整个插件加载失败**——开发者写错一个词就不该让插件变砖。
    /// 取值由 [`normalize_methods`] 在展示层过滤并留痕，与 `platforms` 口径一致。
    #[serde(default)]
    pub methods: Vec<String>,
    /// 开发者自定义标签，用于长尾筛选与展示。
    #[serde(default)]
    pub tags: Vec<String>,
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
        self.games
            .iter_mut()
            .for_each(|g| *g = g.trim().to_lowercase());
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
            let entry =
                self.runtime.entry.as_deref().ok_or_else(|| {
                    "process 型插件必须在 runtime.entry 指定可执行文件".to_string()
                })?;
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
        self.games
            .iter()
            .any(|g| g == "*" || g.eq_ignore_ascii_case(game_id))
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
        self.runtime.entry.as_deref().map(|e| plugin_dir.join(e))
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
    let ok = id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'));
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
///
/// # 为什么显式识别盘符而不是只靠 `is_absolute()`
///
/// `is_absolute()` 跟随**编译平台**：Linux 上 `Path::new("C:/evil.exe")` 不是绝对路径，
/// `C:` 只是个普通文件名组件，于是会**通过**校验。同一份清单在 Windows 与 Linux
/// 构建下被解释成不同结果 —— 审计记录过的"平台相关行为漂移"。
/// 这里改为按字符串直接拒绝，判定在所有平台一致。
pub fn is_safe_relative_path(p: &str) -> bool {
    let p = p.trim();
    if p.is_empty() {
        return false;
    }

    // Windows 盘符：`C:` / `C:/` / `C:\`
    let bytes = p.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return false;
    }
    // UNC：`\\server\share` 或 `//server/share`
    if p.starts_with(r"\\") || p.starts_with("//") {
        return false;
    }
    // 反斜杠一律拒绝（合法相对路径只用 `/`）
    if p.contains('\\') {
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

    fn parse(
        mut value: serde_json::Value,
        patch: impl FnOnce(&mut serde_json::Value),
    ) -> Result<PluginManifest, String> {
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

    /// 越界入口必须被拒，且**判定与编译平台无关**。
    ///
    /// `C:/evil.exe` 曾经只在 Windows 被拒（Linux 下 `is_absolute()` 为 false、
    /// `C:` 是普通组件），导致同一份清单在两个平台校验结果不同。
    #[test]
    fn rejects_escaping_entry() {
        for bad in [
            "../../evil.exe",
            "/etc/passwd",
            // Windows 盘符（正/反斜杠两种写法）
            "C:/evil.exe",
            r"C:\evil.exe",
            "d:evil.exe",
            // UNC 与反斜杠逃逸
            r"\\server\share\evil.exe",
            r"..\..\evil.exe",
            // 空 / 仅空白
            "",
            "   ",
        ] {
            assert!(
                parse(sample(), |v| v["runtime"]["entry"] = bad.into()).is_err(),
                "入口 {bad:?} 应被拒绝"
            );
        }
    }

    #[test]
    fn process_runtime_requires_entry() {
        assert!(parse(sample(), |v| v["runtime"]["entry"] =
            serde_json::Value::Null)
        .is_err());
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

    /// 平台：未知取值丢弃、大小写归一、去重，但**不影响插件加载**。
    #[test]
    fn platform_normalization_drops_unknown_values() {
        let out = normalize_platforms(&[
            "Windows".to_string(),
            " windows ".to_string(),
            "linux".to_string(),
            "solaris".to_string(),
            String::new(),
        ]);
        assert_eq!(out, vec!["windows".to_string(), "linux".to_string()]);
    }

    /// 标签是开发者自定义的：不设白名单（中文允许），但要去重、截长、限量。
    #[test]
    fn tag_normalization_is_lenient_but_bounded() {
        let out = normalize_tags(&["免登录".to_string(), "免登录".to_string(), "x".repeat(200)]);
        assert_eq!(out.len(), 2, "重复标签必须合并: {out:?}");
        assert_eq!(out[0], "免登录");
        assert_eq!(out[1].chars().count(), 24, "超长标签必须截断");

        let many: Vec<String> = (0..50).map(|i| format!("tag-{i}")).collect();
        assert_eq!(normalize_tags(&many).len(), 12, "标签数量必须封顶");
    }

    /// 工作方式的字符串表示必须稳定（筛选界面直接依赖），解析要宽容。
    #[test]
    fn connection_method_parsing_is_tolerant_and_stable() {
        assert_eq!(ConnectionMethod::P2p.as_str(), "p2p");
        assert_eq!(ConnectionMethod::PortMapping.as_str(), "port-mapping");
        assert_eq!(
            ConnectionMethod::parse(" P2P "),
            Some(ConnectionMethod::P2p)
        );
        assert_eq!(
            ConnectionMethod::parse("port_mapping"),
            Some(ConnectionMethod::PortMapping)
        );
        assert_eq!(ConnectionMethod::parse("telepathy"), None);
        assert_eq!(ConnectionMethod::ALL.len(), 4);
    }

    /// 写错的工作方式只应被丢掉，不能让插件加载失败（与平台同口径）。
    #[test]
    fn unknown_method_is_dropped_not_fatal() {
        let m = parse(sample(), |v| {
            v["methods"] = serde_json::json!(["p2p", "telepathy", "relay"]);
        })
        .expect("未知工作方式不应导致解析失败");
        assert_eq!(
            normalize_methods(&m.methods),
            vec!["p2p".to_string(), "relay".to_string()]
        );
    }

    /// 声明了三个新维度的清单要能完整解析出来。
    #[test]
    fn parses_platforms_methods_and_tags() {
        let m = parse(sample(), |v| {
            v["platforms"] = serde_json::json!(["windows", "linux"]);
            v["methods"] = serde_json::json!(["p2p", "relay"]);
            v["tags"] = serde_json::json!(["低延迟"]);
        })
        .unwrap();
        assert_eq!(
            normalize_platforms(&m.platforms),
            vec!["windows".to_string(), "linux".to_string()]
        );
        assert_eq!(
            normalize_methods(&m.methods),
            vec!["p2p".to_string(), "relay".to_string()]
        );
        assert_eq!(normalize_tags(&m.tags), vec!["低延迟".to_string()]);
    }
}
