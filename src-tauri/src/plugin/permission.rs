//! 插件权限模型与最小权限校验。
//!
//! 三条规则：
//! 1. **清单声明**：插件必须在 `plugin.json` 里声明它需要的能力。
//! 2. **信任度上限**：未签名插件的可申请权限被信任度封顶，签名不能自证。
//! 3. **用户授权**：最终生效权限 = 清单声明 ∩ 信任度上限 ∩（用户授予 或 自动放行集）。
//!
//! 第 3 条里的"或"是关键：**用户没做过授权决策（`None`）不等于全放行**。
//! 未验签插件在用户尚未授权时只拿到 [`MINIMAL`]（回环通信 + 自身数据目录），
//! 其余权限必须由用户在界面上显式授予。详见 [`PermissionSet::resolve`]。
//!
//! 核心侧在执行任何敏感 RPC 前都会调用 [`PermissionSet::require`]，插件无法
//! 通过伪造请求绕过——权限判定发生在核心进程内，而非插件进程内。

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::plugin::manifest::PluginKind;
use crate::plugin::protocol::{adapter_method, coupler_method, detector_method, ErrorInfo};

/// 插件可申请的权限项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// 在回环地址上监听端口（适配器/耦合器的常规需求）。
    NetListenLocal,
    /// 在非回环地址上监听端口（面向局域网直连，风险较高）。
    NetListenPublic,
    /// 连接回环地址。
    NetConnectLocal,
    /// 连接任意地址（打洞、探测对端、访问中继）。
    NetConnectAny,
    /// 收发 UDP（打洞必需，同时也是滥用风险点）。
    NetUdp,
    /// 使用 UPnP / NAT-PMP 请求端口映射。
    NetNatMapping,
    /// 读取已登记游戏目录中的文件（存档、配置文件、服务器列表）。
    FsReadGameDirs,
    /// 读写自身插件数据目录。
    FsPluginData,
    /// 执行自身插件目录内的可执行文件。
    ProcSpawnSelf,
    /// 拉起游戏进程。
    ProcSpawnGame,
    /// 枚举系统进程（用于检测游戏是否在运行）。
    ProcInspect,
    /// 只读访问注册表（Windows 游戏安装位置探测）。
    RegistryRead,
    /// 扫描本机游戏实例（组合了进程枚举 + 端口探测 + 局域网监听）。
    GameScan,
    /// 向核心推送用户可见通知。
    UiNotify,
}

impl Permission {
    /// 面向用户的说明文案，用于授权界面展示。
    pub fn describe(&self) -> &'static str {
        match self {
            Permission::NetListenLocal => "在本机回环地址监听端口",
            Permission::NetListenPublic => "在局域网/公网地址监听端口",
            Permission::NetConnectLocal => "连接本机端口",
            Permission::NetConnectAny => "连接任意网络地址",
            Permission::NetUdp => "收发 UDP 数据报",
            Permission::NetNatMapping => "向路由器申请端口映射（UPnP/NAT-PMP）",
            Permission::FsReadGameDirs => "读取已登记的游戏目录",
            Permission::FsPluginData => "读写插件自身数据目录",
            Permission::ProcSpawnSelf => "启动插件自身的可执行文件",
            Permission::ProcSpawnGame => "启动游戏进程",
            Permission::ProcInspect => "枚举系统进程列表",
            Permission::RegistryRead => "只读访问系统注册表",
            Permission::GameScan => "扫描本机游戏实例",
            Permission::UiNotify => "向界面推送通知",
        }
    }

    /// 是否属于高风险权限（授权界面需要二次确认）。
    pub fn is_high_risk(&self) -> bool {
        matches!(
            self,
            Permission::NetListenPublic
                | Permission::NetConnectAny
                | Permission::NetUdp
                | Permission::NetNatMapping
                | Permission::FsReadGameDirs
                | Permission::ProcSpawnGame
                | Permission::ProcInspect
                | Permission::RegistryRead
                | Permission::GameScan
        )
    }
}

/// 插件信任度。由清单签名校验结果决定，插件无法自行声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// 官方内置或官方签名。
    Official,
    /// 第三方但签名有效且发布者在信任列表中。
    Verified,
    /// 无签名（默认）。
    Unsigned,
    /// 签名无效、清单被篡改或用户显式拉黑。
    Blocked,
}

