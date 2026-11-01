//! 多会话注册表。
//!
//! # 为什么锁是"每会话一把"
//!
//! 建链、断链、拉状态会并发发生。若用一把全局锁，一次慢操作（适配器 3 秒握手）
//! 会把所有房间一起堵住；而 `AppMgr` 的 `write_lock` 是给配置文件用的，
//! 更不该被房间占用。这里每个会话一把 `Mutex`，注册表本身只在增删查时短暂持读锁。
//!
//! # 为什么 `rev` 在锁内推进、事件也在锁内发
//!
//! 顺序即正确性：前端靠 `rev` 去重与判序。若先解锁再发事件，两个并发更新可能
//! 以相反顺序到达。Tauri 的 `emit` 只是投递（不阻塞、不回调），持锁发是安全的。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use serde_json::json;

use crate::core::error::{AppError, AppResult};
use crate::core::events::{topic, EventSink};

use super::model::{RoomSession, RoomSnapshot};

pub struct RoomRegistry {
    sink: Arc<dyn EventSink>,
    sessions: RwLock<HashMap<String, Arc<Mutex<RoomSession>>>>,
}

impl RoomRegistry {
    pub fn new(sink: Arc<dyn EventSink>) -> Arc<Self> {
        Arc::new(Self {
            sink,
            sessions: RwLock::new(HashMap::new()),
        })
    }

    /// 登记一个新会话，并立刻广播它的首个快照。
    ///
    /// 顺手清掉已结束的旧会话：否则"建了又关"反复几十次会让表无限增长。
    pub fn insert(&self, session: RoomSession) -> AppResult<RoomSnapshot> {
        let snapshot = session.snapshot();
        let handle = Arc::new(Mutex::new(session));
        {
            let mut map = self
                .sessions
                .write()
                .map_err(|_| AppError::internal("会话表锁中毒"))?;
            map.retain(|_, h| {
                h.lock()
                    .map(|s| s.phase.is_active())
                    .unwrap_or(false)
            });
            if map.contains_key(&snapshot.session_id) {
                return Err(AppError::internal("会话 ID 冲突"));
            }
            map.insert(snapshot.session_id.clone(), handle);
        }
        self.emit_state(&snapshot);
        Ok(snapshot)
    }

