//! 更新相关的 Tauri 命令（命令层只做参数转交与结果映射，不含业务逻辑）。

use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;

use crate::datadir::{exe_path, install_mode};
use crate::mgr::AppMgr;

use super::download::update_cache_dir;
use super::install::{apply_update, is_auto_install_supported};
use super::model::{
    CheckUpdateResult, DownloadUpdateResult, InstallModeInfo, InstallUpdateResult, UpdateAsset,
};

/// 退出前的等待：让 invoke 的返回值先送达到前端，再关闭应用。
const EXIT_DELAY: Duration = Duration::from_millis(600);

/// 检查更新。
///
/// 返回结果里带上 `install_mode` 与 `platform`，界面无需再问一次当前是便携版还是安装版。
#[tauri::command]
pub(crate) async fn check_update_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<CheckUpdateResult, String> {
    mgr.pull().check_update().await
}

/// 下载一个更新包到本地缓存（已缓存且校验通过时不会重复下载）。
///
/// 下载过程中通过 `update-progress` 事件上报进度：`{ downloaded, total }`，
/// `total` 为 0 表示长度未知。
#[tauri::command]
pub(crate) async fn download_update_command(
    app: tauri::AppHandle,
    mgr: tauri::State<'_, Arc<AppMgr>>,
    asset: UpdateAsset,
) -> Result<DownloadUpdateResult, String> {
    mgr.pull()
        .download_update(asset, move |downloaded, total| {
            let _ = app.emit(
                "update-progress",
                serde_json::json!({ "downloaded": downloaded, "total": total }),
            );
        })
        .await
}

/// 安装更新并重启应用。
///
/// 流程：确保包已下载并通过校验 → 交给后台进程执行落地 → **本进程退出**。
/// 对便携版是替换 exe，对安装版是运行安装器；两者都会在完成后重新拉起应用。
#[tauri::command]
pub(crate) async fn install_update_command(
    app: tauri::AppHandle,
    mgr: tauri::State<'_, Arc<AppMgr>>,
    asset: UpdateAsset,
) -> Result<InstallUpdateResult, String> {
    let data_dir = mgr.data_dir().clone();

    // 复用下载结果：命中缓存时不会重新下载
    let downloaded = mgr.pull().download_update(asset.clone(), |_, _| {}).await?;
    let payload = update_cache_dir(&data_dir).join(&downloaded.file);

    let message = apply_update(&data_dir, &asset, &payload)?;

    // 落地进程正在等待本进程退出；稍等片刻让前端先把"正在重启"渲染出来
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(EXIT_DELAY);
        handle.exit(0);
    });

    Ok(InstallUpdateResult {
        restarting: true,
        message,
    })
}

/// 清理更新缓存。
#[tauri::command]
pub(crate) fn clear_update_cache_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<(), String> {
    mgr.write().clear_update_cache()
}

/// 查询安装形态与自更新能力。
///
/// 界面用它显示"便携版 / 安装版"，并决定是给"一键更新"还是"手动下载"按钮。
#[tauri::command]
pub(crate) fn get_install_mode_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<InstallModeInfo, String> {
    let exe = exe_path().ok_or_else(|| "无法定位当前可执行文件".to_string())?;

    Ok(InstallModeInfo {
        install_mode: install_mode().as_str().to_string(),
        auto_install_supported: is_auto_install_supported(),
        exe_path: exe.to_string_lossy().to_string(),
        data_dir: mgr.data_dir().to_string_lossy().to_string(),
    })
}
