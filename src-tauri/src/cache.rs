use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 获取缓存目录下指定相对路径的完整路径。
pub fn cache_path(cache_dir: &Path, relative: &str) -> PathBuf {
    cache_dir.join(relative)
}

/// 检查缓存文件是否存在且未过期（ttl 为 None 表示永不过期）。
pub fn is_cached(path: &Path, ttl: Option<Duration>) -> bool {
    if !path.exists() {
        return false;
    }
    match ttl {
        None => true,
        Some(ttl) => {
            let modified = match std::fs::metadata(path).and_then(|m| m.modified()) {
                Ok(t) => t,
                Err(_) => return false,
            };
            SystemTime::now()
                .duration_since(modified)
                .map_or(false, |age| age < ttl)
        }
    }
}

/// 确保缓存目录存在。
pub fn ensure_cache_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建缓存目录失败: {}", e))
}

// 这里曾经有一个 `clear_cache(dir)`：整目录清空。
//
// 它被用于"资源版本不一致 → 清掉整个 Assets/ 再重下"。问题是前端此刻可能正通过
// `asset://` 引用里面的文件，清空会让图标/字体凭空消失；半途失败还会留下一个被删残的
// 缓存。资源同步已改为逐文件校验 + 原子覆盖 + 成功后回收陈旧文件（见 `assets/pull.rs`），
// 这个能力不再需要，随之删除——留着它只会诱使下一处调用再踩同一个坑。
