//! 离线账号命令。
//!
//! 只有两个动作：读当前账号、注册/改名。UUID 由后端按离线模式算法生成，
//! **不接受前端传入**——否则身份标识就成了可以随便改的值。

use std::sync::Arc;

use crate::domain::account::{AccountService, OfflineAccount};

/// 当前账号；从未注册过时返回 `null`。
#[tauri::command]
pub(crate) fn account_get(
    accounts: tauri::State<'_, Arc<AccountService>>,
) -> Result<Option<OfflineAccount>, String> {
    accounts.get()
}

/// 注册或改名。同名重复调用是幂等的（UUID 不变）。
#[tauri::command]
pub(crate) fn account_register(
    accounts: tauri::State<'_, Arc<AccountService>>,
    name: String,
) -> Result<OfflineAccount, String> {
    accounts.register(&name)
}
