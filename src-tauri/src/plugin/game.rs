//! 多游戏适配层：游戏画像与网络特征分类。
//!
//! 插件系统本身不理解"游戏"，它只理解**能力**（打洞、扫描、接入）。
//! 游戏画像把"某个具体游戏该怎么联机"这件事从代码里抽出来，变成数据：
//! 核心据此决定调用哪个插件、按什么顺序调用、失败后怎么降级。
//!
//! 新增一款游戏 = 新增一份画像（内置或用户放入 `Games/*.json`），
//! 不需要改动核心调度代码。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// 传输层协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Tcp,
    Udp,
    /// 同时使用 TCP 与 UDP（如 Minecraft Bedrock：UDP 游戏流量 + TCP 查询）。
    Both,
}

impl Transport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Transport::Tcp => "tcp",
            Transport::Udp => "udp",
            Transport::Both => "both",
        }
    }
}

/// 游戏的网络特征。决定联机策略与插件选型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkTrait {
    /// 局域网 UDP 广播/组播公布服务器（Minecraft Java：UDP 4445 组播）。
    LanBroadcast,
    /// 专有查询协议（Terraria / Steam A2S / Source Query）。
    QueryProtocol,
    /// 平台大厅制（Steam Lobby、Xbox Live），需要平台 SDK 才可加入。
    PlatformLobby,
    /// 纯直连：玩家直接输入 IP:端口。
    DirectConnect,
    /// 点对点优先，必须打洞（大量 P2P 独立游戏）。
    P2PPreferred,
    /// 只能经中继/专用服务器转发（如部分锁区或强 NAT 场景下的主机游戏）。
    RelayRequired,
}

/// 单款游戏的画像。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameProfile {
    /// 稳定 ID，kebab-case，如 `minecraft-java`。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 别名/常见叫法，用于模糊匹配。
    #[serde(default)]
    pub aliases: Vec<String>,
    /// 默认监听端口。
    #[serde(default)]
    pub default_port: Option<u16>,
    /// 传输层。
    pub transport: Transport,
    /// 网络特征集合。
    #[serde(default)]
    pub traits: Vec<NetworkTrait>,
    /// 用于检出游戏进程的可执行文件名（不含路径）。
    #[serde(default)]
    pub process_names: Vec<String>,
    /// 是否必须由耦合类插件接管才能完成联机。
    #[serde(default)]
    pub requires_coupler: bool,
    /// 首选适配器插件 ID；为空则按插件优先级路由。
    #[serde(default)]
    pub preferred_adapter: Option<String>,
    /// 回退适配器插件 ID 列表。
    #[serde(default)]
    pub fallback_adapters: Vec<String>,
    /// 该游戏的特殊说明，会随上下文下发给插件。
    #[serde(default)]
    pub notes: BTreeMap<String, String>,
}

impl GameProfile {
    /// 是否具备某项网络特征。
    pub fn has_trait(&self, t: NetworkTrait) -> bool {
        self.traits.contains(&t)
    }

    /// 该游戏是否需要 UDP 能力（用于适配器选型）。
    pub fn needs_udp(&self) -> bool {
        matches!(self.transport, Transport::Udp | Transport::Both)
    }
}

/// 游戏画像仓库。
#[derive(Debug, Default)]
pub struct GameRegistry {
    profiles: BTreeMap<String, GameProfile>,
}

impl GameRegistry {
    /// 创建并载入内置画像。
    pub fn with_builtins() -> Self {
        let mut reg = Self::default();
        for p in builtin_profiles() {
            reg.profiles.insert(p.id.clone(), p);
        }
        reg
    }

