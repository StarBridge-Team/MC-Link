//! 更新相关的 Tauri 命令（命令层只做参数转交与结果映射，不含业务逻辑）。

use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;

use crate::build_channel::BuildChannel;
use crate::datadir::{exe_path, install_mode};
use crate::mgr::AppMgr;

use super::download::update_cache_dir;
use super::install::{apply_update, is_auto_install_supported};
use super::model::{
    CheckUpdateResult, DownloadUpdateResult, InstallUpdateResult, RuntimeInfo, UpdateAsset,
};
use super::{install_via_plugin, plugin_available, plugin_preferred};

/// 退出前的等待：让 invoke 的返回值先送达到前端，再关闭应用。
const EXIT_DELAY: Duration = Duration::from_millis(600);

/// 更新落地的渠道闸门：渠道不允许自动替换程序文件时给出明确原因。
///
/// # 为什么闸门要在命令入口
///
/// 两条落地路径的防护并不对等：
///
/// - **自研路径**（[`super::install`]）在 `apply_update` 里有第二道闸门，
///   明确写着"即使有人绕过界面直接调用命令，也不能让它替换掉开发构建或别人自构建的程序文件"；
/// - **官方插件路径**（[`super::plugin_updater`]）的 `install_via_plugin` 内部
///   只有 `updater.check()` → `download_and_install()`，**没有任何渠道判定**。
///   而 `plugins.updater.pubkey` 是配置里硬编码的真实公钥，插件在所有构建
///   （含 dev、自构建、便携版）里都被注册，`plugin_available()` 恒为真 ——
///   于是 Windows 安装版的 dev / 自构建会直接下载并安装官方包，替换掉开发构建。
///
/// 因此闸门放在**命令入口最前面**，对两条落地路径同时生效；
/// `apply_update` 里那道闸门保留，作为纵深防御。
///
/// # 判定口径
///
/// 与 [`crate::build_channel::update_allowed`] 完全一致：官方渠道恒允许；
/// 开发构建需 `MC_LINK_UPDATE_OVERRIDE=1`；自行构建任何情况都不允许。
/// Windows 便携版与官方渠道安装版的**路径选择**不受影响（仍由
/// `plugin_preferred()` 决定），这里只管"能不能落地"。
fn update_gate_error(channel: BuildChannel, allowed: bool) -> Option<String> {
    if allowed {
        return None;
    }
    Some(format!(
        "当前是{}，不会自动替换程序文件；请手动下载安装包",
        channel.display_name()
    ))
}

/// 更新落地的两条路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallRoute {
    /// 交给官方更新插件（安装版 / Linux / macOS）。
    Plugin,
    /// 走自研路径（便携版替换 exe，或插件失败后的兜底）。
    Builtin,
}

/// 决定这次更新怎么落地（纯函数，便于单测）。
///
/// 顺序很重要：**渠道闸门在最前面**——渠道不允许时直接报错，
/// 两条落地路径都到不了；否则官方插件优先，自研路径兜底。
fn resolve_install_route(
    channel: BuildChannel,
    allowed: bool,
    plugin_preferred: bool,
    plugin_available: bool,
) -> Result<InstallRoute, String> {
    if let Some(reason) = update_gate_error(channel, allowed) {
        return Err(reason);
    }
    Ok(if plugin_preferred && plugin_available {
        InstallRoute::Plugin
    } else {
        InstallRoute::Builtin
    })
}

