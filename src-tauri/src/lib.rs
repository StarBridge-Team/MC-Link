mod state;
mod adapter;
mod utils;
mod terracotta_client;
mod account;
mod commands;
use commands::*;
mod tray;
mod assets;
mod asset_server;
mod datadir;
mod cache;
mod effect;
mod page;
mod update;
mod config;
mod mgr;
mod downloader;
mod deep_link;
mod setting_meta;
mod m3;
use m3::commands::*;

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;
use state::AppState;
use datadir::resolve_data_dir;
use adapter::AdapterManager;
use mgr::AppMgr;

fn launch_adapters(app: &tauri::App, data_dir: &std::path::Path) {
    let manager = AdapterManager::new(data_dir);
    app.manage(Arc::new(Mutex::new(manager)));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

            let mgr = Arc::new(AppMgr::new(data_dir.clone())?);
            app.manage(mgr.clone());

            // P2P 连接管理器（基于 wgp-core）
            app.manage(Arc::new(mgr::connection::ConnectionManager::new()));

            if let Err(e) = config::check::check_config_version(&data_dir) {
                eprintln!("[配置检查] {}", e);
            }

            tray::setup_tray(app.handle())?;
            launch_adapters(app, &data_dir);
            effect::setup_window_effects(app);

            // 注册 mclink:// 深度链接
            let result = deep_link::register_scheme();
            if !result.success {
                eprintln!("[深度链接] {}", result.message);
            }

            #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
            {
                if let Err(e) = app.deep_link().register_all() {
                    eprintln!("[深度链接] 插件注册失败: {}", e);
                }
            }

            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                deep_link::handle_deep_link(&handle, &event.urls());
            });

            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

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
                                        if let Some(window) = app_handle.get_webview_window("main") {
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
        })
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
            account_login,
            account_verify,
            account_ping,
            desktop_login_init,
            desktop_login_poll,
            account_get_me,
            account_get_avatar,
            init_app,
            prepare_app,
            get_asset_url,
            get_assets_server_url,
            get_page_manifest,
            get_page_content,
            clear_page_cache_command,
            check_update_command,
            download_update_command,
            clear_update_cache_command,
            get_setting_meta,
            get_setting_manifest,
            clear_setting_meta_cache_command,
            start_p2p_connection,
            stop_p2p_connection,
            get_p2p_status,
            generate_m3_scheme,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