    /// 载入用户自定义画像（`<data_dir>/Games/*.json`）。
    ///
    /// 单个文件解析失败只跳过该文件并返回告警，不影响核心启动。
    pub fn load_dir(&mut self, dir: &Path) -> Vec<String> {
        let mut warnings = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return warnings;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<GameProfile>(&t).map_err(|e| e.to_string()))
            {
                Ok(p) if !p.id.trim().is_empty() => {
                    self.profiles.insert(p.id.clone(), p);
                }
                Ok(_) => warnings.push(format!("游戏画像 {} 缺少 id，已跳过", path.display())),
                Err(e) => warnings.push(format!("游戏画像 {} 解析失败: {}", path.display(), e)),
            }
        }
        warnings
    }

    pub fn get(&self, id: &str) -> Option<&GameProfile> {
        self.profiles.get(id)
    }

    /// 按 ID 或别名查找（大小写不敏感）。
    pub fn find(&self, key: &str) -> Option<&GameProfile> {
        let k = key.trim().to_lowercase();
        if let Some(p) = self.profiles.get(&k) {
            return Some(p);
        }
        self.profiles
            .values()
            .find(|p| p.name.to_lowercase() == k || p.aliases.iter().any(|a| a.to_lowercase() == k))
    }

    pub fn all(&self) -> Vec<&GameProfile> {
        self.profiles.values().collect()
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

/// 内置游戏画像。
///
/// 选取的是"网络行为差异足够大"的几类代表，用来验证插件架构的多游戏可扩展性：
/// 局域网广播（MC Java）、UDP 直连（MC Bedrock）、查询协议（Terraria）、
/// 纯直连（无中心服务的独立游戏）、平台大厅（Steam 系）。
pub fn builtin_profiles() -> Vec<GameProfile> {
    fn profile(
        id: &str,
        name: &str,
        transport: Transport,
        traits: &[NetworkTrait],
        port: Option<u16>,
        processes: &[&str],
        requires_coupler: bool,
    ) -> GameProfile {
        GameProfile {
            id: id.to_string(),
            name: name.to_string(),
            aliases: Vec::new(),
            default_port: port,
            transport,
            traits: traits.to_vec(),
            process_names: processes.iter().map(|s| s.to_string()).collect(),
            requires_coupler,
            preferred_adapter: None,
            fallback_adapters: Vec::new(),
            notes: BTreeMap::new(),
        }
    }

    vec![
        profile(
            "minecraft-java",
            "Minecraft Java Edition",
            Transport::Tcp,
            &[NetworkTrait::LanBroadcast, NetworkTrait::DirectConnect],
            Some(25565),
            &["javaw.exe", "java.exe", "java"],
            true,
        ),
        profile(
            "minecraft-bedrock",
            "Minecraft Bedrock Edition",
            Transport::Udp,
            &[NetworkTrait::LanBroadcast, NetworkTrait::P2PPreferred],
            Some(19132),
            &["Minecraft.Windows.exe", "Minecraft.Win10.exe"],
            true,
        ),
        profile(
            "terraria",
            "Terraria",
            Transport::Tcp,
            &[NetworkTrait::DirectConnect, NetworkTrait::QueryProtocol],
            Some(7777),
            &["Terraria.exe", "TerrariaServer.exe"],
            false,
        ),
        profile(
            "stardew-valley",
            "Stardew Valley",
            Transport::Udp,
            &[NetworkTrait::P2PPreferred, NetworkTrait::DirectConnect],
            Some(24642),
            &["Stardew Valley.exe", "StardewValley.exe"],
            false,
        ),
        profile(
            "factorio",
            "Factorio",
            Transport::Udp,
            &[NetworkTrait::DirectConnect, NetworkTrait::LanBroadcast],
            Some(34197),
            &["factorio.exe"],
            false,
        ),
        profile(
            "steam-remote-play",
            "Steam Remote Play 类平台大厅",
            Transport::Both,
            &[NetworkTrait::PlatformLobby, NetworkTrait::RelayRequired],
            None,
            &["steam.exe"],
            true,
        ),
        profile(
            "generic-tcp",
            "通用 TCP 联机",
            Transport::Tcp,
            &[NetworkTrait::DirectConnect],
            None,
            &[],
            false,
        ),
        profile(
            "generic-udp",
            "通用 UDP 联机",
            Transport::Udp,
            &[NetworkTrait::P2PPreferred],
            None,
            &[],
            false,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_is_populated() {
        let reg = GameRegistry::with_builtins();
        assert!(reg.len() >= 8);
        assert!(reg.get("minecraft-java").is_some());
    }

    #[test]
    fn find_matches_name_case_insensitively() {
        let reg = GameRegistry::with_builtins();
        assert!(reg.find("Terraria").is_some());
        assert!(reg.find("Minecraft Java Edition").is_some());
        assert!(reg.find("不存在的游戏").is_none());
    }

    #[test]
    fn bedrock_needs_udp() {
        let reg = GameRegistry::with_builtins();
        let p = reg.get("minecraft-bedrock").unwrap();
        assert!(p.needs_udp());
        assert!(p.has_trait(NetworkTrait::P2PPreferred));
    }

    #[test]
    fn java_needs_coupler() {
        let reg = GameRegistry::with_builtins();
        assert!(reg.get("minecraft-java").unwrap().requires_coupler);
        assert!(!reg.get("terraria").unwrap().requires_coupler);
    }
}
