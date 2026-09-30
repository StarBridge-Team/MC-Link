//! 远端文件的**可校验下载**：多镜像、断点续传、分片并行、强制 SHA256。
//!
//! # 为什么是唯一实现
//!
//! 适配器包与应用更新包都会被**直接执行**（前者是第三方二进制，后者是应用自身），
//! 因此下载链路必须满足同一组约束，且只应存在一份实现：
//!
//! - 多镜像按顺序回退；
//! - 单文件体积上限（防对端无限推送撑爆磁盘）；
//! - **空闲超时**而非整体超时（大分片在慢速链路上不该被整体判死）；
//! - SHA256 强制校验，校验失败即丢弃，绝不进入解压/执行流程；
//! - 已下载且校验通过的文件直接复用。
//!
//! # 关于"分片并行"与"续传"的实现方式
//!
//! 这两个能力没有现成的成熟 crate 可用：crates.io 上并不存在"多连接并行 + 断点续传"
//! 的通用下载库（同类需求要么调用外部 `aria2c`/`axel` 二进制，要么自己用
//! `reqwest` + `tokio` 实现调度）。因此这里基于两个成熟基础库自行实现调度：
//! 传输交给 `reqwest`，并发与超时交给 `tokio`，本模块只负责分片划分、
//! 续传记账与校验。
//!
//! 前提是服务端支持 HTTP Range（返回 206 + `Content-Range`）。探测方式见 [`probe`]：
//! 发一个 `Range: bytes=0-0` 的 GET，拿到 206 才认为可续传；否则整体退化为单流下载。
//! 服务端不支持 Range 时本模块**照样工作**，只是没有并行与续传。
//!
//! # 续传的安全边界
//!
//! 进度文件里记录着**期望的 SHA256 与总长度**，与本次请求不一致时整份进度作废——
//! 否则可能把上一个版本的字节混进新包。拼装完成后仍要整体校验一次，
//! 校验不过就删掉临时文件并报错（fail-closed），不会把半个文件当成好消息返回。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

/// 单次读取的空闲上限：超过这么久没有任何数据才判定为超时。
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// 探测 Range 支持的超时。
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// 单文件体积上限：防止对端无限推送撑爆磁盘。
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// 分片大小。太小会让请求数暴涨，太大则续传粒度太粗。
const CHUNK_SIZE: u64 = 4 * 1024 * 1024;
/// 并行分片数。
const MAX_PARALLEL: usize = 4;
/// 小于该体积不值得分片：调度与握手的开销比省下的时间还多。
const MIN_PARALLEL_BYTES: u64 = 8 * 1024 * 1024;

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

/// 续传进度（与 `.part` 同级存放的 `.progress` 文件）。
#[derive(Debug, Default, Serialize, Deserialize)]
struct PartialProgress {
    /// 进度所属的期望哈希：不一致即整份作废，避免混入别的版本的字节。
    #[serde(default)]
    sha256: String,
    /// 进度所属的总长度。
    #[serde(default)]
    total: u64,
    /// 已完成的分片索引。
    #[serde(default)]
    done: Vec<u32>,
}

/// 服务端能力探测结果。
#[derive(Debug, PartialEq, Eq)]
struct Probe {
    /// 文件总长度（拿不到则为 None，例如对端未给 Content-Length）。
    total: Option<u64>,
    /// 是否支持范围请求（响应 206 才算）。
    accepts_ranges: bool,
}

