//! 系统配色来源：桌面壁纸的主色、系统强调色。
//!
//! 供"配色种子"的两个选项使用（`theme_seed_source` = `system_wallpaper` / `system_accent`）。
//!
//! # 壁纸路径：**刻意用 `wallpaper` crate**（跨平台兼容性优先）
//!
//! 这不是"随手找个库"，也不是没查过：**自己直接动系统 API 的代价更高**。
//!
//! 读"当前壁纸是哪个文件"没有跨平台统一接口——Windows 是 `SPI_GETDESKWALLPAPER`
//! 加注册表；Linux 上 GNOME 在 gsettings、KDE 在 plasma 的嵌套配置、XFCE 在 xfconf、
//! Wayland 下多数合成器还只写不读。自己写等于把"每个桌面环境 × 每个版本"的兼容性
//! 维护接过来，而那是长期债；`wallpaper` 是唯一把"读"也做了的跨平台 crate。
//!
//! 它的已知代价**明确接受**：带进 15 个 crate（`rust-argon2`、`blake2b_simd`、
//! `redox_users`、`winreg`…）、2021 年后停更。因此这里只用 `get()` 读一个路径，
//! 不碰它写壁纸那条真正会随系统演进失效的分支。
//!
//! # 取色不新写，也不引新库
//!
//! 拿到路径后复用 [`super::extract::extract_seed_from_file`]，与"跟随背景图"同一条
//! 流程（缩到 128px → Celebi 量化 → 打分）。两套取色逻辑迟早会给出不同的主色，
//! 那是最难解释的一类 bug。
//!
//! # 强调色：两段 `cfg`，不引主题引擎
//!
//! `native-theme` 能跨平台读强调色，但它是整套主题引擎（Windows 走 WinRT、
//! Linux 拉 ashpd、macOS 拉 objc2）。为了一个 RGB 值引进这套依赖不划算。
//! 读不到时**明确报错**，由前端回退到手选种子色——不猜一个近似色。

/// 系统壁纸的主色，`#rrggbb`。
///
/// 失败情形都会如实返回错误（前端据此回退手选色）：拿不到壁纸路径、
/// 路径指向的不是本地文件（部分系统把壁纸存在主题缓存里）、图片解码失败。
pub fn seed_from_wallpaper() -> Result<String, String> {
    let path = wallpaper::get().map_err(|e| format!("读取系统壁纸路径失败: {e}"))?;
    let path = std::path::PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("系统壁纸不是本地图片文件：{}", path.display()));
    }
    super::extract::extract_seed_from_file(&path)
}

/// 系统强调色，`#rrggbb`。原样使用，不做取色。
pub fn accent_color() -> Result<String, String> {
    platform::accent_color()
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Graphics::Dwm::DwmGetColorizationColor;

    /// 读 Windows 的强调色。
    ///
    /// 用 `DwmGetColorizationColor` 而不是注册表里的 `ColorizationColor`：前者是
    /// 合成器**当前实际在用**的颜色，后者在部分版本上是缓存值、用户改了强调色要
    /// 重启资源管理器才更新。返回值是 `0xAARRGGBB`，这里只取 RGB——alpha 是
    /// "标题栏着色浓度"，拿它当种子会得到一个偏灰的主题色。
    pub fn accent_color() -> Result<String, String> {
        let mut argb: u32 = 0;
        let mut opaque: windows_sys::core::BOOL = 0;
        let hr = unsafe { DwmGetColorizationColor(&mut argb, &mut opaque) };
        if hr != 0 {
            return Err(format!("读取系统强调色失败（HRESULT=0x{hr:08X}）"));
        }
        let r = (argb >> 16) & 0xff;
        let g = (argb >> 8) & 0xff;
        let b = argb & 0xff;
        Ok(format!("#{r:02x}{g:02x}{b:02x}"))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    /// 读 GNOME 的强调色。
    ///
    /// GNOME 47 起把强调色放进 `org.gnome.desktop.interface accent-color`，
    /// 但值是**名字**（`'blue'`）而不是色值，所以要自己映射成 libadwaita 的九色调色板。
    ///
    /// KDE / XFCE 没有等价的统一入口（KDE 在 `kdeglobals`、XFCE 在 xfconf），
    /// 这里不做：读不到就报错回退手选色，比猜一个颜色可控。
    pub fn accent_color() -> Result<String, String> {
        let output = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "accent-color"])
            .output()
            .map_err(|e| format!("调用 gsettings 失败: {e}"))?;
        if !output.status.success() {
            return Err("当前桌面环境读不到强调色（需要 GNOME 47+）".to_string());
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        let name = raw.trim().trim_matches('\'').to_ascii_lowercase();
        let hex = match name.as_str() {
            // libadwaita 的九个强调色。`default` 在未设置时返回，等同蓝色。
            "blue" | "default" => "#3584e4",
            "teal" => "#2190a4",
            "green" => "#3a944a",
            "yellow" => "#c88800",
            "orange" => "#ed5b00",
            "red" => "#e62d42",
            "pink" => "#d56199",
            "purple" => "#9141ac",
            "slate" => "#6f8396",
            other => return Err(format!("未识别的强调色名称: {other}")),
        };
        Ok(hex.to_string())
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    pub fn accent_color() -> Result<String, String> {
        Err("当前平台不支持读取系统强调色".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 强调色必须是配色引擎能吃的 `#rrggbb`。
    ///
    /// Windows 上这条路恒可用（Win8 起 DWM 一定在跑），所以不加 `ignore`：
    /// 它顺手守住"格式化成了 `#aarrggbb`"这类低级错误——那样种子会整体偏色。
    #[cfg(windows)]
    #[test]
    fn accent_color_is_parseable_hex() {
        let hex = accent_color().expect("Windows 上应能读到强调色");
        assert_eq!(hex.len(), 7, "应为 #rrggbb，实际 {hex}");
        assert!(
            crate::m3::parse_seed(&hex).is_some(),
            "强调色必须能被配色引擎解析，实际 {hex}"
        );
    }

    /// 壁纸取色冒烟：结果取决于**这台机器**装了哪张壁纸，CI（无桌面）上必然失败，
    /// 所以标 `ignore`，本机手测用：
    ///
    /// ```text
    /// cargo test --lib m3::system -- --ignored --nocapture
    /// ```
    ///
    /// 取不到不算失败（Wayland、壁纸存在主题缓存里都会这样），只打印原因——
    /// 前端本来就会回退到手选色。
    #[test]
    #[ignore = "依赖本机壁纸路径"]
    fn wallpaper_seed_smoke() {
        match seed_from_wallpaper() {
            Ok(seed) => {
                println!("系统壁纸种子色 = {seed}");
                assert_eq!(seed.len(), 7, "应为 #rrggbb，实际 {seed}");
            }
            Err(e) => println!("系统壁纸取色不可用（前端会回退手选色）: {e}"),
        }
    }
}
