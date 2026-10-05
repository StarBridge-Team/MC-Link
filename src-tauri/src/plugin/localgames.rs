//! 「最近一次扫描到的本地游戏」缓存。
//!
//! # 为什么需要缓存
//!
//! 扫描结果是**一次性广播**：应用打开时跑一次 `scan_local_games`，把发现逐个用
//! `local-game-found` 事件推给前端。事件没有补发机制，于是只要监听器比事件晚一步
//! （页面重载、启动竞态、那次扫描恰巧失败），首页就再也不知道本机有没有游戏在跑，
//! 只能靠用户手动点重扫。
//!
//! 缓存让前端可以随时**主动拉取**当前状态兜底；事件仍然照发，二者读的是同一份数据。
//!
//! 单独成文件而不是塞进 `manager/mod.rs`：后者已经接近 500 行上限。

use std::sync::RwLock;

use crate::plugin::protocol::LocalGameFound;

/// 最近一次扫描的结果。
#[derive(Default)]
pub struct LocalGamesCache {
    games: RwLock<Vec<LocalGameFound>>,
}

impl LocalGamesCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 覆盖式写入。只保存「最近一次」——空结果同样要覆盖，
    /// 否则游戏退出后前端拉到的仍是上一次的陈旧数据。
    pub fn set(&self, games: &[LocalGameFound]) {
        if let Ok(mut slot) = self.games.write() {
            *slot = games.to_vec();
        }
    }

    /// 读取当前结果（无结果时为空数组）。
    pub fn get(&self) -> Vec<LocalGameFound> {
        self.games
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}
