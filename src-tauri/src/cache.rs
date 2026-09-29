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

/// 清除指定缓存目录下所有内容（谨慎使用）。
pub fn clear_cache(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(|e| format!("读取缓存目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取缓存项失败: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| format!("删除缓存目录失败: {}", e))?;
        } else {
            std::fs::remove_file(&path).map_err(|e| format!("删除缓存文件失败: {}", e))?;
        }
    }
    Ok(())
}
