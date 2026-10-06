use super::PersonalizationSettings;
use crate::datadir::{background_dir, old_background_dir};
use crate::mgr::AppMgr;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub(crate) struct BackgroundFile {
    pub(crate) name: String,
    pub(crate) is_video: bool,
}

#[tauri::command]
pub(crate) fn get_setting(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    section: String,
) -> Result<String, String> {
    mgr.read().setting(&section)
}

#[tauri::command]
pub(crate) fn save_setting(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    section: String,
    content: String,
) -> Result<(), String> {
    mgr.write().setting(&section, &content)
}

#[tauri::command]
pub(crate) fn get_personalization(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<PersonalizationSettings, String> {
    mgr.read().personalization()
}

#[tauri::command]
pub(crate) fn save_personalization(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    settings: PersonalizationSettings,
) -> Result<(), String> {
    mgr.write().personalization(settings)
}

#[tauri::command]
pub(crate) fn get_default_effect() -> String {
    crate::effect::get_default_effect()
}

#[tauri::command]
pub(crate) fn get_background_files(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<Vec<BackgroundFile>, String> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let bg_dir = background_dir(mgr.data_dir());
    read_bg_files(&bg_dir, &mut files, &mut seen);

    if let Some(old_bg) = old_background_dir() {
        if old_bg != bg_dir {
            read_bg_files(&old_bg, &mut files, &mut seen);
        }
    }

    Ok(files)
}

fn read_bg_files(
    bg_dir: &std::path::Path,
    files: &mut Vec<BackgroundFile>,
    seen: &mut std::collections::HashSet<String>,
) {
    if !bg_dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(bg_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    let is_video =
                        matches!(ext.as_str(), "mp4" | "webm" | "avi" | "mov" | "mkv" | "flv");
                    let is_image = matches!(
                        ext.as_str(),
                        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg"
                    );
                    if is_image || is_video {
                        if let Some(name) = path.file_name() {
                            let name = name.to_string_lossy().to_string();
                            if seen.insert(name.clone()) {
                                files.push(BackgroundFile { name, is_video });
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tauri::command]
pub(crate) fn get_background_file_url(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    filename: String,
) -> Result<String, String> {
    // 只接受纯文件名。这个命令的参数来自前端，而它会把路径回传给
    // `convertFileSrc` 去渲染（CSP 的 `asset:` 源对任意绝对路径都成立），
    // 所以不收口就等于开了一条任意文件读取通道：`../../Windows/win.ini`
    // 或 `C:\Windows\win.ini` 都能被回传出去。
    let name = sanitize_background_name(&filename)?;

    for dir in [Some(background_dir(mgr.data_dir())), old_background_dir()]
        .into_iter()
        .flatten()
    {
        let full = dir.join(&name);
        // canonicalize + 包含性校验：连"目录里放了指向别处的软链接"一起挡掉
        if let (Ok(real), Ok(base)) = (full.canonicalize(), dir.canonicalize()) {
            if real.is_file() && real.starts_with(&base) {
                return Ok(real.to_string_lossy().to_string());
            }
        }
    }

    Err(format!("文件不存在: {}", name))
}

/// 把一个网络背景地址交给后端：下载到本地缓存、同时提取种子色。
///
/// # 为什么必须由后端做
///
/// 若前端把 URL 直接交给浏览器，后端拿不到字节就**无法取色**；而且随机图片 API 每次
/// 请求返回的图都不同，会出现"取色用 A 图、显示用 B 图"，配色与背景对不上。
/// 由后端先落成文件，取色与显示读的是**同一个文件**，从根上消除不一致。
///
/// 返回本地文件的绝对路径（前端再用 `convertFileSrc` 转成可加载的 URL）与种子色。
/// `seed` 为 `None` 表示下载成功但取色失败（例如视频、或不支持解码的格式）——
/// 这不该让整个操作失败，前端回退到手选主题色即可。
#[derive(Serialize)]
pub(crate) struct RemoteBackground {
    /// 本地缓存文件绝对路径。
    pub(crate) path: String,
    /// 提取到的种子色（`#rrggbb`）；取不到为 `None`。
    pub(crate) seed: Option<String>,
    /// 命中了旧缓存（本次下载失败）时为 true。
    pub(crate) from_cache: bool,
}

#[tauri::command]
pub(crate) fn fetch_remote_background(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    url: String,
    is_video: bool,
) -> Result<RemoteBackground, String> {
    let url = url.trim();
    // 只接受 http(s)。这个命令会代表后端发起请求，放任其它 scheme
    // （`file://`、`data:`）等于把本机文件读取能力暴露给前端可控参数。
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("只支持 http/https 地址".to_string());
    }

    let outcome = mgr.background_cache().fetch(url, is_video)?;

    // 视频不参与取色：形态上就不该有种子色，而且解码整段视频只为配色并不划算。
    let seed = if is_video || outcome.from_cache && outcome.content_type.starts_with("video/") {
        None
    } else {
        // 取色失败只是没有种子色，回退到手选主题色即可，不该让整个下载失败。
        // （大栈调度在 `extract_seed_from_file` 内部统一处理，这里不必再包一层线程。）
        crate::m3::extract::extract_seed_from_file(&outcome.path).ok()
    };

    Ok(RemoteBackground {
        path: outcome.path.to_string_lossy().to_string(),
        seed,
        from_cache: outcome.from_cache,
    })
}

/// 从一张本地背景图提取 M3 种子色，用于"配色跟随背景图"。
///
/// # 数据来源必须与 `get_background_file_url` 同源
///
/// 这个命令会把文件**内容**读进来（比只回传路径更敏感），所以它复用同一套收口：
/// 先 `sanitize_background_name` 只接受纯文件名，再 `canonicalize` + 包含性校验。
/// 任何放松都会把"配置背景图"变成任意文件读取。
///
/// # 为什么拒绝 SVG
///
/// 背景图允许 `svg`（浏览器能直接渲染），但取色走的是 `image` crate，它不解析 SVG。
/// 与其抛一句难以理解的解码错误，不如明确告知——前端据此回退到用户手选的种子色。
#[tauri::command]
pub(crate) fn extract_background_seed(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    filename: String,
) -> Result<String, String> {
    let name = sanitize_background_name(&filename)?;

    if name.to_lowercase().ends_with(".svg") {
        return Err("SVG 不支持自动取色，请手动选择主题色".to_string());
    }

    for dir in [Some(background_dir(mgr.data_dir())), old_background_dir()]
        .into_iter()
        .flatten()
    {
        let full = dir.join(&name);
        if let (Ok(real), Ok(base)) = (full.canonicalize(), dir.canonicalize()) {
            if real.is_file() && real.starts_with(&base) {
                return crate::m3::extract::extract_seed_from_file(&real);
            }
        }
    }

    Err(format!("文件不存在: {}", name))
}

/// 校验背景图文件名：必须是纯文件名（不含分隔符、盘符、`..`）。
fn sanitize_background_name(filename: &str) -> Result<String, String> {
    let name = filename.trim();
    let plain = std::path::Path::new(name)
        .file_name()
        .map(|f| f == name)
        .unwrap_or(false);
    let ok = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && plain;
    if !ok {
        return Err(format!("非法的背景文件名: {}", filename));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::sanitize_background_name;

    #[test]
    fn background_name_rejects_traversal_and_absolute_paths() {
        // 该参数来自前端，且会被回传给 convertFileSrc 渲染 —— 不收口就是任意文件读取
        assert!(sanitize_background_name("../../Windows/win.ini").is_err());
        assert!(sanitize_background_name(r"..\..\win.ini").is_err());
        assert!(sanitize_background_name(r"C:\Windows\win.ini").is_err());
        assert!(sanitize_background_name("/etc/passwd").is_err());
        assert!(sanitize_background_name("sub/dir/a.png").is_err());
        assert!(sanitize_background_name("").is_err());
        assert!(sanitize_background_name("   ").is_err());
        assert!(sanitize_background_name(".").is_err());
        assert!(sanitize_background_name("..").is_err());
    }

    #[test]
    fn background_name_accepts_plain_file_names() {
        assert_eq!(
            sanitize_background_name("背景图.png").unwrap(),
            "背景图.png"
        );
        assert_eq!(sanitize_background_name("  a.jpg  ").unwrap(), "a.jpg");
        assert_eq!(
            sanitize_background_name("b (1).webp").unwrap(),
            "b (1).webp"
        );
    }
}