    pub fn handle(&self, session_id: &str) -> AppResult<Arc<Mutex<RoomSession>>> {
        let map = self
            .sessions
            .read()
            .map_err(|_| AppError::internal("会话表锁中毒"))?;
        map.get(session_id)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("会话不存在: {}", session_id)))
    }

    pub fn snapshot(&self, session_id: &str) -> AppResult<RoomSnapshot> {
        let handle = self.handle(session_id)?;
        let session = handle
            .lock()
            .map_err(|_| AppError::internal("会话锁中毒"))?;
        Ok(session.snapshot())
    }

    /// 取全部会话句柄。
    ///
    /// **锁顺序约定**：这里只持注册表的**读锁**取 `Arc` 并立刻释放，之后才去取
    /// 会话锁。**不要**改成"持注册表锁时再取会话锁"——那会与 `insert`
    /// （写锁 + 会话锁）形成 ABBA，死锁。
    fn handles(&self) -> Vec<Arc<Mutex<RoomSession>>> {
        match self.sessions.read() {
            Ok(map) => map.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 全部会话（含已结束的），按创建时间升序。
    ///
    /// **代价**：会克隆每个会话的完整快照（含成员列表）。历史会话只给 UI 看，
    /// 数量有限；但别把它放在高频路径上——高频路径用 `list_active`。
    pub fn list(&self) -> Vec<RoomSnapshot> {
        let mut out: Vec<RoomSnapshot> = self
            .handles()
            .iter()
            .filter_map(|h| h.lock().ok().map(|s| s.snapshot()))
            .collect();
        out.sort_by_key(|s| s.created_at);
        out
    }

    /// 进行中的会话。主窗口的房间列表用这个。
    ///
    /// **只克隆活跃会话**：先在会话锁内判 `phase`，非活跃的直接跳过，
    /// 免得"每次列房间都把历史上所有房间克隆一遍"。
    pub fn list_active(&self) -> Vec<RoomSnapshot> {
        let mut out: Vec<RoomSnapshot> = self
            .handles()
            .iter()
            .filter_map(|h| {
                let session = h.lock().ok()?;
                if !session.phase.is_active() {
                    return None;
                }
                Some(session.snapshot())
            })
            .collect();
        out.sort_by_key(|s| s.created_at);
        out
    }

    /// 同一 `(适配器, 游戏)` 已有进行中的会话时返回它。
    ///
    /// 用途是**防双击**：用户连点两次"开始联机"不该开出两个房间，
    /// 而两个房间会各自抢同一个端口，表现为"第二个必然失败"。
    pub fn find_active(&self, adapter_id: &str, game_id: Option<&str>) -> Option<RoomSnapshot> {
        self.list_active()
            .into_iter()
            .find(|s| s.adapter_id == adapter_id && s.game_id.as_deref() == game_id)
    }

    /// 变更状态并广播。`rev` 在锁内推进，事件也在锁内发出。
    pub fn update<F>(&self, session_id: &str, mutate: F) -> AppResult<RoomSnapshot>
    where
        F: FnOnce(&mut RoomSession),
    {
        self.update_if(session_id, |s| {
            mutate(s);
            true
        })
    }

    /// **只在真的变了**时才推进 `rev` 并广播。
    ///
    /// 闭包返回 `true` 表示"我改了状态"；返回 `false` 时快照原样返回——
    /// `rev` 不动、事件不发。高频轮询靠它避免把 `rev` 刷成噪音，
    /// 也让"已结束的会话不被后台写回"这条守卫能真正生效。
    pub fn update_if<F>(&self, session_id: &str, mutate: F) -> AppResult<RoomSnapshot>
    where
        F: FnOnce(&mut RoomSession) -> bool,
    {
        let handle = self.handle(session_id)?;
        let mut session = handle
            .lock()
            .map_err(|_| AppError::internal("会话锁中毒"))?;
        if !mutate(&mut session) {
            return Ok(session.snapshot());
        }
        session.touch();
        let snapshot = session.snapshot();
        self.emit_state(&snapshot);
        Ok(snapshot)
    }

    /// 推送过程提示。可丢：前端只把它当"正在做什么"的临时文案。
    pub fn progress(&self, session_id: &str, stage: &str, message: &str) {
        let Ok(handle) = self.handle(session_id) else {
            return;
        };
        let (seq, role) = {
            let Ok(mut session) = handle.lock() else {
                return;
            };
            (session.next_seq(), session.role)
        };
        self.sink.emit(
            topic::ROOM_PROGRESS,
            json!({
                "sessionId": session_id,
                "seq": seq,
                "stage": stage,
                "message": message,
                "role": role.as_str(),
            }),
        );
    }

    /// 移除会话。返回是否真的移除了。
    ///
    /// **必须先广播终态快照再移除**：移除后 `snapshot` / `update` 一律返回
    /// "会话不存在"，前端就再也拿不到那个"已断开"的终态了。
    pub fn remove(&self, session_id: &str) -> bool {
        match self.sessions.write() {
            Ok(mut map) => map.remove(session_id).is_some(),
            Err(_) => false,
        }
    }

    fn emit_state(&self, snapshot: &RoomSnapshot) {
        self.sink.emit(
            topic::ROOM_STATE,
            json!({
                "sessionId": snapshot.session_id,
                "rev": snapshot.rev,
                "snapshot": snapshot,
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::NoopSink;
    use crate::domain::room::model::{Phase, Role};

    fn registry() -> Arc<RoomRegistry> {
        RoomRegistry::new(Arc::new(NoopSink))
    }

    fn session(adapter: &str) -> RoomSession {
        RoomSession::new(Role::Host, adapter.to_string(), None)
    }

    #[test]
    fn update_bumps_rev_and_is_visible_to_list() {
        let reg = registry();
        let snap = reg.insert(session("a")).unwrap();
        assert_eq!(snap.rev, 1);

        let updated = reg
            .update(&snap.session_id, |s| {
                s.phase = Phase::Online;
                s.room_code = "CODE".into();
            })
            .unwrap();
        assert_eq!(updated.rev, 2);
        assert_eq!(updated.room_code, "CODE");
        assert_eq!(reg.list_active().len(), 1);
    }

    #[test]
    fn closed_sessions_are_not_active_and_get_pruned_on_insert() {
        let reg = registry();
        let first = reg.insert(session("a")).unwrap();
        reg.update(&first.session_id, |s| s.phase = Phase::Closed)
            .unwrap();
        assert!(reg.list_active().is_empty());

        reg.insert(session("b")).unwrap();
        assert_eq!(reg.list().len(), 1, "已结束的会话应在下次登记时清掉");
    }

    #[test]
    fn find_active_matches_adapter_and_game() {
        let reg = registry();
        reg.insert(session("adapter.x")).unwrap();
        assert!(reg.find_active("adapter.x", None).is_some());
        assert!(reg.find_active("adapter.y", None).is_none());
    }

    #[test]
    fn unknown_session_is_not_found() {
        let reg = registry();
        let err = reg.snapshot("nope").unwrap_err();
        assert_eq!(err.code, "not-found");
    }

    #[test]
    fn update_if_does_not_advance_rev_when_nothing_changed() {
        let reg = registry();
        let snap = reg.insert(session("a")).unwrap();
        let same = reg.update_if(&snap.session_id, |_| false).unwrap();
        assert_eq!(same.rev, snap.rev, "没有变化时 rev 不该推进");
    }

    #[test]
    fn remove_drops_the_session_from_the_registry() {
        let reg = registry();
        let snap = reg.insert(session("a")).unwrap();
        assert!(reg.remove(&snap.session_id));
        assert!(reg.snapshot(&snap.session_id).is_err());
        assert!(reg.list().is_empty());
    }
}
