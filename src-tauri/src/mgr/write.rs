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


    pub fn clear_setting_meta_cache(&self) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::setting_meta::clear_setting_meta_cache(self.mgr.data_dir())
    }

    /// 清理社区信息缓存（GitHub 贡献者与 Issues），是"重新拉取"的入口。
    pub fn clear_community_cache(&self) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::community::clear_cache(self.mgr.data_dir())
    }

    /// 完成首次引导：保存语言与地区并标记已完成。
    pub fn complete_setup(&self, language: &str, region: &str) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        // 闸门在 setup::complete 里：语言 + 已同意 EULA + 已选首个游戏，三者齐备才允许完成
        crate::setup::complete(self.mgr.data_dir(), language, region).map(|_| ())
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

    /// 记录 EULA 同意。
    ///
    /// 时间戳由**后端**生成，不接受调用方传入——否则"什么时候同意的"就成了
    /// 前端说了算的东西，这条记录也就不再有审计价值。
    pub fn accept_eula(&self, version: &str, sha256: &str) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        let stamp = crate::setup::now_rfc3339();
        crate::setup::accept_eula(self.mgr.data_dir(), version, sha256, &stamp).map(|_| ())
    }

    /// 记录用户选择的第一个游戏。
    ///
    /// 合法性（该 id 是否真实存在）由命令层对着插件系统校验，这里只负责落盘。
    pub fn set_game(&self, game: &str) -> Result<(), String> {
        let _guard = self
            .mgr
            .write_lock
            .lock()
            .map_err(|e| format!("获取写锁失败: {}", e))?;
        crate::setup::set_game(self.mgr.data_dir(), game).map(|_| ())
    }
}