/// 按当前安装形态与包类型，从服务器清单重新解析更新资产。
///
/// **只信 `kind`**：真实下载地址、文件名与 sha256 一律来自服务器清单
/// （`check_update` 会按 `platform` + 安装形态挑出唯一匹配的那份），
/// 前端传来的同名字段一律忽略。
async fn resolve_asset(mgr: &Arc<AppMgr>, kind: &str) -> Result<UpdateAsset, String> {
    let result = mgr.pull().check_update().await?;
    let latest = result
        .latest
        .ok_or_else(|| "当前已是最新版本，没有可下载的更新包".to_string())?;
    match latest.asset {
        Some(asset) if asset.kind == kind => Ok(asset),
        Some(asset) => Err(format!(
            "当前安装形态对应的更新包是 {}，与请求的 {} 不符",
            asset.kind, kind
        )),
        None => Err("没有与当前安装形态匹配的更新包（可能需要手动下载）".to_string()),
    }
}

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
    // 传入的 asset 只当"想装哪一种包"的意图，地址与哈希由后端重新解析
    let asset = resolve_asset(mgr.inner(), &asset.kind).await?;
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

    // 渠道闸门放在**命令入口**：两条落地路径（官方插件 / 自研）受同一条约束。
    //
    // 官方插件路径内部没有任何渠道判定（见 `plugin_updater::install_via_plugin`），
    // 少了这道闸门，dev / 自行构建的安装版会被官方包直接替换掉。
    // 判定口径与自研路径 `install::apply_update` 里的第二道闸门完全一致。
    let route = resolve_install_route(
        crate::build_channel::channel(),
        crate::build_channel::update_allowed(),
        plugin_preferred(),
        plugin_available(),
    )?;

    // 安装版与非 Windows：优先交给官方更新插件——它能正确处理 NSIS / AppImage / macOS，
    // 系统目录与注册表由官方实现打理。仅在配置了更新公钥时可用；
    // 失败则回退到自研路径（Windows 安装版仍能装上），不让用户卡在"更新不了"。
    if route == InstallRoute::Plugin {
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
    //
    // 资产必须由后端重新解析：这个命令最终会**覆盖主程序或运行安装器**，
    // 若照单全收前端给的 urls/file/sha256，webview 一旦被注入（XSS、前端供应链）
    // 就能让客户端装上攻击者的二进制 —— 而 SHA256 强校验在这里不提供任何真实性，
    // 因为期望值同样来自那份不可信输入。
    let asset = resolve_asset(mgr.inner(), &asset.kind).await?;

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
        sandbox: crate::runtime::sandbox_hint(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自行构建的版本：任何情况下都不允许自动替换程序文件。
    ///
    /// 这条是本次修复的核心——`plugins.updater.pubkey` 是硬编码的真实公钥，
    /// 官方插件在所有构建里都被注册，插件路径内部又没有渠道判定，
    /// 少了闸门就会把别人的自构建（甚至开发构建）直接换成官方包。
    #[test]
    fn self_built_is_never_allowed_to_install() {
        let err = resolve_install_route(BuildChannel::SelfBuilt, false, true, true)
            .expect_err("自行构建不允许自动替换程序文件");
        assert!(
            err.contains("不会自动替换程序文件"),
            "报错要说明原因: {err}"
        );

        // 闸门在最前面：即使插件可用且被优先，也不该走到插件路径
        assert!(resolve_install_route(BuildChannel::SelfBuilt, false, true, true).is_err());
        assert!(resolve_install_route(BuildChannel::SelfBuilt, false, true, false).is_err());
        assert!(resolve_install_route(BuildChannel::SelfBuilt, false, false, true).is_err());
    }

    /// 开发构建：默认不允许；显式开了 `MC_LINK_UPDATE_OVERRIDE` 才允许。
    #[test]
    fn dev_build_follows_the_override_flag() {
        // 不允许时直接报错，两条路径都到不了
        assert!(resolve_install_route(BuildChannel::Dev, false, true, true).is_err());
        // 允许后按原有规则选路径
        assert_eq!(
            resolve_install_route(BuildChannel::Dev, true, true, true).unwrap(),
            InstallRoute::Plugin
        );
        assert_eq!(
            resolve_install_route(BuildChannel::Dev, true, false, true).unwrap(),
            InstallRoute::Builtin
        );
    }

    /// 官方渠道：闸门放行，路径选择保持原样
    /// （Windows 安装版优先官方插件；便携版走自研路径）。
    #[test]
    fn official_channel_keeps_original_routing() {
        assert_eq!(
            resolve_install_route(BuildChannel::Official, true, true, true).unwrap(),
            InstallRoute::Plugin
        );
        // 便携版：plugin_preferred() 为 false → 自研路径
        assert_eq!(
            resolve_install_route(BuildChannel::Official, true, false, true).unwrap(),
            InstallRoute::Builtin
        );
        // 插件不可用（没配公钥）→ 自研路径兜底
        assert_eq!(
            resolve_install_route(BuildChannel::Official, true, true, false).unwrap(),
            InstallRoute::Builtin
        );
    }

    /// 闸门的判定必须与 `build_channel::update_allowed_for` 完全一致：
    /// 两处口径若漂移，自研路径的第二道闸门与命令入口的第一道会互相打架。
    #[test]
    fn gate_matches_build_channel_truth_table() {
        for channel in [
            BuildChannel::Official,
            BuildChannel::Dev,
            BuildChannel::SelfBuilt,
        ] {
            for override_enabled in [false, true] {
                let allowed = crate::build_channel::update_allowed_for(channel, override_enabled);
                let err = resolve_install_route(channel, allowed, true, true);
                assert_eq!(
                    err.is_ok(),
                    allowed,
                    "渠道 {channel:?}（override={override_enabled}）的判定应与 build_channel 一致"
                );
            }
        }
    }
}
