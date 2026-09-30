//! 插件子系统内部工具。

use std::sync::{Mutex, MutexGuard};

use crate::plugin::protocol::{error_code, ErrorInfo};

/// 获取互斥锁；锁中毒时返回错误而不是 panic。
///
/// 与 `crate::utils::lock_or_recover` 的区别：插件子系统里的锁保护的是
/// **安全状态**（权限、密钥、准入表），中毒说明有线程在持有期间 panic，
/// 此时继续使用可能已被破坏的状态反而不安全，因此选择失败而非恢复。
pub fn lock_or_err<'a, T>(lock: &'a Mutex<T>, context: &str) -> Result<MutexGuard<'a, T>, ErrorInfo> {
    lock.lock().map_err(|_| {
        ErrorInfo::new(
            error_code::INTERNAL,
            format!("{} 锁已中毒，拒绝在不确定状态下继续", context),
        )
    })
}
