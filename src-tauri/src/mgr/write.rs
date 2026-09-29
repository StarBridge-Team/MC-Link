use super::AppMgr;

pub struct WriteMgr<'a> {
    mgr: &'a AppMgr,
}

impl<'a> WriteMgr<'a> {
    pub fn new(mgr: &'a AppMgr) -> Self {
        Self { mgr }
    }

    pub fn setting(&self, section: &str, content: &str) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::config::write::write_setting(self.mgr.data_dir(), section, content)
    }

    pub fn personalization(
        &self,
        settings: crate::config::PersonalizationSettings,
    ) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::config::write::write_personalization(self.mgr.data_dir(), settings)
    }

    pub fn clear_page_cache(&self) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::page::clear_page_cache(self.mgr.data_dir())
    }

    pub fn clear_update_cache(&self) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::update::clear_update_cache(self.mgr.data_dir())
    }

    pub fn clear_setting_meta_cache(&self) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::setting_meta::clear_setting_meta_cache(self.mgr.data_dir())
    }
}
