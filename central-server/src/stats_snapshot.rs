//! 定时快照线程 — 每 5 秒记录一次全局状态快照

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use mc_link_common::utils::now_secs;
use crate::types::*;

/// 启动快照线程，每 5 秒拍一次快照，保留最近 60 个（5 分钟）
pub fn start_snapshot_thread(state: Arc<CentralState>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(5));

            let relay_count = state.relays.lock().unwrap_or_else(|e| e.into_inner()).len();
            let online_relay_count = {
                let instants = state.heartbeat_instants.lock().unwrap_or_else(|e| e.into_inner());
                let now = std::time::Instant::now();
                instants.values().filter(|t| now.duration_since(**t) < Duration::from_secs(120)).count()
            };
            let room_count = state.rooms.lock().unwrap_or_else(|e| e.into_inner()).len();
            let player_count: usize = state.players.lock().unwrap_or_else(|e| e.into_inner()).values().map(|v| v.len()).sum();
            let path_count = state.active_paths.lock().unwrap_or_else(|e| e.into_inner()).len();
            let total_traffic_bytes: u64 = state.traffic_reports.lock().unwrap_or_else(|e| e.into_inner())
                .values().map(|s| s.bytes_sent_total.saturating_add(s.bytes_recv_total)).sum();

            let snapshot = StatsSnapshot {
                timestamp: now_secs(),
                relay_count,
                online_relay_count,
                room_count,
                player_count,
                path_count,
                total_traffic_bytes,
            };

            let mut history = state.stats_history.lock().unwrap_or_else(|e| e.into_inner());
            history.push_back(snapshot);
            while history.len() > 60 {
                history.pop_front();
            }
        }
    });
}