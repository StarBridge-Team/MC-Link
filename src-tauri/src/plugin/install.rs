//! 插件安装器：解包 → 清单校验 → 形态校验 → 验签 → 原子落盘。
//!
//! # 顺序为什么不能换
//!
//! 1. **先解包再验签**：签名覆盖的是目录内容（`integrity.txt` + 每个文件的哈希），
//!    必须先把目录摊开才能逐条核对；
//! 2. **验签在落盘之前**：一旦先放进 `Plugins/`，核心的目录扫描就可能先看到它——
//!    哪怕下一毫秒就删掉，也已经出现过一个"半安装"状态；
//! 3. **落盘用 rename**：`Plugins/<id>` 要么是旧的、要么是新的，不存在中间态。
//!
//! # 未签名包
//!
//! `allow_unsigned = false` 时直接拒绝；为 `true` 时按 `Unsigned` 安装并返回告警。
//! 调用方（命令层）负责让用户在界面上确认——这个参数就是"用户已二次确认"的凭证。

use std::path::Path;

use crate::plugin::manifest::PluginManifest;
use crate::plugin::permission::TrustLevel;
use crate::plugin::package;
use crate::plugin::trust::{self, Verdict};

/// 一次安装的结果。
#[derive(Debug, Clone)]
pub struct Installed {
    pub id: String,
    pub name: String,
    pub version: String,
    pub trust: TrustLevel,
    /// 非致命提示（如"未签名，已按未签名安装"）。
    pub warnings: Vec<String>,
    /// 解压后的实际体积（字节）。
    pub bytes: u64,
}

/// 临时目录名（放在 `Plugins/` 下，保证与目标同一卷，`rename` 才是原子的）。
const STAGING: &str = ".staging";