/// 下载并校验单个文件，返回落地后的绝对路径。
///
/// `on_progress(downloaded, total)`：`total` 为 0 表示长度未知，由调用方决定是否展示。
/// 目标文件已存在且哈希一致时直接复用，不重复下载。
pub(crate) async fn download_verified(
    dir: &Path,
    client: &reqwest::Client,
    remote: &RemoteFile<'_>,
    mut on_progress: impl FnMut(u64, u64) + Send,
) -> Result<PathBuf, String> {
    validate(remote)?;

    let expected = remote.sha256.trim().to_ascii_lowercase();
    let dest = dir.join(remote.file);
    let part = dir.join(format!("{}.part", remote.file));
    let progress_path = dir.join(format!("{}.progress", remote.file));

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

    // 回调要被多个分片任务同时使用，因此包一层异步锁
    let emit = Arc::new(tokio::sync::Mutex::new(on_progress));

    let mut last_err = String::new();
    for url in remote.urls {
        match fetch_from(client, url, &part, &progress_path, remote, &expected, &emit).await {
            Ok(()) => {
                // 拼装完成：整体校验一次。这是"下载的东西能被直接执行"的底线。
                if !crate::downloader::verify::verify_file(&part, &expected).await? {
                    let actual = crate::downloader::verify::sha256_file(&part)
                        .await
                        .unwrap_or_else(|_| "无法计算".to_string());
                    discard(&part, &progress_path).await;
                    last_err = format!(
                        "文件校验失败（来源 {}）：期望 sha256={}，实际={}",
                        url, expected, actual
                    );
                    continue;
                }

                if dest.exists() {
                    let _ = tokio::fs::remove_file(&dest).await;
                }
                std::fs::rename(&part, &dest)
                    .map_err(|e| format!("重命名下载文件失败: {}", e))?;
                let _ = tokio::fs::remove_file(&progress_path).await;
                return Ok(dest);
            }
            Err(e) => {
                // 网络类失败保留进度，换下一个镜像时可以接着传
                last_err = format!("{}（来源 {}）", e, url);
            }
        }
    }

    Err(format!("下载失败: {}", last_err))
}

/// 校验清单给出的字段是否可用。
fn validate(remote: &RemoteFile<'_>) -> Result<(), String> {
    // 文件名来自远端清单，必须只是一个文件名：否则可被用来写到目录之外
    if remote.file.trim().is_empty()
        || remote.file.contains('/')
        || remote.file.contains('\\')
        || remote.file.contains("..")
    {
        return Err("清单中的文件名非法（不得为空或包含路径分隔符），已中止下载".to_string());
    }

    let expected = remote.sha256.trim();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("清单中的 sha256 字段非法，已中止下载".to_string());
    }
    if remote.urls.is_empty() {
        return Err("清单未提供下载地址，已中止下载".to_string());
    }
    Ok(())
}

/// 从单个来源把文件取到 `.part`（可续传、可分片）。
async fn fetch_from(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    progress_path: &Path,
    remote: &RemoteFile<'_>,
    expected: &str,
    emit: &Arc<tokio::sync::Mutex<impl FnMut(u64, u64) + Send>>,
) -> Result<(), String> {
    let probe = probe(client, url).await?;

    match probe {
        Probe {
            total: Some(total),
            accepts_ranges: true,
        } if total >= MIN_PARALLEL_BYTES => {
            download_chunked(client, url, part, progress_path, total, expected, emit).await
        }
        _ => {
            // 不支持范围请求或体积很小：单流下载。此时续传没有意义，清掉旧进度。
            let _ = tokio::fs::remove_file(progress_path).await;
            download_streaming(client, url, part, probe.total, remote.size, emit).await
        }
    }
}

/// 探测服务端能力：发一个 `Range: bytes=0-0`。
///
/// 用 GET 而不是 HEAD，是因为"响应 206 + Content-Range"才是范围请求真正被支持的证据；
/// 只看 `Accept-Ranges` 头可能被中间层伪造。
async fn probe(client: &reqwest::Client, url: &str) -> Result<Probe, String> {
    let response = client
        .get(url)
        .header(RANGE, "bytes=0-0")
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = response.status();
    let headers = response.headers().clone();
    // 立刻丢弃响应体：只需要头信息（reqwest 会在 drop 时结束连接）
    drop(response);

    if status == reqwest::StatusCode::PARTIAL_CONTENT {
        let total = headers
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range_total);
        return Ok(Probe {
            total,
            accepts_ranges: true,
        });
    }

    if status.is_success() {
        let total = headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        return Ok(Probe {
            total,
            accepts_ranges: false,
        });
    }

    Err(format!("HTTP {}", status))
}

/// 从 `Content-Range: bytes 0-0/12345` 里取出总长度。
fn parse_content_range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse::<u64>().ok()
}

