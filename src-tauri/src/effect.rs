use tauri::Manager;

/// 返回当前平台默认窗口效果。
pub fn get_default_effect() -> String {
    #[cfg(target_os = "macos")]
    { "hud_window".to_string() }
    #[cfg(windows)]
    { "mica".to_string() }
    #[cfg(not(any(target_os = "macos", windows)))]
    { "none".to_string() }
}

/// 在 Windows 11 / macOS 上应用窗口效果。
pub fn setup_window_effects(app: &tauri::App) {
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
        info.build_number >= 22000
    } else {
        false
    }
}

#[cfg(windows)]
fn apply_mica_backdrop(window: &tauri::WebviewWindow) {
    apply_mica_backdrop_typed(window, 2);
}

#[cfg(windows)]
pub(crate) fn apply_mica_backdrop_typed(window: &tauri::WebviewWindow, backdrop_type: u32) {
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
                DwmSetWindowAttribute(hwnd, 38, &backdrop_type as *const _ as *const _, 4);
            }
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

/// 设置窗口沉浸式深色模式（DWMWA_USE_IMMERSIVE_DARK_MODE = 20）。
/// Mica/Acrylic 背景的明暗由该属性决定，而非页面 CSS；
/// 不设置时跟随系统主题，导致"应用深色 + 系统浅色"时材质仍为浅色。
#[cfg(windows)]
pub(crate) fn set_immersive_dark_mode(window: &tauri::WebviewWindow, dark: bool) {
    use raw_window_handle::HasWindowHandle;
    if let Ok(handle) = window.window_handle() {
        if let raw_window_handle::RawWindowHandle::Win32(win32) = handle.as_raw() {
            let hwnd = win32.hwnd.get() as *mut std::ffi::c_void;
            let value: i32 = if dark { 1 } else { 0 };
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
                DwmSetWindowAttribute(hwnd, 20, &value as *const _ as *const _, 4);
            }
        }
    }
}

/// 记录已添加的 NSVisualEffectView，避免重复切换时视图堆叠、无法卸载。
#[cfg(target_os = "macos")]
static VIBRANCY_VIEW: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// 移除之前添加的 vibrancy 视图（切换到其他效果或重复应用时调用）。
#[cfg(target_os = "macos")]
pub(crate) fn remove_vibrancy_backdrop() {
    use std::sync::atomic::Ordering;
    let view = VIBRANCY_VIEW.swap(std::ptr::null_mut(), Ordering::SeqCst);
    if !view.is_null() {
        unsafe {
            #[link(name = "objc")]
            extern "system" {
                fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
                fn objc_msgSend(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void;
            }
            let sel_name = std::ffi::CString::new("removeFromSuperview").unwrap();
            let sel = sel_registerName(sel_name.as_ptr());
            objc_msgSend(view, sel);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn apply_vibrancy_backdrop(window: &tauri::WebviewWindow) {
    // 先移除旧视图，防止重复添加导致堆叠
    remove_vibrancy_backdrop();
    use raw_window_handle::HasWindowHandle;
    if let Ok(handle) = window.window_handle() {
        if let raw_window_handle::RawWindowHandle::AppKit(ns) = handle.as_raw() {
            let ns_view = ns.ns_view.as_ptr();
            if !ns_view.is_null() {
                unsafe {
                    #[link(name = "objc")]
                    extern "system" {
                        fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
                        fn objc_msgSend(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void;
                        fn objc_getClass(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
                    }
                    use std::ffi::CString;

                    let cls_name = CString::new("NSVisualEffectView").unwrap();
                    let cls = objc_getClass(cls_name.as_ptr());

                    let alloc_sel = sel_registerName(CString::new("alloc").unwrap().as_ptr());
                    let init_sel = sel_registerName(CString::new("initWithFrame:").unwrap().as_ptr());
                    let set_autoresizing_mask_sel = sel_registerName(CString::new("setAutoresizingMask:").unwrap().as_ptr());
                    let set_state_sel = sel_registerName(CString::new("setState:").unwrap().as_ptr());
                    let set_material_sel = sel_registerName(CString::new("setMaterial:").unwrap().as_ptr());
                    let view_add_subview_sel = sel_registerName(CString::new("addSubview:positioned:relativeTo:").unwrap().as_ptr());

                    let effect_view = objc_msgSend(cls, alloc_sel);
                    let effect_view = objc_msgSend(effect_view, init_sel, 0.0f64, 0.0f64, 1.0f64, 1.0f64);

                    objc_msgSend(effect_view, set_autoresizing_mask_sel, 18usize);
                    objc_msgSend(effect_view, set_state_sel, 1usize);
                    objc_msgSend(effect_view, set_material_sel, 7usize);
                    objc_msgSend(ns_view, view_add_subview_sel, effect_view, -1i32, std::ptr::null_mut::<std::ffi::c_void>());

                    VIBRANCY_VIEW.store(effect_view, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }
    }
}
