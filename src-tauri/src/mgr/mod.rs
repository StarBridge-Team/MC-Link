pub mod read;
pub mod write;
pub mod pull;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as SyncMutex;
use tokio::sync::Mutex as AsyncMutex;
use std::time::Duration;

/// 应用全局管理器。
///
/// 统一持有数据目录、共享 HTTP 客户端，并对写操作与网络拉取操作加锁，
/// 避免并发读写同一文件或重复下载相同资源。
pub struct AppMgr {
    data_dir: PathBuf,
    http: reqwest::Client,
    write_lock: Arc<SyncMutex<()>>,
    pull_lock: Arc<AsyncMutex<()>>,
}

impl AppMgr {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
        Ok(Self {
            data_dir,
            http,
            write_lock: Arc::new(SyncMutex::new(())),
            pull_lock: Arc::new(AsyncMutex::new(())),
        })
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn read(&self) -> read::ReadMgr<'_> {
        read::ReadMgr::new(self)
    }

    pub fn write(&self) -> write::WriteMgr<'_> {
        write::WriteMgr::new(self)
    }

    pub fn pull(&self) -> pull::PullMgr<'_> {
        pull::PullMgr::new(self)
    }
}
