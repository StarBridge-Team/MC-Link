mod host;
mod client;
mod protocol;
mod lan;
mod central;
mod state;
mod adapter;
mod terracotta_client;
mod oauth;
#[macro_use]
mod commands;
use commands::*;
mod tray;

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::Manager;
use state::AppState;
use adapter::AdapterManager;
use oauth::OAuthState;

fn launch_adapters(app: &tauri::App) {
    let manager = AdapterManager::new();
    manager.launch_all();
    app.manage(Arc::new(Mutex::new(manager)));
}

fn init_oauth(app: &tauri::App) {
    let data_dir = app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&data_dir).ok();
    let oauth_state = OAuthState::new(data_dir);
    app.manage(Arc::new(oauth_state));
}

/// 仅在 Windows 11 和 macOS 上启用窗口毛玻璃效果
fn setup_window_effects(app: &tauri::App) {
    #[cfg(target_os = "windows")]
    {
        if is_windows_11() {
            if let Some(window) = app.get_webview_window("main") {
                apply_mica_backdrop(&window);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(window) = app.get_webview_window("main") {
            apply_vibrancy_backdrop(&window);
        }
    }
}

#[cfg(windows)]
fn is_windows_11() -> bool {
    #[repr(C)]
    struct OsVersionInfo {
        os_version_info_size: u32,
        major_version: u32,
        minor_version: u32,
        build_number: u32,
        platform_id: u32,
        csd_version: [u16; 128],
    }
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(lpVersionInformation: *mut OsVersionInfo) -> i32;
    }
    let mut info = OsVersionInfo {
        os_version_info_size: std::mem::size_of::<OsVersionInfo>() as u32,
        major_version: 0,
        minor_version: 0,
        build_number: 0,
        platform_id: 0,
        csd_version: [0; 128],
    };
    let ret = unsafe { RtlGetVersion(&mut info) };
    if ret >= 0 {
        // Windows 11 build number >= 22000
        info.build_number >= 22000
    } else {
        false
    }
}

#[cfg(windows)]
fn apply_mica_backdrop(window: &tauri::WebviewWindow) {
    use raw_window_handle::HasWindowHandle;
    if let Ok(handle) = window.window_handle() {
        if let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_raw() {
            let hwnd = win32.hwnd.get() as *mut std::ffi::c_void;
            unsafe {
                #[link(name = "dwmapi")]
                extern "system" {
                    fn DwmSetWindowAttribute(
                        hwnd: *mut std::ffi::c_void,
                        dwAttribute: u32,
                        pvAttribute: *const std::ffi::c_void,
                        cbAttribute: u32,
                    ) -> i32;
                }
                // DWMSBT_MAINWINDOW (Mica) = 2，比 Acrylic 更轻量且适配深色模式
                let backdrop_type: u32 = 2;
                DwmSetWindowAttribute(hwnd, 38, &backdrop_type as *const _ as *const _, 4);
            }
            // 触发窗口重绘使 DWM 属性变更生效
            unsafe {
                #[link(name = "user32")]
                extern "system" {
                    fn SetWindowPos(
                        hwnd: *mut std::ffi::c_void,
                        hwndInsertAfter: *mut std::ffi::c_void,
                        x: i32,
                        y: i32,
                        cx: i32,
                        cy: i32,
                        uFlags: u32,
                    ) -> i32;
                }
                const SWP_NOMOVE: u32 = 0x0002;
                const SWP_NOSIZE: u32 = 0x0001;
                const SWP_FRAMECHANGED: u32 = 0x0020;
                SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn apply_vibrancy_backdrop(window: &tauri::WebviewWindow) {
    use raw_window_handle::HasWindowHandle;
    if let Ok(handle) = window.window_handle() {
        if let raw_window_handle::RawWindowHandle::AppKit(ns) = handle.as_raw() {
            let ns_view = ns.ns_view.as_ptr();
            if !ns_view.is_null() {
                unsafe {
                    // 使用 objc 运行时创建 NSVisualEffectView 并添加到窗口
                    // 导入 objc 运行时函数
                    #[link(name = "objc")]
                    extern "system" {
                        fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
                        fn objc_msgSend(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void;
                        fn objc_getClass(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
                    }
                    use std::ffi::CString;

                    // 创建 NSVisualEffectView
                    let cls_name = CString::new("NSVisualEffectView").unwrap();
                    let cls = objc_getClass(cls_name.as_ptr());

                    let alloc_sel = sel_registerName(CString::new("alloc").unwrap().as_ptr());
                    let init_sel = sel_registerName(CString::new("initWithFrame:").unwrap().as_ptr());
                    let set_autoresizing_mask_sel = sel_registerName(CString::new("setAutoresizingMask:").unwrap().as_ptr());
                    let set_state_sel = sel_registerName(CString::new("setState:").unwrap().as_ptr());
                    let set_material_sel = sel_registerName(CString::new("setMaterial:").unwrap().as_ptr());
                    let view_add_subview_sel = sel_registerName(CString::new("addSubview:positioned:relativeTo:").unwrap().as_ptr());

                    // NSVisualEffectView 的 state 和 material 常量
                    // NSVisualEffectStateActive = 1, NSVisualEffectMaterialHUDWindow = 7
                    let effect_view = objc_msgSend(cls, alloc_sel);
                    // CGRectMake(0, 0, width, height) - use infinite bounds to fill window
                    let rect_sel = sel_registerName(CString::new("CGRectMake").unwrap().as_ptr());
                    let effect_view = objc_msgSend(effect_view, init_sel, 0.0f64, 0.0f64, 1.0f64, 1.0f64);

                    // setAutoresizingMask: NSViewWidthSizable | NSViewHeightSizable = 18
                    objc_msgSend(effect_view, set_autoresizing_mask_sel, 18usize);

                    // setState: NSVisualEffectStateActive = 1
                    objc_msgSend(effect_view, set_state_sel, 1usize);

                    // setMaterial: NSVisualEffectMaterialHUDWindow = 7
                    objc_msgSend(effect_view, set_material_sel, 7usize);

                    // 添加到 NSView 的最底层
                    objc_msgSend(ns_view, view_add_subview_sel, effect_view, -1i32, std::ptr::null_mut::<std::ffi::c_void>());
                }
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            current_room: Arc::new(Mutex::new(None)),
            is_running: Arc::new(AtomicBool::new(false)),
            latency_ms: Arc::new(Mutex::new(0)),
            stop_signal: Arc::new(Mutex::new(None)),
        })
        .setup(|app| {
            tray::setup_tray(app.handle())?;
            launch_adapters(app);
            init_oauth(app);
            setup_window_effects(app);

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
            get_relays,
            scan_lan_servers,
            get_latency,
            start_online,
            stop_online,
            minimize_window,
            maximize_window,
            close_window,
            exit_app,
            show_window,
            show_main_window,
            set_tray_size,
            ping_relay,
            get_ip_info,
            download_adapter,
            get_adapter_status,
            start_adapter,
            stop_adapter,
            get_terracotta_state,
            start_terracotta_host,
            start_terracotta_guest,
            get_app_version,
            get_tauri_version,
            get_players,
            check_room_exists,
            get_oauth_user,
            is_oauth_logged_in,
            get_oauth_config,
            save_oauth_config,
            oauth_login,
            oauth_logout,
            get_setting,
            save_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
