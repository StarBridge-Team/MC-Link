use super::AppMgr;

pub struct PullMgr<'a> {
    mgr: &'a AppMgr,
}

impl<'a> PullMgr<'a> {
    pub fn new(mgr: &'a AppMgr) -> Self {
        Self { mgr }
    }

    /// 同步共享资产：检查远程 manifest 版本，不匹配则清理重下。
    pub async fn sync_assets(&self) -> Result<crate::assets::pull::SyncOutcome, String> {
        let _guard = self.mgr.pull_lock.lock().await;
        crate::assets::pull::sync_assets(self.mgr.data_dir(), self.mgr.http()).await
    }

    /// 检查更新。
    ///
    /// **不持锁**：它只发一个几 KB 的清单请求，既不写文件也不碰其它拉取的缓存，
    /// 加锁反而会让它被一次大下载挡住（用户点"检查更新"却一直转圈）。
    pub async fn check_update(&self) -> Result<crate::update::CheckUpdateResult, String> {
        crate::update::check_update(self.mgr.data_dir(), self.mgr.http()).await
    }

    pub async fn setting_meta(&self, section: &str) -> Result<crate::setting_meta::SettingMeta, String> {
        let _guard = self.mgr.pull_lock.lock().await;
        crate::setting_meta::fetch_setting_meta(self.mgr.data_dir(), section, self.mgr.http()).await
    }

    pub async fn setting_manifest(&self) -> Result<crate::setting_meta::SettingManifest, String> {
        let _guard = self.mgr.pull_lock.lock().await;
        crate::setting_meta::fetch_setting_manifest(self.mgr.data_dir(), self.mgr.http()).await
    }

    /// 拉取社区信息（GitHub 贡献者与 Issues）。
    ///
    /// **不持 `pull_lock`**：它访问的是 GitHub，和资产同步没有共享状态；共用一把锁
    /// 会让"关于"页在 8MB 适配器下载期间一直转圈（与 `check_update` 同样的取舍）。
    /// 它自己只写 `Cache/Community/` 下两份专属缓存，且是原子写。
    pub async fn community(&self) -> crate::community::Community {
        crate::community::fetch(self.mgr.data_dir(), self.mgr.http()).await
    }

    /// 下载更新包。
    ///
    /// 用**独立的 `update_lock`**：更新包动辄十几 MB，若与资产/页面/设置元共用
    /// `pull_lock`，下载期间那些拉取会被整体堵死（用户会感觉界面卡住）。
    pub async fn download_update(
        &self,
        asset: crate::update::UpdateAsset,
        on_progress: impl FnMut(u64, u64) + Send,
    ) -> Result<crate::update::DownloadUpdateResult, String> {
        let _guard = self.mgr.update_lock.lock().await;
        crate::update::download_asset(self.mgr.data_dir(), self.mgr.http(), &asset, on_progress)
            .await
    }

    /// 清理更新缓存。
    ///
    /// 与 [`Self::download_update`] 共用 `update_lock`：两者操作同一个
    /// `Cache/Updates` 目录，用不同的锁会让"边下载边清理"直接把正在写的分片删掉
    /// （下载失败，或留下半截文件）。此前它走的是 `write_lock`——那是设置类写入的锁，
    /// 与下载毫不互斥。
    pub async fn clear_update_cache(&self) -> Result<(), String> {
        let _guard = self.mgr.update_lock.lock().await;
        crate::update::clear_update_cache(self.mgr.data_dir())
    }
}
