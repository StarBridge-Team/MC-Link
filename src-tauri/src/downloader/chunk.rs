use std::path::Path;
use std::time::Duration;
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, RANGE};
use reqwest::Client;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

/// 分片信息。
#[derive(Clone, Debug)]
pub struct Chunk {
    pub index: usize,
    pub start: u64,
    pub end: u64, // 包含 end
}

impl Chunk {
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start) + 1
    }
}

/// 探测服务器是否支持 Range 分片下载，并返回文件总大小。
pub async fn probe_range_support(
    client: &Client,
    url: &str,
    timeout: Duration,
) -> Result<(bool, Option<u64>), String> {
    let resp = client
        .head(url)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("探测下载链接失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("探测下载链接返回状态码: {}", resp.status()));
    }

    let total_size = resp
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let accept_ranges = resp
        .headers()
        .get(ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");

    let supports_range = accept_ranges.eq_ignore_ascii_case("bytes");
    Ok((supports_range, total_size))
}

/// 将文件按 chunk_size 切分为若干分片。
pub fn split_chunks(total_size: u64, chunk_size: u64) -> Vec<Chunk> {
    if total_size == 0 || chunk_size == 0 {
        return vec![];
    }
    let mut chunks = Vec::new();
    let mut start = 0u64;
    let mut index = 0usize;
    while start < total_size {
        let end = (start + chunk_size - 1).min(total_size - 1);
        chunks.push(Chunk { index, start, end });
        start = end + 1;
        index += 1;
    }
    chunks
}

/// 下载单个分片并写入文件对应偏移。
pub(crate) async fn download_chunk(
    client: &Client,
    url: &str,
    dest: &Path,
    chunk: Chunk,
    timeout: Duration,
) -> Result<u64, String> {
    let range = format!("bytes={}-{}", chunk.start, chunk.end);
    let resp = client
        .get(url)
        .header(RANGE, range)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("分片 {} 请求失败: {}", chunk.index, e))?;

    let status = resp.status();
    if status != reqwest::StatusCode::PARTIAL_CONTENT && status != reqwest::StatusCode::OK {
        return Err(format!("分片 {} 返回状态码: {}", chunk.index, status));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("分片 {} 读取失败: {}", chunk.index, e))?;

    let expected = chunk.size() as usize;
    if bytes.len() > expected {
        return Err(format!(
            "分片 {} 数据大小异常: 期望 {}，实际 {}",
            chunk.index,
            expected,
            bytes.len()
        ));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(dest)
        .await
        .map_err(|e| format!("打开目标文件失败: {}", e))?;

    file.seek(std::io::SeekFrom::Start(chunk.start))
        .await
        .map_err(|e| format!("分片 {} 定位失败: {}", chunk.index, e))?;

    file.write_all(&bytes)
        .await
        .map_err(|e| format!("分片 {} 写入失败: {}", chunk.index, e))?;

    file.flush()
        .await
        .map_err(|e| format!("分片 {} 刷新失败: {}", chunk.index, e))?;

    Ok(bytes.len() as u64)
}

/// 单线程流式下载，用于服务器不支持 Range 时的回退。
pub async fn download_single(
    client: &Client,
    url: &str,
    dest: &Path,
    timeout: Duration,
) -> Result<u64, String> {
    let mut resp = client
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("下载返回状态码: {}", resp.status()));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest)
        .await
        .map_err(|e| format!("打开目标文件失败: {}", e))?;

    let mut total = 0u64;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("读取下载流失败: {}", e))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入下载流失败: {}", e))?;
        total += chunk.len() as u64;
    }

    file.flush()
        .await
        .map_err(|e| format!("刷新下载文件失败: {}", e))?;

    Ok(total)
}
