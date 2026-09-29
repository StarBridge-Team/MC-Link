//! 从 node_modules 同步前端资源到 Assets/
//!
//! dev 环境下 assets-server 工作目录为 `assets-server/`，
//! 检测到 `../node_modules` 存在时自动同步 bootstrap-icons 与 Poppins 字体，
//! 无需手动维护 Assets/ 下的字体图标文件。
//! prod 环境无 node_modules，跳过（由部署时准备 Assets/）。

use std::path::Path;

/// 若 node_modules 存在，从 npm 包同步资源到 `<root>/Assets/`
pub fn prepare_from_npm(root_dir: &Path) {
    let npm_dir = root_dir.join("../node_modules");
    if !npm_dir.exists() {
        return;
    }
    let assets_dir = root_dir.join("Assets");
    let _ = std::fs::create_dir_all(&assets_dir);
    sync_bootstrap_icons(&assets_dir, &npm_dir);
    sync_poppins(&assets_dir, &npm_dir);
}

/// 同步 bootstrap-icons：CSS（重写字体路径）+ woff2 + woff
fn sync_bootstrap_icons(assets_dir: &Path, npm_dir: &Path) {
    let src_dir = npm_dir.join("bootstrap-icons/font");
    if !src_dir.exists() {
        return;
    }
    let dst_dir = assets_dir.join("bootstrap-icons");
    let _ = std::fs::create_dir_all(&dst_dir);

    // 复制字体文件（npm 包里在 fonts/ 子目录，目标扁平化到 bootstrap-icons/ 下）
    for ext in &["woff2", "woff"] {
        let src = src_dir.join("fonts").join(format!("bootstrap-icons.{}", ext));
        let dst = dst_dir.join(format!("bootstrap-icons.{}", ext));
        if src.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }

    // 复制 CSS 并重写字体路径：./fonts/bootstrap-icons.woff2?xxx → ./bootstrap-icons.woff2
    let css_src = src_dir.join("bootstrap-icons.css");
    if let Ok(css) = std::fs::read_to_string(&css_src) {
        let rewritten = css
            .replace(
                "./fonts/bootstrap-icons.woff2?e34853135f9e39acf64315236852cd5a",
                "./bootstrap-icons.woff2",
            )
            .replace(
                "./fonts/bootstrap-icons.woff?e34853135f9e39acf64315236852cd5a",
                "./bootstrap-icons.woff",
            );
        let _ = std::fs::write(dst_dir.join("bootstrap-icons.css"), rewritten);
    }
    println!("[prepare] bootstrap-icons 已同步");
}

/// 同步 Poppins：读取 latin-*.css（normal 权重 300-700），复制 woff2/woff，合并为 poppins.css
fn sync_poppins(assets_dir: &Path, npm_dir: &Path) {
    let src_dir = npm_dir.join("@fontsource/poppins");
    if !src_dir.exists() {
        return;
    }
    let dst_dir = assets_dir.join("fonts");
    let _ = std::fs::create_dir_all(&dst_dir);

    let weights = ["300", "400", "500", "600", "700"];
    let mut combined = String::new();

    for w in &weights {
        let css_path = src_dir.join(format!("latin-{}.css", w));
        let css = match std::fs::read_to_string(&css_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // 解析 url(./files/xxx.woff2) / url(./files/xxx.woff)，复制文件并重写为 ./xxx
        let mut rewritten = css.clone();
        let mut idx = 0usize;
        while let Some(pos) = css[idx..].find("url(") {
            let start = idx + pos + "url(".len();
            let end = match css[start..].find(')') {
                Some(e) => start + e,
                None => break,
            };
            let raw = &css[start..end];
            let url = raw.trim_matches('\'').trim_matches('"');

            if let Some(filename) = url.strip_prefix("./files/") {
                let src_file = src_dir.join("files").join(filename);
                let dst_file = dst_dir.join(filename);
                if src_file.exists() {
                    let _ = std::fs::copy(&src_file, &dst_file);
                }
                rewritten = rewritten.replace(url, &format!("./{}", filename));
            }
            idx = end + 1;
        }

        combined.push_str(&rewritten);
        combined.push('\n');
    }

    if !combined.is_empty() {
        let _ = std::fs::write(dst_dir.join("poppins.css"), combined);
        println!("[prepare] Poppins 字体已同步");
    }
}