/// 用户**尚未做授权决策**时，未验签插件自动获得的权限。
///
/// 只包含"能跑起来、并且只跟核心说话"所必需的项：
/// 回环监听（适配器/耦合器的常规形态）、连接回环、读写自身数据目录。
///
/// 刻意**不含** `NetConnectAny` / `NetUdp` / `FsReadGameDirs` / `ProcInspect` /
/// `GameScan` 等——它们要么越过本机边界，要么触碰用户的其他数据，必须由用户显式授权。
/// 改动这里等于改动未签名插件的默认能力，请同时更新 `插件开发规范.md` 的对照表。
pub const MINIMAL: &[Permission] = &[
    Permission::NetListenLocal,
    Permission::NetConnectLocal,
    Permission::FsPluginData,
];

impl TrustLevel {
    /// 用户**未做授权决策**时，是否按清单声明直接放行。
    ///
    /// 官方内置与已验签插件的清单来自可信发布者，可以按声明放行（仍受上限约束）；
    /// 未验签插件只给 [`MINIMAL`]——这正是"未授权 ≠ 全放行"的落点。
    pub fn auto_grants_declared(&self) -> bool {
        matches!(self, TrustLevel::Official | TrustLevel::Verified)
    }

    /// 该信任度下允许申请的权限上限。
    pub fn ceiling(&self) -> &'static [Permission] {
        const LOCAL_ONLY: &[Permission] = &[Permission::NetConnectLocal, Permission::FsPluginData];
        const UNSIGNED: &[Permission] = &[
            Permission::NetListenLocal,
            Permission::NetConnectLocal,
            Permission::NetConnectAny,
            Permission::NetUdp,
            Permission::FsReadGameDirs,
            Permission::FsPluginData,
            Permission::ProcSpawnSelf,
            Permission::GameScan,
            Permission::ProcInspect,
        ];
        const FULL: &[Permission] = &[
            Permission::NetListenLocal,
            Permission::NetListenPublic,
            Permission::NetConnectLocal,
            Permission::NetConnectAny,
            Permission::NetUdp,
            Permission::NetNatMapping,
            Permission::FsReadGameDirs,
            Permission::FsPluginData,
            Permission::ProcSpawnSelf,
            Permission::ProcSpawnGame,
            Permission::ProcInspect,
            Permission::RegistryRead,
            Permission::GameScan,
            Permission::UiNotify,
        ];
        match self {
            TrustLevel::Official | TrustLevel::Verified => FULL,
            // 未签名插件默认只能做回环通信与自身数据读写，由用户显式授权才能放开。
            TrustLevel::Unsigned => UNSIGNED,
            TrustLevel::Blocked => LOCAL_ONLY,
        }
    }

    pub fn is_runnable(&self) -> bool {
        !matches!(self, TrustLevel::Blocked)
    }
}

/// 某个插件最终生效的权限集合。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    granted: BTreeSet<Permission>,
    /// 被信任度上限裁剪掉的权限，用于向用户解释"为什么装了却用不了"。
    denied_by_ceiling: Vec<Permission>,
}

impl PermissionSet {
    /// 计算生效权限。
    ///
    /// | 用户是否做过授权决策 | 生效权限 |
    /// |---|---|
    /// | 做过（`Some`） | 用户授予 ∩ 信任度上限 —— 用户决策优先，因此**可撤销** |
    /// | 没做过（`None`） | 清单声明 ∩ 信任度上限 ∩ 自动放行集 |
    ///
    /// 第二行是关键：`None` 曾被实现成"按清单声明放行到信任度上限"，于是
    /// **"从未授权"等于"全放行"**，与最小权限原则相反，也让未验签插件的上限形同虚设。
    /// 现在 `None` 只放行 [`TrustLevel::auto_grants_declared`] 允许的部分：
    /// 官方/已验签插件按声明放行，未验签插件只拿到 [`MINIMAL`]。
    ///
    /// `denied_by_ceiling` 只记录**因信任度上限**被拒的项；因"等用户授权"而未生效的项
    /// 不进这个列表——界面上它们是"待授权"，而不是"被核心封顶拒绝"。
    pub fn resolve(
        declared: &[Permission],
        trust: TrustLevel,
        user_granted: Option<&[Permission]>,
    ) -> Self {
        let ceiling = trust.ceiling();
        let auto_grants = trust.auto_grants_declared();
        let mut granted = BTreeSet::new();
        let mut denied_by_ceiling = Vec::new();

        for perm in declared {
            if !ceiling.contains(perm) {
                denied_by_ceiling.push(*perm);
                continue;
            }
            match user_granted {
                // 用户已做过授权决策：完全以用户决策为准（这是"撤销权限"能生效的前提）
                Some(user) => {
                    if user.contains(perm) {
                        granted.insert(*perm);
                    }
                }
                // 用户未决策：只放行无需用户确认的部分
                None => {
                    if auto_grants || MINIMAL.contains(perm) {
                        granted.insert(*perm);
                    }
                }
            }
        }

        Self {
            granted,
            denied_by_ceiling,
        }
    }

