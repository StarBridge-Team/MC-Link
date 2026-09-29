use std::sync::{Mutex, MutexGuard};

/// 获取 Mutex 锁，锁中毒时记录错误日志并强制恢复
///
/// 使用场景：在无法优雅处理锁中毒时，确保程序能继续运行并记录中毒事件。
/// 对于不能安全恢复的关键操作，请使用 `match lock.lock() { Ok(g) => ..., Err(_) => return }` 模式。
///
/// `context` 参数用于在日志中标识锁的用途，便于问题定位。
/// `window` 参数可选，提供时将同时向前端发送日志。
pub fn lock_or_recover<'a, T>(
    lock: &'a Mutex<T>,
    context: &str,
) -> MutexGuard<'a, T> {
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            let error_msg = format!("{} 锁中毒，强制恢复", context);
            println!("[锁] {}", error_msg);
            poisoned.into_inner()
        }
    }
}
