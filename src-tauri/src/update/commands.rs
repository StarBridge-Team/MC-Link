//! 更新相关的 Tauri 命令（命令层只做参数转交与结果映射，不含业务逻辑）。

use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;

use crate::datadir::{exe_path, install_mode};
use crate::mgr::AppMgr;

use super::download::update_cache_dir;
use super::install::{apply_update, is_auto_install_supported};
use super::{install_via_plugin, plugin_available, plugin_preferred};
use super::model::{
    CheckUpdateResult, DownloadUpdateResult, InstallUpdateResult, RuntimeInfo, UpdateAsset,
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

    // 安装版与非 Windows：优先交给官方更新插件——它能正确处理 NSIS / AppImage / macOS，
    // 系统目录与注册表由官方实现打理。仅在配置了更新公钥时可用；
    // 失败则回退到自研路径（Windows 安装版仍能装上），不让用户卡在"更新不了"。
    if plugin_preferred() && plugin_available() {
        match install_via_plugin(&app).await {
            Ok(version) => {
                // Windows 上插件会在安装前让应用自行退出；其他平台需要我们重启
                restart_after_delay(app, !cfg!(windows));
                return Ok(InstallUpdateResult {
                    restarting: true,
                    message: format!("已开始安装 {}，应用即将重启", version),
                });
            }
            Err(e) => {
                eprintln!("[更新] 官方插件落地失败，回退到自研路径: {}", e);
            }
        }
    }

    // 自研路径：便携版必然走这里（替换 exe）；Windows 安装版也作为插件的回退。
    // 复用下载结果：命中缓存时不会重新下载。
    let downloaded = mgr.pull().download_update(asset.clone(), |_, _| {}).await?;
    let payload = update_cache_dir(&data_dir).join(&downloaded.file);

    let message = apply_update(&data_dir, &asset, &payload)?;

    // 落地进程（或安装器）正在等待本进程退出，稍等片刻让前端先把"正在重启"渲染出来
    restart_after_delay(app, false);
    Ok(InstallUpdateResult {
        restarting: true,
        message,
    })
}

/// 延迟重启/退出：让 `invoke` 的返回值先送达前端。
///
/// `reexec = true` 时用 `restart()` 重新拉起（安装已由插件完成，需要换成新二进制）；
/// `false` 时只退出——由后台的落地进程负责重新启动应用。
fn restart_after_delay(app: tauri::AppHandle, reexec: bool) {
    std::thread::spawn(move || {
        std::thread::sleep(EXIT_DELAY);
        if reexec {
            app.restart();
        } else {
            app.exit(0);
        }
    });
}

/// 清理更新缓存。
///
/// 走 `pull()` 而不是 `write()`：它与"下载更新包"操作同一个目录，
/// 必须共用 `update_lock` 才能与下载互斥。用 `write_lock` 的话，
/// 边下载边点清理会把正在写的分片删掉（下载失败或留下半截文件）。
#[tauri::command]
pub(crate) async fn clear_update_cache_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<(), String> {
    mgr.pull().clear_update_cache().await
}

/// 查询当前运行环境（安装形态 + 构建渠道 + 自更新许可）。
///
/// 不联网，适合"关于"页首次渲染：界面据此显示"便携版 / 安装版"与
/// "官方版本 / 自行构建"，并决定给"一键更新"还是"手动下载"按钮。
#[tauri::command]
pub(crate) fn get_runtime_info_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<RuntimeInfo, String> {
    let exe = exe_path().ok_or_else(|| "无法定位当前可执行文件".to_string())?;

    Ok(RuntimeInfo {
        install_mode: install_mode().as_str().to_string(),
        build_channel: crate::build_channel::channel().as_str().to_string(),
        auto_install_supported: is_auto_install_supported(),
        update_allowed: crate::build_channel::update_allowed(),
        exe_path: exe.to_string_lossy().to_string(),
        data_dir: mgr.data_dir().to_string_lossy().to_string(),
    })
}