    /// 官方内置插件使用：直接采用清单声明（清单本身就来自核心代码）。
    pub fn from_declared(declared: &[Permission]) -> Self {
        Self {
            granted: declared.iter().copied().collect(),
            denied_by_ceiling: Vec::new(),
        }
    }

    pub fn contains(&self, perm: Permission) -> bool {
        self.granted.contains(&perm)
    }

    pub fn allowed(&self) -> Vec<Permission> {
        self.granted.iter().copied().collect()
    }

    pub fn denied_by_ceiling(&self) -> &[Permission] {
        &self.denied_by_ceiling
    }

    /// 校验是否具备某项权限；缺失时返回可直接回传给插件的拒绝原因。
    pub fn require(&self, perm: Permission) -> Result<(), ErrorInfo> {
        if self.granted.contains(&perm) {
            return Ok(());
        }
        let reason = if self.denied_by_ceiling.contains(&perm) {
            "插件信任度不足，该权限已被核心封顶策略拒绝"
        } else {
            "权限未授予，请在插件设置中显式授权"
        };
        Err(ErrorInfo::forbidden(format!(
            "{}（缺少权限 {}）",
            reason,
            perm_name(perm)
        )))
    }
}

/// 某能力方法所要求的最小权限。
///
/// 这张映射表是权限模型真正的落地点：核心在派发调用前用它查表，再交给
/// [`PermissionSet::require`] 判定。插件无法通过伪造参数绕过，因为判定发生在
/// 核心进程内，且判定依据来自清单与用户授权，而非请求内容。
pub fn required_for(kind: PluginKind, method: &str) -> Permission {
    match kind {
        PluginKind::Adapter => match method {
            adapter_method::HOST_START | adapter_method::JOIN | adapter_method::PROBE => {
                // 打洞/直连要主动连出去，属于"连接任意地址"。
                Permission::NetConnectAny
            }
            _ => Permission::NetListenLocal,
        },
        PluginKind::Detector => match method {
            detector_method::SCAN => Permission::GameScan,
            detector_method::WATCH_START | detector_method::WATCH_STOP => Permission::GameScan,
            _ => Permission::NetConnectLocal,
        },
        PluginKind::Coupler => match method {
            coupler_method::LAUNCH => Permission::ProcSpawnGame,
            coupler_method::ATTACH | coupler_method::DETACH => Permission::NetConnectLocal,
            _ => Permission::FsReadGameDirs,
        },
    }
}

/// 某类插件"能干活"所必需的最低权限。
///
/// 路由用它排除"选中了也做不了事"的候选：未验签插件默认拿不到这些权限，
/// 于是它们天然不会成为首选适配器——这比只靠分值排序更可靠
/// （分值可以被清单自述影响，权限不能）。
pub fn kind_baseline(kind: PluginKind) -> Permission {
    match kind {
        // 适配器的核心动作是打洞/直连，需要主动连出去
        PluginKind::Adapter => Permission::NetConnectAny,
        // 探测器要扫描本机游戏实例
        PluginKind::Detector => Permission::GameScan,
        // 耦合器要拉起游戏进程
        PluginKind::Coupler => Permission::ProcSpawnGame,
    }
}

