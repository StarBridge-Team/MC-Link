use std::path::Path;
use super::CONFIG_VERSION;
use crate::datadir::setting_dir;

/// 读取本地记录的配置版本号，若不存在返回 0。
fn read_recorded_version(data_dir: &Path) -> u32 {
    let path = match setting_dir(data_dir) {
        Ok(d) => d.join(".version"),
        Err(_) => return 0,
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => s.trim().parse().unwrap_or(0),
        Err(_) => 0,
    }
}

/// 检查本地配置版本是否与当前版本一致。
///
/// 若版本低于 `CONFIG_VERSION`，会尝试执行迁移并写入新版本号。
/// 返回值表示配置是否可用（true = 版本一致或迁移成功）。
pub fn check_config_version(data_dir: &Path) -> Result<bool, String> {
    let recorded = read_recorded_version(data_dir);
    if recorded == CONFIG_VERSION {
        return Ok(true);
    }

    // 本地配置版本**高于**当前程序支持的版本：说明用户跑过更新的构建。
    // 这时既不能跑迁移（旧逻辑不知道新结构），更不能把版本号写小——
    // 写小会让下次启动新构建时把所有迁移重跑一遍，可能把配置改坏。
    if recorded > CONFIG_VERSION {
        eprintln!(
            "[配置] 本地配置版本 {} 高于当前程序支持的 {}，跳过迁移（请升级到较新的构建）",
            recorded, CONFIG_VERSION
        );
        return Ok(true);
    }

    run_migrations(data_dir, recorded)?;
    write_config_version(data_dir)?;
    Ok(true)
}

/// 写入当前配置版本号。
pub fn write_config_version(data_dir: &Path) -> Result<(), String> {
    let path = setting_dir(data_dir)?.join(".version");
    std::fs::write(&path, CONFIG_VERSION.to_string())
        .map_err(|e| format!("写入配置版本失败: {}", e))
}

/// 执行配置迁移。
fn run_migrations(data_dir: &Path, from: u32) -> Result<(), String> {
    if from == 0 {
        migrate_from_v0(data_dir)?;
    }
    // 未来版本在此处追加更多迁移分支
    Ok(())
}

/// 从 v0 迁移到 v1：
/// - 若存在旧版 personalization.yml 且无法解析，交由统一持久化层隔离为备份并重建默认值。
fn migrate_from_v0(data_dir: &Path) -> Result<(), String> {
    let path = setting_dir(data_dir)?.join("personalization.yml");
    if !path.exists() {
        return Ok(());
    }

    // 旧实现在此处直接把默认值写入原文件，等于静默清空用户配置且无法取证。
    // 现在：损坏文件被改名保留（`.corrupt-<时间戳>`），再原子写入一份默认配置。
    let loaded = crate::persist::load_yaml::<super::PersonalizationSettings>(&path)?;
    if loaded.recovered {
        let default = super::PersonalizationSettings::default();
        crate::persist::save_yaml(&path, &default)?;
        eprintln!(
            "[配置迁移] 旧版个性化配置损坏，原文件已隔离为 {}，已重建默认配置",
            loaded
                .backup
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "（隔离失败，原文件仍保留在原处）".to_string())
        );
    }

    Ok(())
}