/// 安装一个本地插件包。
pub fn install_from_file(
    data_dir: &Path,
    package_path: &Path,
    allow_unsigned: bool,
) -> Result<Installed, String> {
    let meta = std::fs::metadata(package_path)
        .map_err(|e| format!("读取插件包失败 {}: {e}", package_path.display()))?;
    if !meta.is_file() {
        return Err("插件包路径不是文件".to_string());
    }
    if meta.len() > package::MAX_PACKAGE_BYTES {
        return Err(format!(
            "插件包 {:.1} MB 超过上限 {:.0} MB",
            meta.len() as f64 / 1024.0 / 1024.0,
            package::MAX_PACKAGE_BYTES as f64 / 1024.0 / 1024.0
        ));
    }

    let plugins_dir = data_dir.join("Plugins");
    let staging_root = plugins_dir.join(STAGING);
    std::fs::create_dir_all(&staging_root)
        .map_err(|e| format!("创建临时目录失败: {e}"))?;

    let work = staging_root.join(crate::plugin::crypto::random_hex(8));
    let result = install_into(data_dir, &plugins_dir, &work, package_path, allow_unsigned);
    // 无论成败都清掉临时目录：失败路径上留着半包，下次扫描会看到它
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn install_into(
    data_dir: &Path,
    plugins_dir: &Path,
    work: &Path,
    package_path: &Path,
    allow_unsigned: bool,
) -> Result<Installed, String> {
    // 解压到临时目录（不是最终位置）
    package::extract_tar_gz(package_path, work)?;

    let manifest = PluginManifest::load(work)
        .map_err(|e| format!("插件包清单无效: {e}"))?;
    let bytes = package::validate_layout(work, &manifest)?;

    // 验签：结论只来自目录内容与内置公钥，包本身说什么都不作数
    let mut warnings = Vec::new();
    let trust = match trust::verify_directory(work) {
        Verdict::Verified => TrustLevel::Verified,
        Verdict::Invalid(reason) => {
            return Err(format!("插件包验签未通过，已拒绝安装: {reason}"));
        }
        Verdict::NotConfigured => {
            if !allow_unsigned {
                return Err(
                    "插件包未签名（或缺少 integrity.txt / integrity.sig）。\
                     确认信任来源后可选择“仍要安装”，它会以未签名身份运行并只拿到最小权限"
                        .to_string(),
                );
            }
            warnings.push("插件未签名，已按未签名安装（只会获得最小权限）".to_string());
            TrustLevel::Unsigned
        }
    };

    let target = plugins_dir.join(&manifest.id);
    if target.exists() {
        // 覆盖安装：先把旧的挪走（同一个卷内 rename 是原子的）
        let backup = plugins_dir
            .join(STAGING)
            .join(format!("{}.old", manifest.id));
        let _ = std::fs::remove_dir_all(&backup);
        std::fs::rename(&target, &backup)
            .map_err(|e| format!("无法替换已安装的插件（{}）: {e}", manifest.id))?;
        if let Err(e) = std::fs::rename(work, &target) {
            // 回滚：宁可保持旧版本可用，也不要留下"装了半个"
            let _ = std::fs::rename(&backup, &target);
            return Err(format!("安装失败（已回滚到旧版本）: {e}"));
        }
        let _ = std::fs::remove_dir_all(&backup);
        warnings.push("已覆盖安装同 ID 的旧版本".to_string());
    } else {
        std::fs::rename(work, &target).map_err(|e| format!("安装插件失败: {e}"))?;
    }

    // 数据目录与安装目录分开（见 plugin_data）：这里只保证它存在
    let _ = crate::plugin::plugin_data::ensure(data_dir, &manifest.id);

    Ok(Installed {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        trust,
        warnings,
        bytes,
    })
}

/// 卸载插件目录与注册表条目。
///
/// 只删**核心自己管的东西**：安装目录、注册表状态、（可选）数据目录。
/// 不猜用户放在别处的文件。
pub fn uninstall(data_dir: &Path, plugin_id: &str, remove_data: bool) -> Result<(), String> {
    crate::plugin::manifest::validate_plugin_id(plugin_id)?;
    let target = data_dir.join("Plugins").join(plugin_id);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|e| format!("删除插件目录失败（{}）: {e}", target.display()))?;
    }
    if remove_data {
        let data = crate::plugin::plugin_data::plugin_data_dir(data_dir, plugin_id)?;
        if data.exists() {
            std::fs::remove_dir_all(&data)
                .map_err(|e| format!("删除插件数据失败（{}）: {e}", data.display()))?;
        }
    }
    // 会话文件也一并清掉：它是含 PSK 的临时产物，没有保留价值
    let _ = std::fs::remove_file(
        data_dir
            .join("Plugins")
            .join("runtime")
            .join(format!("{plugin_id}.session.json")),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mclink-install-{}",
            crate::plugin::crypto::random_hex(6)
        ))
    }

    /// 造一个最小的 tar.gz 包（带指定文件）。
    fn build_package(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, *bytes).unwrap();
        }
        let raw = tar.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        );
        std::io::Write::write_all(&mut gz, &raw).unwrap();
        gz.finish().unwrap()
    }

    const MANIFEST: &[u8] = br#"{"id":"dev.example.pack","name":"Pack Example","version":"1.2.3",
        "kind":"utility","permissions":["fs_plugin_data"],
        "runtime":{"kind":"process","entry":"bin/tool.exe"}}"#;

    #[test]
    fn rejects_unsigned_package_unless_allowed() {
        let root = temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        let pkg = root.join("p.tar.gz");
        std::fs::write(
            &pkg,
            build_package(&[("plugin.json", MANIFEST), ("bin/tool.exe", b"x")]),
        )
        .unwrap();

        let err = install_from_file(&root, &pkg, false).unwrap_err();
        assert!(err.contains("未签名"), "{err}");
        // 失败不留残留：Plugins/<id> 不应存在
        assert!(!root.join("Plugins").join("dev.example.pack").exists());

        let outcome = install_from_file(&root, &pkg, true).unwrap();
        assert_eq!(outcome.id, "dev.example.pack");
        assert_eq!(outcome.trust, TrustLevel::Unsigned);
        assert!(root.join("Plugins/dev.example.pack/plugin.json").is_file());
        // 数据目录同步建好
        assert!(root.join("PluginData/dev.example.pack").is_dir());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_package_with_wrong_entry() {
        let root = temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        let pkg = root.join("p.tar.gz");
        std::fs::write(&pkg, build_package(&[("plugin.json", MANIFEST)])).unwrap();
        let err = install_from_file(&root, &pkg, true).unwrap_err();
        assert!(err.contains("入口文件不存在"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uninstall_removes_install_dir_and_keeps_data_when_asked() {
        let root = temp_dir();
        let plugin_dir = root.join("Plugins/dev.example.pack");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.json"),
            br#"{"id":"dev.example.pack","name":"x","version":"1","kind":"utility"}"#,
        )
        .unwrap();
        let data = crate::plugin::plugin_data::ensure(&root, "dev.example.pack").unwrap();

        uninstall(&root, "dev.example.pack", false).unwrap();
        assert!(!plugin_dir.exists());
        assert!(data.is_dir(), "keep data 时必须保留 PluginData/");

        uninstall(&root, "dev.example.pack", true).unwrap();
        assert!(!data.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uninstall_rejects_bad_ids() {
        let root = temp_dir();
        assert!(uninstall(&root, "../etc", true).is_err());
    }
}
