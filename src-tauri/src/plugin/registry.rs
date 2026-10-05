//! 插件注册表：发现、登记、信任判定与预共享密钥管理。
//!
//! 目录约定：
//!
//! ```text
//! <data_dir>/Plugins/
//! ├── registry.json            # 启用状态与用户授权（由核心维护）
//! ├── dev.mclink.terracotta/   # 内置插件的数据目录（无清单文件）
//! └── dev.example.adapter/
//!     ├── plugin.json          # 清单
//!     ├── secret.key           # 预共享密钥（hex，0600）
//!     └── bin/adapter.exe
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::plugin::crypto;
use crate::plugin::manifest::{PluginKind, PluginManifest};
use crate::plugin::permission::{Permission, PermissionSet, TrustLevel};

/// 插件来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSource {
    /// 编译进核心的官方实现，不经 WebSocket。
    Builtin,
    /// 从磁盘加载的第三方插件。
    External,
}

/// 注册表条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRecord {
    pub manifest: PluginManifest,
    pub source: PluginSource,
    /// 插件目录（内置插件指向其数据目录）。
    pub dir: PathBuf,
    /// 当前生效的信任度（由签名校验 + 用户拉黑决定）。
    pub trust: TrustLevel,
    /// 是否启用。
    pub enabled: bool,
    /// 生效权限。
    pub permissions: PermissionSet,
}

impl PluginRecord {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn kind(&self) -> PluginKind {
        self.manifest.kind
    }

    /// 预共享密钥路径。内置插件不落盘。
    ///
    /// # 为什么不放在插件目录里
    ///
    /// PSK 是握手双方信任根（`auth.rs` 的 HMAC 只依赖它）。放在
    /// `Plugins/<id>/secret.key` 时，任何能写插件目录的进程（插件自身、
    /// 安装器、其它本地进程）都能替换它，从而**冒充该插件**取得其已授权权限集。
    /// Windows 上 `restrict_to_current_user` 的 `icacls` 又是尽力而为，
    /// 因此改为落在核心独占的 `Setting/plugin_keys/` 下。
    ///
    /// 文件名用插件 ID 的哈希而非直出 ID：ID 里可能含 `.`/`-` 之外被
    /// sanitize 掉的字符，同名插件不能互相覆盖。
    pub fn secret_path(&self, data_dir: &Path) -> PathBuf {
        use sha2::Digest as _;
        let key = hex::encode(sha2::Sha256::digest(self.id().as_bytes()));
        data_dir
            .join("Setting")
            .join("plugin_keys")
            .join(format!("{}.key", &key[..32]))
    }

    /// 读取或生成预共享密钥。
    ///
    /// 密钥只在首次登记时生成一次；后续握手双方都用它做挑战-应答，
    /// 核心不会把密钥发送给插件（外部插件在安装时由安装器写入同一份密钥）。
    pub fn ensure_secret(&self, data_dir: &Path) -> Result<[u8; 32], String> {
        let path = self.secret_path(data_dir);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(bytes) = hex::decode(text.trim()) {
                if bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&bytes);
                    return Ok(key);
                }
            }
        }
        let raw = crypto::random_bytes(32);
        // 原子写入：插件密钥被截断会导致会话密钥派生失效
        crate::persist::atomic_write(&path, hex::encode(&raw).as_bytes())?;
        // 收紧失败必须上抛：密钥文件是信任根，"只剩隐藏属性"等于没有保护
        crate::plugin::fs_secure::restrict_to_current_user(&path)
            .map_err(|e| format!("收紧插件密钥文件权限失败: {}", e))?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw);
        Ok(key)
    }
}

/// 注册表持久化状态。
#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryState {
    #[serde(default)]
    entries: BTreeMap<String, RegistryEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryEntryState {
    #[serde(default = "default_true")]
    enabled: bool,
    /// 用户是否**显式**启用/停用过该插件（由 `set_enabled` 置位）。
    ///
    /// 未验签插件必须由用户表过态才能运行：`enabled` 的默认值是 true（新装即启用），
    /// 只看它的话，"往插件目录丢一个目录"就等于"核心会执行它"。
    /// 见 [`PluginRegistry::reload`] 里的生效条件。
    #[serde(default)]
    enabled_by_user: bool,
    /// `None` 表示未做用户授权决策，此时只放行自动放行集（见
    /// [`crate::plugin::permission::PermissionSet::resolve`]）。
    #[serde(default)]
    granted: Option<Vec<Permission>>,
    #[serde(default)]
    blocked: bool,
    /// 安装时是否通过**包验签**（见 `crate::plugin::trust`）。
    ///
    /// 这是外部插件提权到 `TrustLevel::Verified` 的唯一依据。它只能由安装器写入，
    /// 插件自身（清单、运行时行为）无法影响它——否则又回到"自述即可信"。
    #[serde(default)]
    verified: bool,
}

