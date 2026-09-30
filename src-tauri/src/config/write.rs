use std::path::Path;
use super::PersonalizationSettings;
use crate::persist;

pub(crate) fn write_setting(
    data_dir: &Path,
    section: &str,
    content: &str,
) -> Result<(), String> {
    let path = super::section_path(data_dir, section)?;
    persist::atomic_write(&path, content.as_bytes())
}

pub(crate) fn write_personalization(
    data_dir: &Path,
    settings: PersonalizationSettings,
) -> Result<(), String> {
    let path = super::read::personalization_path(data_dir)?;
    persist::save_yaml(&path, &settings)
}