/// 分片并行下载，支持断点续传。
async fn download_chunked(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    progress_path: &Path,
    total: u64,
    expected: &str,
    emit: &Arc<tokio::sync::Mutex<impl FnMut(u64, u64) + Send>>,
) -> Result<(), String> {
    if total > MAX_FILE_BYTES {
        return Err(format!(
            "文件体积 {} 字节超过上限 {} 字节",
            total, MAX_FILE_BYTES
        ));
    }

    preallocate(part, total).await?;

    let mut progress = load_progress(progress_path, expected, total).await;
    let chunk_count = total.div_ceil(CHUNK_SIZE) as u32;
    progress.done.sort_unstable();
    progress.done.dedup();
    progress.done.retain(|i| *i < chunk_count);

    let done_bytes: u64 = progress
        .done
        .iter()
        .map(|i| chunk_len(*i, total))
        .sum();
    let counter = Arc::new(AtomicU64::new(done_bytes));

    let pending: Vec<u32> = (0..chunk_count)
        .filter(|i| !progress.done.contains(i))
        .collect();

    if pending.is_empty() {
        let mut progress = emit.lock().await;
        progress(total, total);
        return Ok(());
    }

    let progress = Arc::new(tokio::sync::Mutex::new(progress));

    // buffer_unordered 自身即限制并发度，无需额外信号量
    let tasks = futures_util::stream::iter(pending.into_iter().map(|index| {
        let client = client.clone();
        let url = url.to_string();
        let part = part.to_path_buf();
        let progress_path = progress_path.to_path_buf();
        let progress = progress.clone();
        let counter = counter.clone();
        let emit = emit.clone();

        async move {
            let (start, end) = chunk_bounds(index, total);
            let bytes = fetch_range(&client, &url, start, end).await?;
            write_at(&part, start, &bytes).await?;

            // 记账与落盘在同一把锁里完成：保证进度文件的写入顺序与磁盘状态一致。
            // 先记账再上报，崩溃后重启时"已写入的字节"一定已经被记下。
            {
                let mut p = progress.lock().await;
                p.done.push(index);
                save_progress(&progress_path, &p).await?;
            }

            let done = counter.fetch_add(bytes.len() as u64, Ordering::Relaxed)
                + bytes.len() as u64;
            let mut emit = emit.lock().await;
            emit(done, total);
            Ok::<(), String>(())
        }
    }))
    .buffer_unordered(MAX_PARALLEL)
    .collect::<Vec<Result<(), String>>>()
    .await;

    // 有任何一片失败就整体报错：已完成的进度留在磁盘上，重试时接着传
    for result in tasks {
        result?;
    }
    Ok(())
}

/// 取一个分片，返回其字节。
async fn fetch_range(
    client: &reqwest::Client,
    url: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, String> {
    let expected_len = end - start + 1;

    let response = client
        .get(url)
        .header(RANGE, format!("bytes={}-{}", start, end))
        .send()
        .await
        .map_err(|e| format!("分片 {}-{} 请求失败: {}", start, end, e))?;

    let status = response.status();
    if status != reqwest::StatusCode::PARTIAL_CONTENT {
        // 探测阶段确认过支持 Range，这里再出现非 206 说明对端不一致，直接失败
        return Err(format!("分片 {}-{} 返回状态码: {}", start, end, status));
    }

    let mut out: Vec<u8> = Vec::with_capacity(expected_len as usize);
    let mut stream = response.bytes_stream();
    loop {
        let step = tokio::time::timeout(IDLE_TIMEOUT, stream.next())
            .await
            .map_err(|_| format!("分片 {}-{} 读取超时（{} 秒无数据）", start, end, IDLE_TIMEOUT.as_secs()))?;

        let chunk = match step {
            None => break,
            Some(chunk) => chunk.map_err(|e| format!("分片 {}-{} 读取失败: {}", start, end, e))?,
        };

        out.extend_from_slice(&chunk);
        if out.len() as u64 > expected_len {
            return Err(format!(
                "分片 {}-{} 数据多于请求范围（已收 {} 字节）",
                start, end, out.len()
            ));
        }
    }

    if out.len() as u64 != expected_len {
        return Err(format!(
            "分片 {}-{} 数据不完整：期望 {} 字节，实际 {} 字节",
            start, end, expected_len, out.len()
        ));
    }
    Ok(out)
}

/// 单流下载（对端不支持 Range 或文件很小）。
async fn download_streaming(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    declared_total: Option<u64>,
    fallback_total: Option<u64>,
    emit: &Arc<tokio::sync::Mutex<impl FnMut(u64, u64) + Send>>,
) -> Result<(), String> {
    let total = declared_total.or(fallback_total).unwrap_or(0);
    if total > MAX_FILE_BYTES {
        return Err(format!(
            "文件体积 {} 字节超过上限 {} 字节",
            total, MAX_FILE_BYTES
        ));
    }

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dest)
        .await
        .map_err(|e| format!("创建临时文件失败: {}", e))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    loop {
        let step = tokio::time::timeout(IDLE_TIMEOUT, stream.next())
            .await
            .map_err(|_| {
                format!("读取超时（{} 秒无数据）", IDLE_TIMEOUT.as_secs())
            })?;

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

        let mut emit = emit.lock().await;
        emit(downloaded, total);
    }

    file.flush().await.map_err(|e| format!("刷新文件失败: {}", e))?;
    Ok(())
}

