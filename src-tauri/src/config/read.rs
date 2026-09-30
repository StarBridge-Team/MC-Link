use std::path::Path;
use super::PersonalizationSettings;
use crate::datadir::setting_dir;
use crate::persist;

pub(crate) fn read_setting(data_dir: &Path, section: &str) -> Result<String, String> {
    let path = super::section_path(data_dir, section)?;
    if !path.exists() {
        return Ok(format!("# {}\n", section));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取设置文件失败: {}", e))
}

pub(crate) fn read_personalization(data_dir: &Path) -> Result<PersonalizationSettings, String> {
    let path = personalization_path(data_dir)?;
    // 文件损坏时由 persist 隔离原文件并返回默认值，不再就地覆盖用户数据
    Ok(persist::load_yaml::<PersonalizationSettings>(&path)?.value)
}

pub(crate) fn personalization_path(data_dir: &Path) -> Result<std::path::PathBuf, String> {
    Ok(setting_dir(data_dir)?.join("personalization.yml"))
}
