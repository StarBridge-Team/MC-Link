use std::path::Path;
use super::PersonalizationSettings;
use crate::datadir::setting_dir;

pub(crate) fn write_setting(
    data_dir: &Path,
    section: &str,
    content: &str,
) -> Result<(), String> {
    let section_file = section
        .replace(' ', "_")
        .replace('/', "_")
        .replace('\\', "_");
    let path = setting_dir(data_dir)?.join(format!("{}.yml", section_file));
    std::fs::write(&path, content).map_err(|e| format!("保存设置文件失败: {}", e))
}

pub(crate) fn write_personalization(
    data_dir: &Path,
    settings: PersonalizationSettings,
) -> Result<(), String> {
    let content = serde_yaml::to_string(&settings)
        .map_err(|e| format!("序列化个性化设置失败: {}", e))?;
    let path = super::read::personalization_path(data_dir)?;
    std::fs::write(&path, &content)
        .map_err(|e| format!("保存个性化设置失败: {}", e))
}
