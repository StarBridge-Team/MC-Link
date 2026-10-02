pub mod pull;
pub mod read;
pub mod write;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as SyncMutex;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;

/// 应用全局管理器。
///
/// 统一持有数据目录、共享 HTTP 客户端，并对写操作与网络拉取操作加锁，
/// 避免并发读写同一文件或重复下载相同资源。
///
/// # 为什么拉取锁有两把
///
/// `pull_lock` 是"快操作"（清单、页面、设置元，都是几 KB 的请求）；
/// `update_lock` 单独留给更新包下载——它要传十几 MB 甚至更多，
/// 若和快操作共用一把锁，用户点"下载更新"期间资产同步与页面加载会全部被堵住。
/// 两把锁覆盖的资源互不重叠，分开不会引入竞争。
pub struct AppMgr {
    data_dir: PathBuf,
    http: reqwest::Client,
    write_lock: Arc<SyncMutex<()>>,
    pull_lock: Arc<AsyncMutex<()>>,
    update_lock: Arc<AsyncMutex<()>>,
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
            update_lock: Arc::new(AsyncMutex::new(())),
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
