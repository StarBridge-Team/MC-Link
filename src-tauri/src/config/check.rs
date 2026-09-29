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
/// - 若存在旧版 personalization.yml 且无法解析，则重置为默认配置。
fn migrate_from_v0(data_dir: &Path) -> Result<(), String> {
    let path = setting_dir(data_dir)?.join("personalization.yml");
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取旧版个性化配置失败: {}", e))?;

    if serde_yaml::from_str::<serde_yaml::Value>(&content).is_err() {
        let default = super::PersonalizationSettings::default();
        let yaml = serde_yaml::to_string(&default)
            .map_err(|e| format!("序列化默认配置失败: {}", e))?;
        std::fs::write(&path, yaml)
            .map_err(|e| format!("重置个性化配置失败: {}", e))?;
    }

    Ok(())
}
