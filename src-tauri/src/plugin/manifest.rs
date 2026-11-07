//! 插件清单（`plugin.json`）的结构、解析与校验。
//!
//! 清单是插件与核心之间唯一的静态契约。核心**只信任清单**，不信任插件进程
//! 的任何自述：插件通过网络声明自己是哪个插件，核心用清单里记录的 ID、版本
//! 与预共享密钥来核验。因此清单校验失败等于拒绝加载。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::plugin::permission::Permission;

// 展示维度的归一化（工作方式/平台/标签）单独成模块：它们与结构校验是两条路，
// 混在一起会让"哪条该严格"变得难以判断。这里再导出，调用方路径保持不变。
mod normalize;
pub use normalize::*;

/// 清单文件名。
pub const MANIFEST_FILE: &str = "plugin.json";

/// 插件种类。对应需求中的六类插件能力。
///
/// 前三类是联机链路的核心；后三类是 B7 扩展：
/// 桥接器（不携带引擎、只做协议兼容）、挂件（窗口内挂件 / 窗口外页面）、
/// 小工具（不参与链路的功能扩展）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// 适配类：负责 NAT 打洞 / 直连，向上层屏蔽网络拓扑差异。
    Adapter,
    /// 检测类：扫描本机游戏实例，回传连接信息。
    Detector,
    /// 耦合类：把具体游戏接入 MC Link 的统一调度流程。
    Coupler,
    /// 桥接类：**不携带引擎**，只做协议兼容/翻译（把某种游戏协议转成统一协议）。
    Bridge,
    /// 挂件类：向窗口内注入小挂件，或提供窗口外页面（`mount` 区分两种形态）。
    Widget,
    /// 工具类：不参与联机链路的小工具（脚本、批处理、辅助面板等）。
    Utility,
}

impl PluginKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginKind::Adapter => "adapter",
            PluginKind::Detector => "detector",
            PluginKind::Coupler => "coupler",
            PluginKind::Bridge => "bridge",
            PluginKind::Widget => "widget",
            PluginKind::Utility => "utility",
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
            // 桥接器只做协议翻译，不携带引擎，因此只需要本机回环 + 自身数据目录。
            PluginKind::Bridge => &[Permission::NetConnectLocal, Permission::FsPluginData],
            // 挂件向界面注入 UI，只需要通知能力与自身数据目录。
            PluginKind::Widget => &[Permission::UiNotify, Permission::FsPluginData],
            // 小工具默认不需要网络与游戏目录，只读写自身数据并推送通知。
            PluginKind::Utility => &[Permission::FsPluginData, Permission::UiNotify],
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
    /// Node 脚本，由**核心提供的共享 Bun** 拉起（`entry` 必须是插件目录内的
    /// `.js` / `.mjs` / `.cjs`；解释器路径由核心决定，见
    /// [`crate::plugin::runtime`]）。
    Node,
}

impl RuntimeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeKind::Builtin => "builtin",
            RuntimeKind::Process => "process",
            RuntimeKind::Node => "node",
        }
    }
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
        if self.runtime.kind == RuntimeKind::Node {
            let entry = self.runtime.entry.as_deref().ok_or_else(|| {
                "node 型插件必须在 runtime.entry 指定入口脚本（.js/.mjs/.cjs）".to_string()
            })?;
            if !is_safe_relative_path(entry) {
                return Err(format!("runtime.entry 必须是不越界的相对路径: {}", entry));
            }
            if !is_node_script(entry) {
                return Err(format!(
                    "node 型插件的 runtime.entry 必须是 .js/.mjs/.cjs: {}",
                    entry
                ));
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

/// 判断是否为 Node 入口脚本（扩展名大小写不敏感）。
///
/// 只认这三种扩展名：`.ts` 也能被 Bun 直接跑，但我们**刻意不支持**——发布包里的
/// TypeScript 会让"用户机器上跑的到底是什么"变得不确定（Bun 版本一变，转译规则就变）。
/// 作者应当在构建期用 `bun build` 产出 `.js`。
pub fn is_node_script(entry: &str) -> bool {
    let lower = entry.trim().to_ascii_lowercase();
    [".js", ".mjs", ".cjs"]
        .iter()
        .any(|ext| lower.ends_with(ext))
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
#[path = "manifest_tests.rs"]
mod tests;
