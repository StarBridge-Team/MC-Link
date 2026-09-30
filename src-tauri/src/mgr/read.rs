use super::AppMgr;

pub struct ReadMgr<'a> {
    mgr: &'a AppMgr,
}

impl<'a> ReadMgr<'a> {
    pub fn new(mgr: &'a AppMgr) -> Self {
        Self { mgr }
    }

    pub fn setting(&self, section: &str) -> Result<String, String> {
        crate::config::read::read_setting(self.mgr.data_dir(), section)
    }

    pub fn personalization(&self) -> Result<crate::config::PersonalizationSettings, String> {
        crate::config::read::read_personalization(self.mgr.data_dir())
    }

    pub fn asset_path(&self, relative: &str) -> Result<String, String> {
        let path = crate::assets::get_asset_path(self.mgr.data_dir(), relative)?;
        Ok(path.to_string_lossy().to_string())
    }

    /// 引导状态（是否已完成 OOBE、生效与检测到的语言/地区）。
    pub fn setup_state(&self) -> Result<crate::setup::SetupState, String> {
        crate::setup::state(self.mgr.data_dir())
    }
}
