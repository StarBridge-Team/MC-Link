use std::cmp;

/// 默认目标分片大小：2 MB。
pub const DEFAULT_TARGET_CHUNK_SIZE: u64 = 2 * 1024 * 1024;
/// 默认最小线程数。
pub const DEFAULT_MIN_THREADS: usize = 1;
/// 默认最大线程数。
pub const DEFAULT_MAX_THREADS: usize = 8;

/// 下载策略，用于根据文件大小与网速决定分片线程数。
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub min_threads: usize,
    pub max_threads: usize,
    pub target_chunk_size: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            min_threads: DEFAULT_MIN_THREADS,
            max_threads: DEFAULT_MAX_THREADS,
            target_chunk_size: DEFAULT_TARGET_CHUNK_SIZE,
        }
    }
}

impl Policy {
    #[allow(dead_code)]
    pub fn new(min_threads: usize, max_threads: usize, target_chunk_size: u64) -> Self {
        Self {
            min_threads,
            max_threads,
            target_chunk_size,
        }
    }

    /// 根据文件总大小与网速决定下载线程数。
    ///
    /// - 文件过小或网速过低时使用单线程，避免连接开销大于收益。
    /// - 文件较大且网速充足时按目标分片大小计算线程数，不超过上限。
    pub fn decide_threads(&self, total_size: u64, speed_bps: Option<u64>) -> usize {
        if total_size == 0 || total_size <= self.target_chunk_size {
            return self.min_threads;
        }

        let mut threads =
            ((total_size + self.target_chunk_size - 1) / self.target_chunk_size) as usize;
        threads = cmp::max(threads, self.min_threads);
        threads = cmp::min(threads, self.max_threads);

        if let Some(speed) = speed_bps {
            let adjusted = match speed {
                s if s < 100 * 1024 => 1,
                s if s < 1024 * 1024 => threads.min(2),
                s if s < 5 * 1024 * 1024 => threads.min(4),
                _ => threads,
            };
            return cmp::max(adjusted, self.min_threads);
        }

        threads
    }
}
