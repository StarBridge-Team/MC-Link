use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// `Setting/account.yml` 的磁盘结构。
///
/// `extra` 用 `flatten` 收下所有未知键：这个文件在 0.5.x 时代由前端经
/// `save_setting` 整段改写，可能残留别的键。**不认识不等于可以删**——
/// 丢掉它们等于悄悄改用户配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountFile {
    pub player_name: String,
    /// 离线模式 UUID。为空时由 [`super::uuid::offline_uuid`] 现算。
    pub uuid: String,
    /// 首次注册时间（epoch ms）。0 表示老数据，没有记录。
    pub created_at: u64,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// 前端契约（camelCase）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineAccount {
    pub name: String,
    pub uuid: String,
    pub created_at: u64,
}

impl OfflineAccount {
    pub fn from_file(file: &AccountFile, uuid: String) -> Self {
        Self {
            name: file.player_name.trim().to_string(),
            uuid,
            created_at: file.created_at,
        }
    }
}