impl Default for RegistryEntryState {
    fn default() -> Self {
        Self {
            // 新装的插件在 registry.json 里还没有条目，这里若是 `enabled: false`
            // 会表现为"装了但从来不生效"（`#[serde(default)]` 的规则决定了
            // `Default::default()` 与字段级 default 是两条路径）。
            //
            // 但**真正决定能不能跑的是下面两项**：`enabled_by_user` 或验签通过。
            // 单看 `enabled` 会让"往插件目录丢一个目录"变成"核心直接执行它"。
            enabled: true,
            enabled_by_user: false,
            granted: None,
            blocked: false,
            // 默认未验签：只有目录级验签通过（或用户显式启用）才拿得到更多权限。
            verified: false,
        }
    }
}

fn default_true() -> bool {
    true
}

/// 插件注册表。
pub struct PluginRegistry {
    plugins_dir: PathBuf,
    records: BTreeMap<String, PluginRecord>,
}

impl PluginRegistry {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            plugins_dir: data_dir.join("Plugins"),
            records: BTreeMap::new(),
        }
    }

    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    fn state_path(&self) -> PathBuf {
        self.plugins_dir.join("registry.json")
    }

    fn read_state(&self) -> RegistryState {
        crate::persist::load_json::<RegistryState>(&self.state_path())
            .map(|loaded| loaded.value)
            .unwrap_or_default()
    }

    fn write_state(&self, state: &RegistryState) -> Result<(), String> {
        crate::persist::save_json(&self.state_path(), state)
    }

    /// 扫描磁盘上的外部插件并合并内置插件。
    ///
    /// 返回非致命告警列表（坏清单会被跳过而不是让核心启动失败）。
    pub fn reload(&mut self, builtins: Vec<PluginRecord>) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut state = self.read_state();
        let mut state_dirty = false;
        self.records.clear();

        for mut record in builtins {
            let entry = state.entries.get(record.id()).cloned().unwrap_or_default();
            record.enabled = entry.enabled;
            record.trust = if entry.blocked {
                TrustLevel::Blocked
            } else {
                record.trust
            };
            record.permissions = PermissionSet::resolve(
                &record.manifest.permissions,
                record.trust,
                entry.granted.as_deref(),
            );
            self.records.insert(record.id().to_string(), record);
        }

        let Ok(entries) = std::fs::read_dir(&self.plugins_dir) else {
            return warnings;
        };

        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() || !dir.join(crate::plugin::manifest::MANIFEST_FILE).is_file() {
                continue;
            }
            let manifest = match PluginManifest::load(&dir) {
                Ok(m) => m,
                Err(e) => {
                    warnings.push(format!("跳过插件 {}: {}", dir.display(), e));
                    continue;
                }
            };
            let st = state.entries.get(&manifest.id).cloned().unwrap_or_default();

            // 目录级验签。刻意**每次扫描都做**，而不是只信持久化的 `verified` 标志：
            // 标志只能证明"装进来的时候是好的"，证明不了"现在还是好的"。
            let verdict = crate::plugin::trust::verify_directory(&dir);
            if let crate::plugin::trust::Verdict::Invalid(reason) = &verdict {
                // trust.rs 的约定：校验不通过必须拒绝，不允许降级成"未验签"
                warnings.push(format!(
                    "插件 {} 验签未通过，已忽略: {}",
                    manifest.id, reason
                ));
                continue;
            }
            let verified = verdict.grants_verified();
            if verified != st.verified {
                state
                    .entries
                    .entry(manifest.id.clone())
                    .or_default()
                    .verified = verified;
                state_dirty = true;
            }

            let trust = if st.blocked {
                TrustLevel::Blocked
            } else if verified {
                TrustLevel::Verified
            } else {
                // 外部插件的信任等级**只能由验签结果决定**，清单自述不参与
                TrustLevel::Unsigned
            };
            let permissions =
                PermissionSet::resolve(&manifest.permissions, trust, st.granted.as_deref());

            // 未验签的插件必须由用户**显式启用**才运行。
            // 否则"能往插件目录写一个目录"就等于"核心会拉起并执行它"，
            // 而这条路径不需要任何签名。
            let enabled = st.enabled && (verified || st.enabled_by_user);
            if st.enabled && !enabled {
                // 必须说清"为什么装了却不生效"，否则会被当成 bug（或干脆被忽略）
                warnings.push(format!(
                    "插件 {} 未通过验签，保持停用；如需使用请在插件管理中显式启用（只会获得最小权限）",
                    manifest.id
                ));
            }

            let record = PluginRecord {
                manifest,
                source: PluginSource::External,
                dir,
                trust,
                enabled,
                permissions,
            };
            if self.records.contains_key(record.id()) {
                warnings.push(format!("插件 ID 冲突，磁盘版本已忽略: {}", record.id()));
                continue;
            }
            self.records.insert(record.id().to_string(), record);
        }

        if state_dirty {
            if let Err(e) = self.write_state(&state) {
                warnings.push(format!("写入插件注册表失败: {}", e));
            }
        }

        warnings
    }

    pub fn get(&self, id: &str) -> Option<&PluginRecord> {
        self.records.get(id)
    }

    pub fn list(&self) -> Vec<&PluginRecord> {
        self.records.values().collect()
    }

    pub fn by_kind(&self, kind: PluginKind) -> Vec<&PluginRecord> {
        self.records
            .values()
            .filter(|r| r.manifest.kind == kind && r.enabled && r.trust.is_runnable())
            .collect()
    }

    /// 启用/停用插件。
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        if !self.records.contains_key(id) {
            return Err(format!("插件不存在: {}", id));
        }
        let mut state = self.read_state();
        let entry = state.entries.entry(id.to_string()).or_default();
        entry.enabled = enabled;
        // 记录"用户明确表过态"：未验签插件只有置位后才能运行（见 reload）
        entry.enabled_by_user = enabled;
        self.write_state(&state)?;
        if let Some(r) = self.records.get_mut(id) {
            r.enabled = enabled;
        }
        Ok(())
    }

    /// 记录安装时的验签结果，并据此重算生效权限。
    ///
    /// **只能由安装器调用**：这是外部插件提权到 [`TrustLevel::Verified`] 的唯一入口。
    /// 它不接收任何来自插件的内容，只接收安装器对整包验签的结论，
    /// 因此插件无法通过清单或运行时行为影响这个结果。
    pub fn set_verified(&mut self, id: &str, verified: bool) -> Result<(), String> {
        let Some(record) = self.records.get(id) else {
            return Err(format!("插件不存在: {}", id));
        };
        if record.source != PluginSource::External {
            return Err(format!("{} 是内置插件，不参与验签", id));
        }
        let manifest = record.manifest.clone();

        let mut state = self.read_state();
        // 注意取的是**持久化的用户授权**，不是"当前生效权限"。
        // 后者已被信任度上限裁剪过一遍，拿它当授权集会让权限越收越窄：
        // 首次以未验签身份安装时被裁掉的项，之后即使验签通过也回不来。
        let granted = {
            let entry = state.entries.entry(id.to_string()).or_default();
            entry.verified = verified;
            entry.granted.clone()
        };
        self.write_state(&state)?;

        let trust = if verified {
            TrustLevel::Verified
        } else {
            TrustLevel::Unsigned
        };
        let permissions = PermissionSet::resolve(&manifest.permissions, trust, granted.as_deref());
        if let Some(r) = self.records.get_mut(id) {
            r.trust = trust;
            r.permissions = permissions;
        }
        Ok(())
    }

    /// 写入用户授权集合并重算生效权限。
    pub fn set_grants(&mut self, id: &str, granted: Option<Vec<Permission>>) -> Result<(), String> {
        let Some(record) = self.records.get(id) else {
            return Err(format!("插件不存在: {}", id));
        };
        let manifest = record.manifest.clone();
        let trust = record.trust;

        let mut state = self.read_state();
        state.entries.entry(id.to_string()).or_default().granted = granted.clone();
        self.write_state(&state)?;

        let permissions = PermissionSet::resolve(&manifest.permissions, trust, granted.as_deref());
        if let Some(r) = self.records.get_mut(id) {
            r.permissions = permissions;
        }
        Ok(())
    }

    /// 拉黑/解除拉黑插件。
    ///
    /// 解除拉黑时必须**重算**信任度与权限：`set_blocked(true)` 把 `trust` 改成了
    /// `Blocked`（并据 ceiling 裁过权限），只清 `blocked` 标志的话该插件会停在一个
    /// 降级状态——插件出现在路由候选里（`is_runnable()` 为 true）却拿不到基线权限，
    /// 表现为"解封了但功能诡异失效"，必须重启或 reload 才能恢复。
    pub fn set_blocked(&mut self, id: &str, blocked: bool) -> Result<(), String> {
        let Some(existing) = self.records.get(id) else {
            return Err(format!("插件不存在: {}", id));
        };
        // 解除拉黑要恢复的信任度：外部插件由验签结果（持久化的 `verified`）决定，
        // 内置插件保持其官方身份。
        let restored_trust = match existing.source {
            PluginSource::Builtin => TrustLevel::Official,
            PluginSource::External => {
                let verified = self
                    .read_state()
                    .entries
                    .get(id)
                    .map(|e| e.verified)
                    .unwrap_or(false);
                if verified {
                    TrustLevel::Verified
                } else {
                    TrustLevel::Unsigned
                }
            }
        };
        let manifest_permissions = existing.manifest.permissions.clone();

        let mut state = self.read_state();
        let entry = state.entries.entry(id.to_string()).or_default();
        entry.blocked = blocked;
        let granted = entry.granted.clone();
        let enabled = entry.enabled;
        // 未验签的外部插件仍需"用户显式启用"才运行（与 reload 的生效条件一致）
        let enabled_by_user = entry.enabled_by_user;
        self.write_state(&state)?;

        if let Some(r) = self.records.get_mut(id) {
            if blocked {
                r.trust = TrustLevel::Blocked;
                r.permissions = PermissionSet::resolve(&manifest_permissions, r.trust, granted.as_deref());
            } else {
                r.trust = restored_trust;
                r.permissions =
                    PermissionSet::resolve(&manifest_permissions, r.trust, granted.as_deref());
                if r.source == PluginSource::External {
                    r.enabled = enabled && (restored_trust == TrustLevel::Verified || enabled_by_user);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_registers_builtin() {
        let tmp = std::env::temp_dir().join(format!("mclink-reg-{}", crypto::random_hex(6)));
        let mut reg = PluginRegistry::new(&tmp);
        let manifest = PluginManifest::parse(
            &serde_json::json!({
                "id": "dev.mclink.builtin.test",
                "name": "内置测试",
                "version": "0.1.0",
                "kind": "adapter",
                "permissions": ["net_listen_local"]
            })
            .to_string(),
        )
        .unwrap();
        let builtin = PluginRecord {
            manifest,
            source: PluginSource::Builtin,
            dir: tmp.join("dev.mclink.builtin.test"),
            trust: TrustLevel::Official,
            enabled: true,
            permissions: PermissionSet::resolve(
                &[Permission::NetListenLocal],
                TrustLevel::Official,
                None,
            ),
        };
        let warnings = reg.reload(vec![builtin]);
        assert!(warnings.is_empty());
        assert_eq!(reg.list().len(), 1);
        assert!(reg.by_kind(PluginKind::Adapter).len() == 1);
        reg.set_enabled("dev.mclink.builtin.test", false).unwrap();
        assert!(reg.by_kind(PluginKind::Adapter).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 未验签的外部插件默认**不启用**：往插件目录丢一个目录不再等于"核心会执行它"。
    /// 用户显式启用后才生效，且只拿最小权限。
    #[test]
    fn unverified_external_plugin_stays_disabled_until_user_enables_it() {
        let tmp = std::env::temp_dir().join(format!("mclink-reg-ext-{}", crypto::random_hex(6)));
        let dir = tmp.join("Plugins").join("dev.example.adapter");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(crate::plugin::manifest::MANIFEST_FILE),
            serde_json::json!({
                "id": "dev.example.adapter",
                "name": "示例适配器",
                "version": "1.0.0",
                "kind": "adapter",
                "permissions": ["net_connect_any", "net_listen_local"]
            })
            .to_string(),
        )
        .unwrap();

        let mut reg = PluginRegistry::new(&tmp);
        let warnings = reg.reload(Vec::new());
        assert!(
            warnings.iter().any(|w| w.contains("未通过验签")),
            "应给出可解释的告警，实际: {:?}",
            warnings
        );

        let record = reg.get("dev.example.adapter").expect("目录应被登记");
        assert!(!record.enabled, "未验签插件默认不启用");
        assert_eq!(record.trust, TrustLevel::Unsigned);
        // 未授权时只拿"最小集 ∩ 清单声明"：声明的 net_connect_any 不在其中
        assert!(!record.permissions.contains(Permission::NetConnectAny));
        assert!(record.permissions.contains(Permission::NetListenLocal));
        // 清单没声明的项即使属于最小集也不会生效（生效权限始终是交集）
        assert!(!record.permissions.contains(Permission::FsPluginData));

        // 用户显式启用后才运行
        reg.set_enabled("dev.example.adapter", true).unwrap();
        assert!(reg.get("dev.example.adapter").unwrap().enabled);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
