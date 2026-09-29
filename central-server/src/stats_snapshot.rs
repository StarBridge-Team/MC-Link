//! 定时快照线程 — 每 5 秒记录一次全局状态快照
//!
//! 使用 handlers::compute_stats() 统一统计逻辑。

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::handlers::compute_stats;
use crate::types::*;

/// 启动快照线程，每 5 秒拍一次快照，保留最近 60 个（5 分钟）
pub fn start_snapshot_thread(state: Arc<CentralState>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(5));
            let snapshot = compute_stats(&state);
            let mut history = state.stats_history.lock().unwrap_or_else(|e| e.into_inner());
            history.push_back(snapshot);
            while history.len() > 60 {
                history.pop_front();
            }
        }
    });
}