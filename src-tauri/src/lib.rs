mod adapter;
mod build_channel;
mod commands;
mod persist;
mod state;
mod utils;
use commands::*;
mod asset_server;
mod assets;
mod cache;
mod community;
mod config;
mod datadir;
mod deep_link;
mod downloader;
mod effect;
mod legal;
mod m3;
mod mgr;
mod plugin;
mod setting_meta;
mod setup;
mod tray;
mod action;
mod update;
use m3::commands::*;

use datadir::resolve_data_dir;
use mgr::AppMgr;
use plugin::PluginManager;
use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

/// 初始化插件子系统。
///
/// 同步部分（清单加载、内置插件注册、游戏画像）立即完成；需要网络的网关绑定
/// 交给后台任务，避免拖慢窗口启动。
fn init_plugin_subsystem(app: &tauri::App, data_dir: &std::path::Path) -> Result<(), String> {
    let manager = PluginManager::new(data_dir)?;
    manager.attach_app(app.handle().clone());
    app.manage(manager.clone());

    tauri::async_runtime::spawn(async move {
        if let Err(e) = manager.start().await {
            eprintln!("[插件] 网关启动失败，外部插件将不可用: {}", e);
        }
    });
    Ok(())
}

/// 重新执行所有打开动作（首页刷新按钮触发，用于重新扫描局域网游戏）。
#[tauri::command]
async fn run_open_actions(app: AppHandle) -> Result<(), String> {
    let mgr: Arc<crate::action::ActionManager> =
        app.state::<Arc<crate::action::ActionManager>>().inner().clone();
    mgr.run_open_actions(app).await;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();

    // 官方更新插件只有在配置了签名公钥时才有意义：它的签名校验不可关闭，
    // 没填 pubkey 就根本无法完成校验。因此没配就不注册，避免启动阶段报错，
    // 更新流程会自动回退到自研路径（见 update/plugin_updater.rs）。
    let updater_configured = update::plugin_configured(&context.config().plugins);
    // 记下来供"能否自动安装"与选资产使用：Linux/macOS 只有插件可用时才支持自更新
    update::plugin_record_configured(updater_configured);

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(AppState)
        .setup(|app| {
            let data_dir = resolve_data_dir(app);
            std::fs::create_dir_all(&data_dir).ok();

            // 清理上一次自动更新留下的旧 exe 备份与下载残留；此时旧进程必然已退出
            update::cleanup_leftovers(&data_dir);

            let mgr = Arc::new(AppMgr::new(data_dir.clone())?);
            app.manage(mgr.clone());

            if let Err(e) = config::check::check_config_version(&data_dir) {
                eprintln!("[配置检查] {}", e);
            }

            tray::setup_tray(app.handle())?;
            if let Err(e) = init_plugin_subsystem(app, &data_dir) {
                eprintln!("[插件] 子系统初始化失败: {}", e);
            }

            // 应用打开时动作管理器：注册内置打开动作（扫描局域网游戏）并在启动后执行。
            let action_mgr = Arc::new(crate::action::ActionManager::new());
            crate::action::register_builtin_open_actions(&action_mgr);
            app.manage(action_mgr.clone());
            let am = action_mgr.clone();
            let apph = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                am.run_open_actions(apph).await;
            });
            // 用已保存的设置预应用窗口效果，避免启动瞬间先闪一下平台默认效果
            let effect_name = mgr
                .read()
                .personalization()
                .map(|p| p.transparent_effect)
                .unwrap_or_else(|_| effect::get_default_effect());
            effect::setup_window_effects(app, &effect_name);

            // 登记 mclink:// 深链接。**是否登记由安装形态决定**（见 deep_link::should_register_scheme）：
            // 安装版由安装器/系统登记，程序再写一遍会在卸载或挪动安装目录后残留失效的协议项；
            // 便携版与 Linux 没有安装器可用，只能自己登记。
            //
            // 登记动作交给官方插件：它写的注册项是 `"<exe>" "%1"`，与插件自己解析命令行
            // 参数的约定一致。此前自写的实现写的是 `"<exe>" --deep-link "%1"`（两个参数），
            // 而插件只认"恰好一个 URL 参数"，便携版的深链接会被静默丢弃。
            if deep_link::should_register_scheme() {
                if let Err(e) = app.deep_link().register_all() {
                    eprintln!("[深链接] 协议登记失败: {}", e);
                }
            }

            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                deep_link::handle_deep_link(&handle, &event.urls());
            });

            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };

                let chord_pressed = Arc::new(AtomicBool::new(false));
                let chord_handler = chord_pressed.clone();

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app_handle, shortcut, event| {
                            if event.state() == ShortcutState::Pressed {
                                if shortcut.matches(Modifiers::ALT, Code::KeyM) {
                                    chord_handler.store(true, Ordering::SeqCst);
                                    let reset = chord_handler.clone();
                                    std::thread::spawn(move || {
                                        std::thread::sleep(Duration::from_secs(1));
                                        reset.store(false, Ordering::SeqCst);
                                    });
                                }
                                if shortcut.matches(Modifiers::ALT, Code::KeyO) {
                                    if chord_handler.load(Ordering::SeqCst) {
                                        if let Some(window) = app_handle.get_webview_window("main")
                                        {
                                            let _ = window.show();
                                            let _ = window.set_focus();
                                        }
                                    }
                                }
                            }
                        })
                        .build(),
                )?;

                let alt_m = Shortcut::new(Some(Modifiers::ALT), Code::KeyM);
                let alt_o = Shortcut::new(Some(Modifiers::ALT), Code::KeyO);
                if let Err(e) = app.global_shortcut().register(alt_m) {
                    eprintln!("[热键] ALT+M 注册失败(可能已被其他程序占用): {}", e);
                }
                if let Err(e) = app.global_shortcut().register(alt_o) {
                    eprintln!("[热键] ALT+O 注册失败(可能已被其他程序占用): {}", e);
                }
            }

            Ok(())
        });

    if updater_configured {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    let app = builder
        .invoke_handler(tauri::generate_handler![
            minimize_window,
            maximize_window,
            close_window,
            drag_window,
            exit_app,
            show_window,
            show_main_window,
            set_tray_size,
            resize_window,
            get_ip_info,
            download_adapter,
            adapter_startup_init,
            get_adapter_status,
            get_terracotta_state,
            start_terracotta_host,
            start_terracotta_guest,
            get_app_version,
            get_tauri_version,
            get_setting,
            save_setting,
            get_personalization,
            save_personalization,
            get_default_effect,
            set_window_effect,
            set_window_dark_mode,
            get_background_files,
            get_background_file_url,
            init_app,
            prepare_app,
            get_asset_url,
            read_asset_text,
            check_update_command,
            download_update_command,
            install_update_command,
            clear_update_cache_command,
            get_runtime_info_command,
            get_setup_state_command,
            complete_setup_command,
            update_setup_command,
            reset_setup_command,
            community_fetch_command,
            community_clear_cache_command,
            legal_fetch_command,
            accept_eula_command,
            set_first_game_command,
            game_recommend_command,
            get_setting_meta,
            get_setting_manifest,
            clear_setting_meta_cache_command,
            generate_m3_scheme,
            plugin_list,
            plugin_gateway,
            game_list,
            plugin_route_plan,
            plugin_set_enabled,
            plugin_set_grants,
            plugin_set_blocked,
            plugin_reload,
            run_open_actions,
        ])
        .build(context)
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(manager) = app_handle.try_state::<Arc<PluginManager>>() {
                manager.shutdown();
            }
        }
    });
}
