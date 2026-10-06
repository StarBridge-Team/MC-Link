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
    ///
    /// 中毒时留日志：这一项本身不是安全状态（只是展示缓存），但静默吞掉会掩盖
    /// "某线程持锁期间 panic"这个更值得查的事实。
    pub fn set(&self, games: &[LocalGameFound]) {
        match self.games.write() {
            Ok(mut slot) => *slot = games.to_vec(),
            Err(_) => eprintln!("[插件] 本地游戏缓存写锁已中毒，本次结果未记录"),
        }
    }

    /// 读取当前结果（无结果时为空数组）。
    pub fn get(&self) -> Vec<LocalGameFound> {
        match self.games.read() {
            Ok(g) => g.clone(),
            Err(_) => {
                eprintln!("[插件] 本地游戏缓存读锁已中毒，返回空结果");
                Vec::new()
            }
        }
    }
}
