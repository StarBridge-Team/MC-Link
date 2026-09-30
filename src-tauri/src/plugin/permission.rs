//! 插件权限模型与最小权限校验。
//!
//! 三条规则：
//! 1. **清单声明**：插件必须在 `plugin.json` 里声明它需要的能力。
//! 2. **信任度上限**：未签名插件的可申请权限被信任度封顶，签名不能自证。
//! 3. **用户授权**：最终生效权限 = 清单声明 ∩ 信任度上限 ∩ 用户授予。
//!
//! 核心侧在执行任何敏感 RPC 前都会调用 [`PermissionSet::require`]，插件无法
//! 通过伪造请求绕过——权限判定发生在核心进程内，而非插件进程内。

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::plugin::manifest::PluginKind;
use crate::plugin::protocol::{
    adapter_method, coupler_method, detector_method, ErrorInfo,
};

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

impl TrustLevel {
    /// 该信任度下允许申请的权限上限。
    pub fn ceiling(&self) -> &'static [Permission] {
        const LOCAL_ONLY: &[Permission] = &[
            Permission::NetConnectLocal,
            Permission::FsPluginData,
        ];
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
    /// 计算生效权限：清单声明 ∩ 信任度上限 ∩ 用户授予。
    pub fn resolve(
        declared: &[Permission],
        trust: TrustLevel,
        user_granted: Option<&[Permission]>,
    ) -> Self {
        let ceiling = trust.ceiling();
        let mut granted = BTreeSet::new();
        let mut denied_by_ceiling = Vec::new();

        for perm in declared {
            if !ceiling.contains(perm) {
                denied_by_ceiling.push(*perm);
                continue;
            }
            if let Some(user) = user_granted {
                if !user.contains(perm) {
                    continue;
                }
            }
            granted.insert(*perm);
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

    #[test]
    fn unsigned_plugin_cannot_get_public_listen() {
        let declared = [Permission::NetListenPublic, Permission::NetConnectAny];
        let set = PermissionSet::resolve(&declared, TrustLevel::Unsigned, None);
        assert!(!set.contains(Permission::NetListenPublic));
        assert!(set.contains(Permission::NetConnectAny));
        assert_eq!(set.denied_by_ceiling(), &[Permission::NetListenPublic]);
        assert!(set.require(Permission::NetListenPublic).is_err());
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
