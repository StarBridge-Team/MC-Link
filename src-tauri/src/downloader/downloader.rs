use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;

use super::chunk::{download_chunk, download_single, probe_range_support, split_chunks, Chunk};
use super::policy::Policy;
use super::verify;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// 断点续传进度。
#[derive(Serialize, Deserialize, Default, Debug)]
struct Progress {
    #[serde(default)]
    chunks: Vec<usize>,
}

/// 多线程下载器。
///
/// 支持分片下载、断点续传、哈希校验，以及不支持 Range 时的单线程回退。
#[derive(Clone, Debug)]
pub struct Downloader {
    client: Client,
    policy: Policy,
    timeout: Duration,
}

impl Downloader {
    /// 使用已有的 HTTP 客户端创建下载器。
    pub fn new(client: Client) -> Self {
        Self {
            client,
            policy: Policy::default(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// 设置下载策略。
    #[allow(dead_code)]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// 设置单次请求超时。
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 下载文件到目标路径。
    ///
    /// 若目标文件已存在且哈希校验通过，则直接跳过下载。
    /// 下载完成后会根据 `expected_hash` 进行 SHA-256 校验。
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        expected_hash: Option<&str>,
    ) -> Result<u64, String> {
        if let Some(hash) = expected_hash {
            if verify::verify_file(dest, hash).await.unwrap_or(false) {
                let meta = fs::metadata(dest)
                    .await
                    .map_err(|e| format!("获取文件元数据失败: {}", e))?;
                return Ok(meta.len());
            }
        }

        let part_path = part_file(dest);
        let progress_path = progress_file(&part_path);

        let (supports_range, total_size) = probe_range_support(&self.client, url, self.timeout)
            .await?;

        if supports_range && total_size.is_some() {
            let total = total_size.unwrap();
            self.download_chunked(url, &part_path, &progress_path, total)
                .await?;

            if let Some(hash) = expected_hash {
                if !verify::verify_file(&part_path, hash).await? {
                    let _ = fs::remove_file(&part_path).await;
                    let _ = fs::remove_file(&progress_path).await;
                    return Err("文件哈希校验失败".to_string());
                }
            }

            fs::rename(&part_path, dest)
                .await
                .map_err(|e| format!("重命名临时文件失败: {}", e))?;
            let _ = fs::remove_file(&progress_path).await;
            Ok(total)
        } else {
            let bytes = download_single(&self.client, url, &part_path, self.timeout).await?;

            if let Some(hash) = expected_hash {
                if !verify::verify_file(&part_path, hash).await? {
                    let _ = fs::remove_file(&part_path).await;
                    return Err("文件哈希校验失败".to_string());
                }
            }

            fs::rename(&part_path, dest)
                .await
                .map_err(|e| format!("重命名临时文件失败: {}", e))?;
            Ok(bytes)
        }
    }

    /// 分片下载逻辑，支持断点续传。
    async fn download_chunked(
        &self,
        url: &str,
        part_path: &Path,
        progress_path: &Path,
        total: u64,
    ) -> Result<(), String> {
        ensure_file_size(part_path, total).await?;

        let mut progress = load_progress(progress_path).await.unwrap_or_default();
        progress.chunks.sort_unstable();
        progress.chunks.dedup();
        let completed: HashSet<usize> = progress.chunks.iter().copied().collect();

        let threads = self.policy.decide_threads(total, None);
        let pending: Vec<Chunk> = split_chunks(total, self.policy.target_chunk_size)
            .into_iter()
            .filter(|c| !completed.contains(&c.index))
            .collect();

        if pending.is_empty() {
            return Ok(());
        }

        let progress = Arc::new(Mutex::new(progress));
        let sem = Arc::new(Semaphore::new(threads.max(1)));
        let mut set = JoinSet::new();

        let url = url.to_string();
        let part_path = part_path.to_path_buf();
        let progress_path = progress_path.to_path_buf();

        let timeout = self.timeout;
        for chunk in pending {
            let client = self.client.clone();
            let url = url.clone();
            let part_path = part_path.clone();
            let progress_path = progress_path.clone();
            let progress = progress.clone();
            let sem = sem.clone();

            let permit = sem
                .acquire_owned()
                .await
                .map_err(|e| format!("获取并发许可失败: {}", e))?;

            let chunk_index = chunk.index;
            set.spawn(async move {
                let _permit = permit;
                let bytes = download_chunk(&client, &url, &part_path, chunk, timeout)
                    .await?;
                let mut p = progress.lock().await;
                if !p.chunks.contains(&chunk_index) {
                    p.chunks.push(chunk_index);
                }
                save_progress(&progress_path, &*p).await?;
                Ok::<u64, String>(bytes)
            });
        }

        while let Some(res) = set.join_next().await {
            res.map_err(|e| format!("分片任务异常: {}", e))??;
        }

        Ok(())
    }
}

fn part_file(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.part", dest.to_string_lossy()))
}

fn progress_file(part_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.progress", part_path.to_string_lossy()))
}

async fn ensure_file_size(path: &Path, size: u64) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .await
        .map_err(|e| format!("创建临时文件失败: {}", e))?;
    file.set_len(size)
        .await
        .map_err(|e| format!("预分配临时文件失败: {}", e))?;
    file.flush()
        .await
        .map_err(|e| format!("刷新临时文件失败: {}", e))?;
    Ok(())
}

async fn load_progress(path: &Path) -> Result<Progress, String> {
    let text = fs::read_to_string(path)
        .await
        .map_err(|e| format!("读取进度文件失败: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("解析进度文件失败: {}", e))
}

async fn save_progress(path: &Path, progress: &Progress) -> Result<(), String> {
    let text = serde_json::to_string_pretty(progress)
        .map_err(|e| format!("序列化进度文件失败: {}", e))?;
    fs::write(path, text)
        .await
        .map_err(|e| format!("保存进度文件失败: {}", e))
}
