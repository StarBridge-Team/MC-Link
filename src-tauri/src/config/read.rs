use std::path::Path;
use super::PersonalizationSettings;
use crate::datadir::setting_dir;

pub(crate) fn read_setting(data_dir: &Path, section: &str) -> Result<String, String> {
    let section_file = section
        .replace(' ', "_")
        .replace('/', "_")
        .replace('\\', "_");
    let path = setting_dir(data_dir)?.join(format!("{}.yml", section_file));
    if !path.exists() {
        return Ok(format!("# {}\n", section));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取设置文件失败: {}", e))
}

pub(crate) fn read_personalization(data_dir: &Path) -> Result<PersonalizationSettings, String> {
    let path = personalization_path(data_dir)?;
    if !path.exists() {
        return Ok(PersonalizationSettings::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取个性化设置失败: {}", e))?;
    serde_yaml::from_str(&content)
        .map_err(|e| format!("解析个性化设置失败: {}", e))
}

pub(crate) fn personalization_path(data_dir: &Path) -> Result<std::path::PathBuf, String> {
    Ok(setting_dir(data_dir)?.join("personalization.yml"))
}
