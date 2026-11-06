//! 运行时可调参数（来自设置）。
//!
//! 这些值此前是分散在三条互不相干链路里的 `const`：下载器并发/空闲超时、
//! 插件握手超时、插件网关并发上限。硬编码的问题不是"数字选得不对"，而是
//! **用户无法按自己的网络/机器状况调整**，遇到弱网或大量插件时只能等改版本。
//!
//! # 为什么用原子变量而不是传参
//!
//! 调用点分散在下载器、插件会话、网关，逐层传参会污染这些模块的签名（且调用点很多），
//! 而这里只需要"读一个数"。原子变量是最小侵入方案：没有跨模块状态机，
//! 写入方只有两处——启动预应用与 `save_personalization`。
//!
//! 越界值统一在这里夹取，模块内不再重复判断。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::config::PersonalizationSettings;

/// 下载并行分片数。
static DOWNLOAD_PARALLEL: AtomicU64 = AtomicU64::new(4);
/// 下载空闲超时（秒）：这么久没有任何数据才判超时。
static DOWNLOAD_IDLE_SECS: AtomicU64 = AtomicU64::new(30);
/// 插件握手最长等待（秒）。
static PLUGIN_HANDSHAKE_SECS: AtomicU64 = AtomicU64::new(15);
/// 插件网关在途连接上限。
static GATEWAY_INFLIGHT: AtomicU64 = AtomicU64::new(32);

/// 把设置里的可调参数落到运行时。
pub(crate) fn apply(s: &PersonalizationSettings) {
    DOWNLOAD_PARALLEL.store(s.download_parallel.clamp(1.0, 8.0) as u64, Ordering::Relaxed);
    DOWNLOAD_IDLE_SECS.store(
        s.download_idle_timeout.clamp(10.0, 120.0) as u64,
        Ordering::Relaxed,
    );
    PLUGIN_HANDSHAKE_SECS.store(
        s.plugin_handshake_timeout.clamp(5.0, 60.0) as u64,
        Ordering::Relaxed,
    );
    GATEWAY_INFLIGHT.store(
        s.gateway_max_inflight.clamp(4.0, 128.0) as u64,
        Ordering::Relaxed,
    );
}

pub(crate) fn download_parallel() -> usize {
    DOWNLOAD_PARALLEL.load(Ordering::Relaxed).clamp(1, 8) as usize
}

pub(crate) fn download_idle_timeout() -> Duration {
    Duration::from_secs(DOWNLOAD_IDLE_SECS.load(Ordering::Relaxed).clamp(10, 120))
}

pub(crate) fn plugin_handshake_timeout() -> Duration {
    Duration::from_secs(PLUGIN_HANDSHAKE_SECS.load(Ordering::Relaxed).clamp(5, 60))
}

pub(crate) fn gateway_max_inflight() -> usize {
    GATEWAY_INFLIGHT.load(Ordering::Relaxed).clamp(4, 128) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> PersonalizationSettings {
        PersonalizationSettings::default()
    }

    #[test]
    fn defaults_match_previous_constants() {
        let s = settings();
        apply(&s);
        assert_eq!(download_parallel(), 4);
        assert_eq!(download_idle_timeout(), Duration::from_secs(30));
        assert_eq!(plugin_handshake_timeout(), Duration::from_secs(15));
        assert_eq!(gateway_max_inflight(), 32);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let mut s = settings();
        s.download_parallel = 999.0;
        s.download_idle_timeout = 1.0;
        s.plugin_handshake_timeout = 1.0;
        s.gateway_max_inflight = 999.0;
        apply(&s);
        assert_eq!(download_parallel(), 8);
        assert_eq!(download_idle_timeout(), Duration::from_secs(10));
        assert_eq!(plugin_handshake_timeout(), Duration::from_secs(5));
        assert_eq!(gateway_max_inflight(), 128);

        // 复位，避免影响其它用例（静态变量是全局的）。
        apply(&settings());
    }
}
