//! URL 背景的下载与本地缓存。
//!
//! # 为什么要后端下载
//!
//! 原本网络背景图由前端直接交给浏览器加载，后端拿不到字节，于是**无法取色**；
//! 而且随机图片 API 每次请求返回的图都不同，会出现"取色用的是 A 图、显示的是 B 图"，
//! 配色与背景对不上。
//!
//! 改为后端下载后，"取色的那张图"和"显示的那张图"是同一个文件，从根上消除不一致。
//!
//! # 为什么不复用 `downloader/verified.rs`
//!
//! 那个模块是"**可校验**下载"，前提是已知期望 SHA256（适配器包、应用更新包都是发布物）。
//! 背景图 URL 是任意用户输入，没有哈希可比 —— 走那条路会在强制的 SHA256 校验处直接失败。
//! 两者约束不同（这里是"尽力拿到、拿不到就用旧缓存"，那里是"校验不过就丢弃"），
//! 因此各自实现，而不是放宽前者的 fail-closed 语义。
//!
//! # 缓存命名：URL 哈希 + 世代号
//!
//! 因为策略是"每次启动 / 每次点应用都重拉"，随机图片 API 每次返回不同的图。
//! 若只按 URL 哈希命名，重拉会直接覆盖，且无法判断新旧、无法在下载失败时回退。
//! 所以文件名是 `<url 哈希>-<世代>.bin`，世代号单调递增：
//! - 写入永远落在**新世代**文件上，下载失败时旧世代完好无损；
//! - 写完后用一份索引把"该 URL 当前用哪个世代"记录下来，前端只读当前世代；
//! - 顺带清掉更早的世代，避免磁盘无限增长。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 单张背景图 / 视频的体积上限。
///
/// 与"下载发布物"不同，这里没有哈希可比，所以**体积上限是唯一能挡住
/// "对端无限推送撑爆磁盘"的手段**，必须设。视频给得宽一些。
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_VIDEO_BYTES: u64 = 512 * 1024 * 1024;

/// 连接与整体超时。
///
/// 整体超时给得比较宽：视频可能几十 MB，慢速链路下 30 秒不够。
/// 与既有下载器的取舍相反（那边用空闲超时），这里刻意简单些——
/// 缓存下载失败是可接受的（回退旧缓存），不值得为它引入一套分片续传。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(180);

/// 一个 URL 的缓存条目。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheEntry {
    /// 当前世代文件名（相对 `Background/remote/`）。
    pub file: String,
    /// 世代号，单调递增。
    pub generation: u64,
    /// 内容类型（用于前端判断是图还是视频）。
    #[serde(default)]
    pub content_type: String,
}

/// 索引：URL → 当前条目。整体存成一份 JSON，避免为每个 URL 建一个文件。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CacheIndex {
    entries: HashMap<String, CacheEntry>,
}

/// URL 背景缓存。
pub struct RemoteCache {
    dir: PathBuf,
    /// 内存里的索引副本 + 互斥：下载是并发的（启动时可能同时拉图与视频）。
    index: Mutex<CacheIndex>,
    http: reqwest::blocking::Client,
}

/// 下载结果。
pub struct FetchOutcome {
    pub path: PathBuf,
    pub content_type: String,
    /// 命中旧缓存时为 true。调用方可据此提示"当前用的是缓存版本"。
    pub from_cache: bool,
}

impl RemoteCache {
    /// # 参数是**数据目录**，不是缓存目录
    ///
    /// 内部会再拼 `Background/remote/`。这条契约必须写清楚：传错时不会报错，
    /// 只会让缓存悄悄落到 `<传入路径>/Background/remote/`（例如把缓存目录再传进来，
    /// 就得到 `Background/remote/Background/remote/`），排查起来很费时间——
    /// 写这个模块的初版测试就踩了它。
    pub fn new(data_dir: &Path) -> Result<Self, String> {
        let dir = data_dir.join("Background").join("remote");
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建背景缓存目录失败: {e}"))?;

        let http = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .build()
            .map_err(|e| format!("创建背景下载客户端失败: {e}"))?;

        let index = Self::load_index(&dir);
        Ok(Self {
            dir,
            index: Mutex::new(index),
            http,
        })
    }

    fn index_path(dir: &Path) -> PathBuf {
        dir.join("index.json")
    }

