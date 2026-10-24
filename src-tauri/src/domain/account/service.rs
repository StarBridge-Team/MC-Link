use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::persist;

use super::model::{AccountFile, OfflineAccount};
use super::uuid::offline_uuid;

/// 账号服务：`Setting/account.yml` 的**唯一**读写方。
///
/// 该文件不再允许经 `save_setting` 整段改写（命令层会拒绝），否则前端与这里
/// 各写一半，UUID 与名字会对不上——而这种不一致只会在联机时表现为"身份不被识别"。
pub struct AccountService {
    data_dir: PathBuf,
    /// 专用锁：与 `AppMgr::write_lock` 分开，避免账号读写被设置页的批量保存堵住。
    /// 前提是 `account.yml` 只由本服务写（见上面的约束）。
    lock: Mutex<()>,
}

impl AccountService {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            data_dir,
            lock: Mutex::new(()),
        })
    }

    fn path(&self) -> Result<PathBuf, String> {
        Ok(crate::datadir::setting_dir(&self.data_dir)?.join("account.yml"))
    }

    /// 读取当前账号。从未注册过时返回 `None`（而不是一个空名字的假账号）。
    pub fn get(&self) -> Result<Option<OfflineAccount>, String> {
        let file: AccountFile = persist::load_yaml(&self.path()?)?.value;
        if file.player_name.trim().is_empty() {
            return Ok(None);
        }
        // 老数据只有名字没有 UUID：现算一个即可，不必写盘（读操作不该产生写）。
        let uuid = if file.uuid.trim().is_empty() {
            offline_uuid(file.player_name.trim())
        } else {
            file.uuid.clone()
        };
        Ok(Some(OfflineAccount::from_file(&file, uuid)))
    }

    /// 注册 / 改名。同名重复注册返回既有账号（幂等，不改 UUID）。
    pub fn register(&self, name: &str) -> Result<OfflineAccount, String> {
        let name = name.trim();
        validate(name)?;

        let _guard = self.lock.lock().map_err(|_| "账号锁不可用".to_string())?;
        let path = self.path()?;
        let mut file: AccountFile = persist::load_yaml(&path)?.value;

        if file.player_name.trim() == name && !file.uuid.trim().is_empty() {
            return Ok(OfflineAccount::from_file(&file, file.uuid.clone()));
        }

        // 改名 = 换身份：UUID 必须跟着变，否则服务端会把两个人当成同一个。
        file.player_name = name.to_string();
        file.uuid = offline_uuid(name);
        file.created_at = now_ms();
        persist::save_yaml(&path, &file)?;

        Ok(OfflineAccount::from_file(&file, file.uuid.clone()))
    }
}

fn validate(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("玩家名不能为空".to_string());
    }
    if name.chars().count() > 32 {
        return Err("玩家名最多 32 个字符".to_string());
    }
    if name.chars().any(|c| c.is_control()) {
        return Err("玩家名不能包含控制字符".to_string());
    }
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_service() -> (Arc<AccountService>, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "mclink-account-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (AccountService::new(dir.clone()), dir)
    }

    #[test]
    fn register_then_get_round_trips() {
        let (svc, dir) = temp_service();
        assert!(svc.get().unwrap().is_none());

        let account = svc.register("  Steve  ").unwrap();
        assert_eq!(account.name, "Steve");
        assert_eq!(account.uuid, offline_uuid("Steve"));

        let loaded = svc.get().unwrap().unwrap();
        assert_eq!(loaded.uuid, account.uuid);
        assert_eq!(loaded.name, "Steve");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn re_register_same_name_keeps_uuid() {
        let (svc, dir) = temp_service();
        let first = svc.register("Steve").unwrap();
        let second = svc.register("Steve").unwrap();
        assert_eq!(first.uuid, second.uuid);
        assert_eq!(first.created_at, second.created_at, "同名注册应是幂等的");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rename_changes_uuid() {
        let (svc, dir) = temp_service();
        let first = svc.register("Steve").unwrap();
        let second = svc.register("Alex").unwrap();
        assert_ne!(first.uuid, second.uuid);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_blank_and_overlong_names() {
        let (svc, dir) = temp_service();
        assert!(svc.register("   ").is_err());
        assert!(svc.register(&"x".repeat(33)).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
