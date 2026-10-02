//! 社区信息（GitHub 贡献者与 Issues）相关的 Tauri 命令。

use std::sync::Arc;

use crate::community::Community;
use crate::mgr::AppMgr;

/// 读取社区信息。
///
/// **实际上永远不会返回 `Err`**：拉不到时给的是"空列表 + `stale` + `error`"，
/// 界面照常渲染、只在次要位置提示一句（关于页因为网络问题打不开是最没必要的失败，
/// 而未认证的 GitHub API 只有 60 次/小时/IP，限流是常态而非异常）。
///
/// 签名仍是 `Result`，因为 Tauri 宏要求"带引用入参的 async 命令"必须如此；
/// 前端把它当"总会成功"处理即可，`stale` 与 `error` 才是真实状态来源。
#[tauri::command]
pub(crate) async fn community_fetch_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<Community, String> {
    let mgr = mgr.inner().clone();
    Ok(mgr.pull().community().await)
}

/// 清理社区信息缓存（"重新拉取"入口）。下次读取会重新请求 GitHub。
#[tauri::command]
pub(crate) fn community_clear_cache_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<(), String> {
    mgr.write().clear_community_cache()
}