/// 权限的稳定字符串名（与 serde 序列化保持一致，用于日志与前端展示）。
pub fn perm_name(perm: Permission) -> &'static str {
    match perm {
        Permission::NetListenLocal => "net_listen_local",
        Permission::NetListenPublic => "net_listen_public",
        Permission::NetConnectLocal => "net_connect_local",
        Permission::NetConnectAny => "net_connect_any",
        Permission::NetUdp => "net_udp",
        Permission::NetNatMapping => "net_nat_mapping",
        Permission::FsReadGameDirs => "fs_read_game_dirs",
        Permission::FsPluginData => "fs_plugin_data",
        Permission::ProcSpawnSelf => "proc_spawn_self",
        Permission::ProcSpawnGame => "proc_spawn_game",
        Permission::ProcInspect => "proc_inspect",
        Permission::RegistryRead => "registry_read",
        Permission::GameScan => "game_scan",
        Permission::UiNotify => "ui_notify",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RISKY: &[Permission] = &[Permission::NetConnectAny, Permission::FsReadGameDirs];

    #[test]
    fn unsigned_plugin_cannot_get_public_listen() {
        let declared = [Permission::NetListenPublic, Permission::NetUdp];
        let set = PermissionSet::resolve(&declared, TrustLevel::Unsigned, None);
        assert!(!set.contains(Permission::NetListenPublic));
        assert!(
            !set.contains(Permission::NetUdp),
            "未授权时不得自动拿到 UDP"
        );
        assert_eq!(set.denied_by_ceiling(), &[Permission::NetListenPublic]);
        assert!(set.require(Permission::NetListenPublic).is_err());
    }

    /// 核心回归：**"用户没做过授权决策"不等于"全放行"**。
    ///
    /// 旧实现在 `None` 时按清单声明放行到信任度上限，于是未验签插件只要在清单里
    /// 声明 `net_connect_any` 就能自动拿到——最小权限原则形同虚设。
    #[test]
    fn no_user_decision_grants_only_minimal_for_unsigned() {
        let set = PermissionSet::resolve(RISKY, TrustLevel::Unsigned, None);
        assert!(
            !set.contains(Permission::NetConnectAny),
            "未验签插件不得自动拿到外连权限"
        );
        assert!(!set.contains(Permission::FsReadGameDirs));
        // 这两项属于"等用户授权"，不是"被信任度封顶拒绝"，界面提示要能区分
        assert!(set.denied_by_ceiling().is_empty());
        assert!(set.require(Permission::NetConnectAny).is_err());
    }

    #[test]
    fn minimal_set_is_auto_granted_even_for_unsigned() {
        let declared = [Permission::NetListenLocal, Permission::FsPluginData];
        let set = PermissionSet::resolve(&declared, TrustLevel::Unsigned, None);
        assert!(set.contains(Permission::NetListenLocal));
        assert!(set.contains(Permission::FsPluginData));
    }

    #[test]
    fn verified_plugin_follows_its_manifest_without_user_decision() {
        // 已验签插件的清单来自可信发布者，用户未决策时按声明放行
        let set = PermissionSet::resolve(RISKY, TrustLevel::Verified, None);
        assert!(set.contains(Permission::NetConnectAny));
        assert!(set.contains(Permission::FsReadGameDirs));
    }

    #[test]
    fn user_can_revoke_everything() {
        let set = PermissionSet::resolve(RISKY, TrustLevel::Verified, Some(&[]));
        assert!(set.allowed().is_empty(), "用户全部撤销后不得有任何生效权限");
    }

    #[test]
    fn user_grant_cannot_exceed_ceiling() {
        let granted = [Permission::NetListenPublic];
        let set = PermissionSet::resolve(&granted, TrustLevel::Unsigned, Some(&granted));
        assert!(!set.contains(Permission::NetListenPublic));
        assert_eq!(set.denied_by_ceiling(), &[Permission::NetListenPublic]);
    }

    #[test]
    fn baseline_permission_per_kind() {
        assert_eq!(
            kind_baseline(PluginKind::Adapter),
            Permission::NetConnectAny
        );
        assert_eq!(kind_baseline(PluginKind::Detector), Permission::GameScan);
        assert_eq!(
            kind_baseline(PluginKind::Coupler),
            Permission::ProcSpawnGame
        );
    }

    #[test]
    fn user_grant_narrows_declaration() {
        let declared = [Permission::NetUdp, Permission::GameScan];
        let granted = [Permission::NetUdp];
        let set = PermissionSet::resolve(&declared, TrustLevel::Official, Some(&granted));
        assert!(set.contains(Permission::NetUdp));
        assert!(!set.contains(Permission::GameScan));
    }

    #[test]
    fn blocked_plugin_only_gets_local() {
        let declared = [Permission::NetUdp];
        let set = PermissionSet::resolve(&declared, TrustLevel::Blocked, None);
        assert!(set.allowed().is_empty());
    }
}
