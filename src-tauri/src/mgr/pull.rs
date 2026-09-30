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

    pub async fn page_manifest(&self) -> Result<crate::page::PageManifest, String> {
        let _guard = self.mgr.pull_lock.lock().await;
        crate::page::pull::fetch_page_manifest(self.mgr.data_dir(), self.mgr.http()).await
    }

    pub async fn page_content(&self, name: &str) -> Result<String, String> {
        let _guard = self.mgr.pull_lock.lock().await;
        crate::page::pull::fetch_page_content(self.mgr.data_dir(), name, self.mgr.http()).await
    }

    pub async fn check_update(&self) -> Result<crate::update::CheckUpdateResult, String> {
        let _guard = self.mgr.pull_lock.lock().await;
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

    pub async fn download_update(
        &self,
        asset: crate::update::UpdateAsset,
        on_progress: impl FnMut(u64, u64) + Send,
    ) -> Result<crate::update::DownloadUpdateResult, String> {
        let _guard = self.mgr.pull_lock.lock().await;
        crate::update::download_asset(self.mgr.data_dir(), self.mgr.http(), &asset, on_progress)
            .await
    }
}
