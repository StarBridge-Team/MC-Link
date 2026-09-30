//! 远端文件的**可校验流式下载**。
//!
//! # 为什么单独抽出来
//!
//! 适配器包与应用更新包都会被**直接执行**——前者是第三方二进制，后者是应用自身，
//! 替换错了就等于把"发布资产被替换"变成任意代码执行。因此它们的下载链路必须满足
//! 同一组约束：
//!
//! - 多镜像按顺序回退；
//! - 单文件体积上限（防对端无限推送撑爆磁盘）；
//! - 分片读取超时（防对端挂起导致下载永不结束）；
//! - SHA256 强制校验，校验失败即丢弃，绝不进入解压/执行流程；
//! - 已下载且校验通过的文件直接复用（安装失败后重试不必重新下载）。
//!
//! 这些约束此前在适配器侧实现过一次；更新包若各写一遍，两边迟早会漂移
//! （例如只在一侧加了体积上限）。本模块是唯一实现，调用方只负责把自家清单
//! 映射成 [`RemoteFile`]。

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

/// 单次读取分片的等待上限：防止对端挂起导致下载永不结束。
const CHUNK_TIMEOUT: Duration = Duration::from_secs(30);
/// 单文件体积上限：防止对端无限推送撑爆磁盘。
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// 远端文件描述。
///
/// 由调用方从各自清单格式映射而来，本模块不关心清单长什么样。
pub(crate) struct RemoteFile<'a> {
    /// 文件名，同时作为落地文件名。
    pub file: &'a str,
    /// 候选下载地址，按顺序尝试。
    pub urls: &'a [String],
    /// 小写十六进制 SHA256。
    pub sha256: &'a str,
    /// 清单声明的文件大小，仅作为进度分母的兜底。
    pub size: Option<u64>,
}

/// 下载并校验单个文件，返回落地后的绝对路径。
///
/// `on_progress(downloaded, total)`：`total` 为 0 表示长度未知，由调用方决定是否展示。
/// 目标文件已存在且哈希一致时直接复用，不重复下载。
pub(crate) async fn download_verified(
    dir: &Path,
    client: &reqwest::Client,
    remote: &RemoteFile<'_>,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    // 文件名来自远端清单，必须只是一个文件名：否则可被用来写到目录之外。
    if remote.file.trim().is_empty()
        || remote.file.contains('/')
        || remote.file.contains('\\')
        || remote.file.contains("..")
    {
        return Err("清单中的文件名非法（不得为空或包含路径分隔符），已中止下载".to_string());
    }

    let expected = remote.sha256.trim().to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("清单中的 sha256 字段非法，已中止下载".to_string());
    }
    if remote.urls.is_empty() {
        return Err("清单未提供下载地址，已中止下载".to_string());
    }

    let dest = dir.join(remote.file);
    let tmp = dir.join(format!("{}.tmp", remote.file));

    // 复用校验通过的既有文件：安装失败重试时不必重新下载整个包
    if crate::downloader::verify::verify_file(&dest, &expected)
        .await
        .unwrap_or(false)
    {
        if let Ok(meta) = tokio::fs::metadata(&dest).await {
            on_progress(meta.len(), meta.len());
        }
        return Ok(dest);
    }

    let mut last_err = String::new();
    let mut got = false;
    for url in remote.urls {
        match stream_to_file(client, url, &tmp, remote.size, &mut on_progress).await {
            Ok(()) => {
                got = true;
                break;
            }
            Err(e) => {
                last_err = format!("{}（来源 {}）", e, url);
                let _ = tokio::fs::remove_file(&tmp).await;
            }
        }
    }
    if !got {
        return Err(format!("下载失败: {}", last_err));
    }

    // 完整性校验：本模块存在的理由
    if !crate::downloader::verify::verify_file(&tmp, &expected).await? {
        let actual = crate::downloader::verify::sha256_file(&tmp)
            .await
            .unwrap_or_else(|_| "无法计算".to_string());
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(format!(
            "文件校验失败，已丢弃：期望 sha256={}，实际={}",
            expected, actual
        ));
    }

    if dest.exists() {
        let _ = tokio::fs::remove_file(&dest).await;
    }
    std::fs::rename(&tmp, &dest).map_err(|e| format!("重命名下载文件失败: {}", e))?;
    Ok(dest)
}

/// 流式下载单个来源到临时文件（分片超时与体积上限；连接超时由调用方构造 client 时设置）。
async fn stream_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    declared_size: Option<u64>,
    on_progress: &mut impl FnMut(u64, u64),
) -> Result<(), String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let total = response
        .content_length()
        .or(declared_size)
        .unwrap_or(0);
    if total > MAX_FILE_BYTES {
        return Err(format!(
            "文件体积 {} 字节超过上限 {} 字节",
            total, MAX_FILE_BYTES
        ));
    }

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("创建临时文件失败: {}", e))?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    loop {
        let step = tokio::time::timeout(CHUNK_TIMEOUT, stream.next())
            .await
            .map_err(|_| format!("读取超时（{} 秒无数据）", CHUNK_TIMEOUT.as_secs()))?;

        let chunk = match step {
            None => break,
            Some(chunk) => chunk.map_err(|e| format!("读取数据失败: {}", e))?,
        };

        downloaded += chunk.len() as u64;
        if downloaded > MAX_FILE_BYTES {
            return Err(format!("下载体积超过上限 {} 字节", MAX_FILE_BYTES));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入文件失败: {}", e))?;

        on_progress(downloaded, total);
    }

    file.flush().await.map_err(|e| format!("刷新文件失败: {}", e))?;
    Ok(())
}