/// 第 `index` 片的闭区间 `[start, end]`。
fn chunk_bounds(index: u32, total: u64) -> (u64, u64) {
    let start = index as u64 * CHUNK_SIZE;
    let end = (start + CHUNK_SIZE - 1).min(total - 1);
    (start, end)
}

/// 第 `index` 片的字节数（用于续传后的进度起点）。
fn chunk_len(index: u32, total: u64) -> u64 {
    let (start, end) = chunk_bounds(index, total);
    end - start + 1
}

async fn preallocate(path: &Path, total: u64) -> Result<(), String> {
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .await
        .map_err(|e| format!("创建临时文件失败: {}", e))?;
    file.set_len(total)
        .await
        .map_err(|e| format!("预分配临时文件失败: {}", e))?;
    Ok(())
}

/// 在指定偏移写入字节（各分片写的是互不重叠的区间，因此无需额外加锁）。
async fn write_at(path: &Path, offset: u64, bytes: &[u8]) -> Result<(), String> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .await
        .map_err(|e| format!("打开目标文件失败: {}", e))?;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(|e| format!("定位写入偏移失败: {}", e))?;
    file.write_all(bytes)
        .await
        .map_err(|e| format!("写入文件失败: {}", e))?;
    file.flush().await.map_err(|e| format!("刷新文件失败: {}", e))?;
    Ok(())
}

/// 读取进度；与本次请求的哈希/长度不一致，或文件损坏时，一律从头开始。
async fn load_progress(path: &Path, sha256: &str, total: u64) -> PartialProgress {
    let empty = || PartialProgress {
        sha256: sha256.to_string(),
        total,
        done: Vec::new(),
    };

    match tokio::fs::read_to_string(path).await {
        Ok(text) => match serde_json::from_str::<PartialProgress>(&text) {
            Ok(p) if p.sha256 == sha256 && p.total == total => p,
            // 属于另一个版本的进度：作废，不能混用
            _ => empty(),
        },
        Err(_) => empty(),
    }
}

async fn save_progress(path: &Path, progress: &PartialProgress) -> Result<(), String> {
    let text = serde_json::to_string(progress)
        .map_err(|e| format!("序列化下载进度失败: {}", e))?;
    tokio::fs::write(path, text)
        .await
        .map_err(|e| format!("保存下载进度失败: {}", e))
}

