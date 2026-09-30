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

    /// 预共享密钥路径（内置插件不落盘）。
    pub fn secret_path(&self) -> PathBuf {
        self.dir.join("secret.key")
    }

    /// 读取或生成预共享密钥。
    ///
    /// 密钥只在首次登记时生成一次并写入插件目录；后续握手双方都用它做
    /// 挑战-应答，核心不会把密钥发送给插件（外部插件在安装时由安装器
    /// 写入同一份密钥）。
    pub fn ensure_secret(&self) -> Result<[u8; 32], String> {
        let path = self.secret_path();
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
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("创建插件目录失败: {}", e))?;
        std::fs::write(&path, hex::encode(&raw))
            .map_err(|e| format!("写入插件密钥失败: {}", e))?;
        restrict_permissions(&path);
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
    /// `None` 表示未做用户授权决策（按清单声明放行到信任度上限）。
    #[serde(default)]
    granted: Option<Vec<Permission>>,
    #[serde(default)]
    blocked: bool,
}

impl Default for RegistryEntryState {
    fn default() -> Self {
        Self {
            // 关键：默认必须是"启用"。
            //
            // `#[serde(default = "...")]` 只在反序列化缺字段时生效，`Default::default()`
            // 并不会走它。新安装的插件在 registry.json 里还没有条目，如果这里返回
            // `enabled: false`，插件会被静默禁用 —— 表现为"装了但从来不生效"。
            enabled: true,
            granted: None,
            blocked: false,
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
        std::fs::read_to_string(self.state_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn write_state(&self, state: &RegistryState) -> Result<(), String> {
        std::fs::create_dir_all(&self.plugins_dir)
            .map_err(|e| format!("创建插件目录失败: {}", e))?;
        let text = serde_json::to_string_pretty(state)
            .map_err(|e| format!("序列化注册表失败: {}", e))?;
        std::fs::write(self.state_path(), text).map_err(|e| format!("写入注册表失败: {}", e))
    }

    /// 扫描磁盘上的外部插件并合并内置插件。
    ///
    /// 返回非致命告警列表（坏清单会被跳过而不是让核心启动失败）。
    pub fn reload(&mut self, builtins: Vec<PluginRecord>) -> Vec<String> {
        let mut warnings = Vec::new();
        let state = self.read_state();
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
            let trust = if st.blocked {
                TrustLevel::Blocked
            } else {
                manifest.declared_trust()
            };
            let permissions =
                PermissionSet::resolve(&manifest.permissions, trust, st.granted.as_deref());
            let record = PluginRecord {
                manifest,
                source: PluginSource::External,
                dir,
                trust,
                enabled: st.enabled,
                permissions,
            };
            if self.records.contains_key(record.id()) {
                warnings.push(format!(
                    "插件 ID 冲突，磁盘版本已忽略: {}",
                    record.id()
                ));
                continue;
            }
            self.records.insert(record.id().to_string(), record);
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
        self.write_state(&state)?;
        if let Some(r) = self.records.get_mut(id) {
            r.enabled = enabled;
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
    pub fn set_blocked(&mut self, id: &str, blocked: bool) -> Result<(), String> {
        if !self.records.contains_key(id) {
            return Err(format!("插件不存在: {}", id));
        }
        let mut state = self.read_state();
        state.entries.entry(id.to_string()).or_default().blocked = blocked;
        self.write_state(&state)?;
        if let Some(r) = self.records.get_mut(id) {
            if blocked {
                r.trust = TrustLevel::Blocked;
            }
        }
        Ok(())
    }
}

/// Windows 上收紧密钥文件 ACL（尽力而为；失败不阻断流程）。
#[cfg(windows)]
fn restrict_permissions(path: &Path) {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    // 仅设置隐藏属性，避免依赖外部 crate 调整 DACL。
    if let Ok(file) = std::fs::OpenOptions::new()
        .write(true)
        .attributes(FILE_ATTRIBUTE_HIDDEN)
        .open(path)
    {
        drop(file);
    }
}

#[cfg(not(windows))]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
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
}