    fn load_index(dir: &Path) -> CacheIndex {
        std::fs::read_to_string(Self::index_path(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_index(&self, index: &CacheIndex) -> Result<(), String> {
        let text = serde_json::to_string_pretty(index)
            .map_err(|e| format!("序列化背景缓存索引失败: {e}"))?;
        std::fs::write(Self::index_path(&self.dir), text)
            .map_err(|e| format!("写入背景缓存索引失败: {e}"))
    }

    /// URL → 稳定的短哈希。用它当文件名前缀，避免 URL 里的 `/`、`?` 污染路径。
    fn url_key(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let digest = hasher.finalize();
        // 8 字节足够（碰撞概率可忽略），且文件名短一些便于排查。
        digest[..8].iter().map(|b| format!("{b:02x}")).collect()
    }

    /// 取当前世代对应的文件路径（不存在则 `None`）。
    pub fn cached_path(&self, url: &str) -> Option<PathBuf> {
        let index = self.index.lock().ok()?;
        let entry = index.entries.get(url)?;
        let path = self.dir.join(&entry.file);
        if path.is_file() {
            Some(path)
        } else {
            None
        }
    }

    /// 下载 URL 到缓存（新世代），返回可用的文件路径。
    ///
    /// `is_video` 只用于挑体积上限：视频比图片给得宽。
    ///
    /// **下载失败时回退到旧缓存**（若存在）——"断网也要能看"是明确要求；
    /// 两者都没有才报错。
    pub fn fetch(&self, url: &str, is_video: bool) -> Result<FetchOutcome, String> {
        let limit = if is_video { MAX_VIDEO_BYTES } else { MAX_IMAGE_BYTES };
        match self.download(url, limit) {
            Ok((file, content_type, generation)) => {
                let entry = CacheEntry {
                    file: file.clone(),
                    generation,
                    content_type: content_type.clone(),
                };
                // 更新索引并清理旧世代。
                let previous = {
                    let mut index = self.index.lock().map_err(|_| "背景缓存索引锁不可用")?;
                    let prev = index.entries.insert(url.to_string(), entry).map(|e| e.file);
                    self.save_index(&index)?;
                    prev
                };
                if let Some(prev) = previous {
                    if prev != file {
                        // 清理失败不影响本次结果：只是多占一点磁盘。
                        let _ = std::fs::remove_file(self.dir.join(prev));
                    }
                }
                Ok(FetchOutcome {
                    path: self.dir.join(file),
                    content_type,
                    from_cache: false,
                })
            }
            Err(e) => {
                // 回退旧缓存。这条路径正是"随机图片 API 挂了 / 断网"时的表现。
                if let Some(path) = self.cached_path(url) {
                    let content_type = self
                        .index
                        .lock()
                        .ok()
                        .and_then(|i| i.entries.get(url).map(|e| e.content_type.clone()))
                        .unwrap_or_default();
                    eprintln!("[背景] 下载失败，改用旧缓存: {e}");
                    return Ok(FetchOutcome {
                        path,
                        content_type,
                        from_cache: true,
                    });
                }
                Err(e)
            }
        }
    }

    /// 真正下载。返回 `(文件名, content-type, 世代号)`。
    fn download(&self, url: &str, limit: u64) -> Result<(String, String, u64), String> {
        let mut resp = self
            .http
            .get(url)
            .send()
            .map_err(|e| format!("请求失败: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!("服务端返回 HTTP {status}"));
        }

        // 先看 `Content-Length`：能在写盘前就挡掉超大文件，不必先下完再判断。
        if let Some(len) = resp.content_length() {
            if len > limit {
                return Err(format!("文件过大（{len} 字节，上限 {limit}）"));
            }
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let generation = {
            let index = self.index.lock().map_err(|_| "背景缓存索引锁不可用")?;
            index.entries.get(url).map(|e| e.generation + 1).unwrap_or(1)
        };
        let file = format!("{}-{}.bin", Self::url_key(url), generation);
        let final_path = self.dir.join(&file);
        // 先写临时文件再改名：中途失败不会留下一个"看起来完整"的半截缓存。
        let tmp_path = self.dir.join(format!("{file}.part"));

        {
            let mut file_handle = std::fs::File::create(&tmp_path)
                .map_err(|e| format!("创建缓存文件失败: {e}"))?;
            let mut written: u64 = 0;
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = resp.read(&mut buf).map_err(|e| format!("读取响应失败: {e}"))?;
                if n == 0 {
                    break;
                }
                written += n as u64;
                // 分块累计校验：`Content-Length` 可能缺失（分块传输），
                // 那时只有这里能挡住超大响应。
                if written > limit {
                    let _ = std::fs::remove_file(&tmp_path);
                    return Err(format!("文件超过上限（{limit} 字节），已中断"));
                }
                file_handle
                    .write_all(&buf[..n])
                    .map_err(|e| format!("写入缓存失败: {e}"))?;
            }
            file_handle
                .sync_all()
                .map_err(|e| format!("落盘失败: {e}"))?;
        }

        std::fs::rename(&tmp_path, &final_path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            format!("缓存文件改名失败: {e}")
        })?;

        Ok((file, content_type, generation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个测试一个独立的**数据目录**。
    ///
    /// 用「进程 id + 纳秒 + 调用序号」组合：单靠纳秒在并行测试下可能撞（同一纳秒内
    /// 两次调用），而共享目录会让测试互相覆盖彼此的索引文件。
    ///
    /// 注意返回的是**数据目录**——`RemoteCache::new` 会再拼 `Background/remote/`。
    fn temp_data_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "mclink-bg-{}-{}-{}-{}",
            tag,
            std::process::id(),
            nanos,
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 返回 `(缓存, 数据目录, 缓存目录)`——测试同时需要后两者，且用缓存目录读索引。
    fn temp_cache(tag: &str) -> (RemoteCache, PathBuf, PathBuf) {
        let data_dir = temp_data_dir(tag);
        let cache = RemoteCache::new(&data_dir).unwrap();
        let cache_dir = cache.dir.clone();
        (cache, data_dir, cache_dir)
    }

    /// 不同 URL 必须落到不同文件；同一 URL 的世代号递增。
    #[test]
    fn url_key_is_stable_and_distinct() {
        let a1 = RemoteCache::url_key("https://example.com/a.png");
        let a2 = RemoteCache::url_key("https://example.com/a.png");
        let b = RemoteCache::url_key("https://example.com/b.png");
        assert_eq!(a1, a2, "同一 URL 的 key 必须稳定");
        assert_ne!(a1, b, "不同 URL 的 key 必须不同");
        assert!(!a1.contains('/') && !a1.contains('?'), "key 不能含路径分隔符");
    }

    /// 无网络时：没有旧缓存必须报错（而不是返回一个不存在的路径）。
    #[test]
    fn fetch_without_network_and_without_cache_fails() {
        let (cache, data_dir, _) = temp_cache("nonet");
        // 一个必定失败的地址（保留给本机不可路由的端口）。
        let r = cache.fetch("http://127.0.0.1:9/never.png", false);
        assert!(r.is_err(), "既下载不到、又没有旧缓存时应当报错");
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// 索引的读写往返：这是"前端拿到的路径一定是后端取色用的那个文件"的依据。
    #[test]
    fn index_roundtrip_keeps_current_generation() {
        let (cache, data_dir, cache_dir) = temp_cache("roundtrip");
        {
            let mut index = cache.index.lock().unwrap();
            index.entries.insert(
                "https://example.com/x.png".into(),
                CacheEntry {
                    file: "deadbeef-3.bin".into(),
                    generation: 3,
                    content_type: "image/png".into(),
                },
            );
            cache.save_index(&index).unwrap();
        }

        // 读的是**缓存目录**（`new` 拼出来的那个），不是传进去的数据目录。
        let reloaded = RemoteCache::load_index(&cache_dir);
        let entry = reloaded
            .entries
            .get("https://example.com/x.png")
            .expect("索引往返后条目应当还在");
        assert_eq!(entry.generation, 3);
        assert_eq!(entry.file, "deadbeef-3.bin");

        // 顺带钉住 `new` 的路径契约：缓存必须落在 `<data>/Background/remote`。
        assert!(
            cache_dir.ends_with(std::path::Path::new("Background").join("remote")),
            "缓存目录应为 <data>/Background/remote，实际 {}",
            cache_dir.display()
        );
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// 损坏的索引不能让它整个失效——否则所有缓存都白下了。
    #[test]
    fn corrupted_index_falls_back_to_empty() {
        let (_, data_dir, cache_dir) = temp_cache("corrupt");
        std::fs::write(RemoteCache::index_path(&cache_dir), "{ not json").unwrap();
        let index = RemoteCache::load_index(&cache_dir);
        assert!(index.entries.is_empty());
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