async fn discard(part: &Path, progress_path: &Path) {
    let _ = tokio::fs::remove_file(part).await;
    let _ = tokio::fs::remove_file(progress_path).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    /// 极简测试服务器：支持 `Range: bytes=a-b`，可关闭范围支持，并统计并发峰值与总字节数。
    struct TestServer {
        base: String,
        max_concurrent: Arc<AtomicUsize>,
        served: Arc<AtomicU64>,
    }

    /// 记录并发峰值的守卫：进入时 +1 并刷新峰值，离开时 -1。
    struct ConcurrencyGuard {
        current: Arc<AtomicUsize>,
    }

    impl ConcurrencyGuard {
        fn new(current: Arc<AtomicUsize>, peak: Arc<AtomicUsize>) -> Self {
            let now = current.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            Self { current }
        }
    }

    impl Drop for ConcurrencyGuard {
        fn drop(&mut self) {
            self.current.fetch_sub(1, Ordering::SeqCst);
        }
    }

    async fn start_server(body: Vec<u8>, accept_ranges: bool) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let body = Arc::new(body);
        let current = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let served = Arc::new(AtomicU64::new(0));

        let (cur, peak, total_served) = (current.clone(), max_concurrent.clone(), served.clone());
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let body = body.clone();
                let cur = cur.clone();
                let peak = peak.clone();
                let total_served = total_served.clone();
                tokio::spawn(async move {
                    let _guard = ConcurrencyGuard::new(cur, peak);
                    handle(socket, body, accept_ranges, total_served).await;
                });
            }
        });

        TestServer {
            base: format!("http://{}", addr),
            max_concurrent,
            served,
        }
    }

    async fn handle(
        mut socket: tokio::net::TcpStream,
        body: Arc<Vec<u8>>,
        accept_ranges: bool,
        served: Arc<AtomicU64>,
    ) {
        let mut buf = vec![0u8; 2048];
        let mut head = Vec::new();
        // 读到请求头结束
        loop {
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            head.extend_from_slice(&buf[..n]);
            if head.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }

        let text = String::from_utf8_lossy(&head).to_lowercase();
        let range = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("range: bytes=").map(|v| v.to_string()));

        // 制造重叠窗口：并发下载才能真正被测出来
        tokio::time::sleep(Duration::from_millis(40)).await;

        let range = range.filter(|_| accept_ranges);
        let (status, payload, content_range) = match range.as_deref().and_then(|r| parse_test_range(r, body.len() as u64)) {
            Some((start, end)) => (
                "206 Partial Content",
                body[start as usize..=end as usize].to_vec(),
                Some(format!("bytes {}-{}/{}", start, end, body.len())),
            ),
            None => ("200 OK", body.as_ref().clone(), None),
        };

        served.fetch_add(payload.len() as u64, Ordering::SeqCst);

        let mut head_out = format!(
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n",
            status,
            payload.len()
        );
        if let Some(cr) = content_range {
            head_out.push_str(&format!("Content-Range: {}\r\n", cr));
        }
        head_out.push_str("\r\n");

        let _ = socket.write_all(head_out.as_bytes()).await;
        let _ = socket.write_all(&payload).await;
    }

    /// 测试服务器用的范围解析：只需支持 `start-end` 与 `start-`。
    fn parse_test_range(spec: &str, total: u64) -> Option<(u64, u64)> {
        let (start, end) = spec.split_once('-')?;
        let start: u64 = start.trim().parse().ok()?;
        if start >= total {
            return None;
        }
        let end = match end.trim() {
            "" => total - 1,
            v => v.parse::<u64>().ok()?.min(total - 1),
        };
        (end >= start).then_some((start, end))
    }

    fn body_bytes(len: usize) -> Vec<u8> {
        // 可复现的伪随机内容，避免压缩/重复内容影响校验
        (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mc-link-verified-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .build()
            .expect("构建测试客户端失败")
    }

    #[tokio::test]
    async fn downloads_large_file_in_parallel_chunks() {
        let body = body_bytes(9 * 1024 * 1024);
        let server = start_server(body.clone(), true).await;
        let dir = scratch("parallel");
        let hash = sha256_hex(&body);

        let remote = RemoteFile {
            file: "big.bin",
            urls: &[format!("{}/big.bin", server.base)],
            sha256: &hash,
            size: Some(body.len() as u64),
        };

        let path = download_verified(&dir, &client(), &remote, |_, _| {})
            .await
            .expect("下载应当成功");

        assert_eq!(std::fs::read(&path).unwrap(), body, "内容必须与源一致");
        assert!(
            server.max_concurrent.load(Ordering::SeqCst) >= 2,
            "大文件应当并行分片下载，并发峰值={}",
            server.max_concurrent.load(Ordering::SeqCst)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn resumes_from_existing_progress() {
        let body = body_bytes(9 * 1024 * 1024);
        let server = start_server(body.clone(), true).await;
        let dir = scratch("resume");
        let hash = sha256_hex(&body);
        let total = body.len() as u64;

        // 预先写好第一片，并登记为已完成（模拟上一次下载中断）
        let part = dir.join("big.bin.part");
        let downloaded_once = chunk_len(0, total);
        std::fs::write(&part, &body[..downloaded_once as usize]).unwrap();
        let progress = PartialProgress {
            sha256: hash.clone(),
            total,
            done: vec![0],
        };
        std::fs::write(
            dir.join("big.bin.progress"),
            serde_json::to_string(&progress).unwrap(),
        )
        .unwrap();

        let remote = RemoteFile {
            file: "big.bin",
            urls: &[format!("{}/big.bin", server.base)],
            sha256: &hash,
            size: Some(total),
        };

        let path = download_verified(&dir, &client(), &remote, |_, _| {})
            .await
            .expect("续传应当成功");

        assert_eq!(std::fs::read(&path).unwrap(), body, "续传后内容必须完好");
        let served = server.served.load(Ordering::SeqCst);
        assert!(
            served < total,
            "已完成的分片不应重传：已传输 {} 字节，总长 {} 字节",
            served,
            total
        );
        assert!(
            !dir.join("big.bin.progress").exists(),
            "完成后应清理进度文件"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn falls_back_to_single_stream_without_range_support() {
        let body = body_bytes(9 * 1024 * 1024);
        let server = start_server(body.clone(), false).await;
        let dir = scratch("no-range");
        let hash = sha256_hex(&body);

        let remote = RemoteFile {
            file: "big.bin",
            urls: &[format!("{}/big.bin", server.base)],
            sha256: &hash,
            size: Some(body.len() as u64),
        };

        let path = download_verified(&dir, &client(), &remote, |_, _| {})
            .await
            .expect("不支持 Range 时也应能下载");

        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(
            server.max_concurrent.load(Ordering::SeqCst),
            1,
            "不支持范围请求时不应发起并行分片"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn discards_everything_when_hash_mismatches() {
        let body = body_bytes(9 * 1024 * 1024);
        let server = start_server(body.clone(), true).await;
        let dir = scratch("bad-hash");

        let remote = RemoteFile {
            file: "big.bin",
            urls: &[format!("{}/big.bin", server.base)],
            sha256: &"a".repeat(64),
            size: Some(body.len() as u64),
        };

        let err = download_verified(&dir, &client(), &remote, |_, _| {})
            .await
            .expect_err("哈希不符必须失败");

        assert!(err.contains("校验失败"), "错误信息应说明校验失败: {}", err);
        assert!(!dir.join("big.bin").exists(), "不得留下未校验的文件");
        assert!(!dir.join("big.bin.part").exists(), "应清理分片临时文件");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reuses_already_verified_file() {
        let body = body_bytes(1024 * 1024);
        // 已存在且哈希一致的文件必须直接复用：此时不应发出任何请求
        let server = start_server(body.clone(), true).await;
        let dir = scratch("reuse");
        let hash = sha256_hex(&body);
        std::fs::write(dir.join("big.bin"), &body).unwrap();

        let remote = RemoteFile {
            file: "big.bin",
            urls: &[format!("{}/big.bin", server.base)],
            sha256: &hash,
            size: Some(body.len() as u64),
        };

        let path = download_verified(&dir, &client(), &remote, |_, _| {})
            .await
            .expect("已校验的文件应直接复用");

        assert_eq!(path, dir.join("big.bin"));
        assert_eq!(server.served.load(Ordering::SeqCst), 0, "不应产生任何下载");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chunk_bounds_cover_the_whole_file() {
        let total = CHUNK_SIZE * 2 + 100;
        assert_eq!(chunk_bounds(0, total), (0, CHUNK_SIZE - 1));
        assert_eq!(chunk_bounds(1, total), (CHUNK_SIZE, CHUNK_SIZE * 2 - 1));
        assert_eq!(chunk_bounds(2, total), (CHUNK_SIZE * 2, total - 1));
        assert_eq!(
            chunk_len(0, total) + chunk_len(1, total) + chunk_len(2, total),
            total,
            "分片长度之和必须等于文件长度"
        );
    }

    #[test]
    fn parses_total_from_content_range() {
        assert_eq!(parse_content_range_total("bytes 0-0/12345"), Some(12345));
        assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
    }
}
