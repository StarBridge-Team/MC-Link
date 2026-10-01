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

    /// 完成首次引导：保存语言与地区并标记已完成。
    pub fn complete_setup(&self, language: &str, region: &str) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::setup::save_selection(self.mgr.data_dir(), language, region, Some(true)).map(|_| ())
    }

    /// 修改语言/地区（保持"已完成"标记不变）。
    pub fn update_setup(&self, language: &str, region: &str) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::setup::save_selection(self.mgr.data_dir(), language, region, None).map(|_| ())
    }

    /// 重置引导状态，让下次启动重新走一遍 OOBE。
    pub fn reset_setup(&self) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::setup::reset(self.mgr.data_dir()).map(|_| ())
    }
}
