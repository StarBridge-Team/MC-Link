//! 落地流程的单元测试。
//!
//! 单独成文件（Rust 2018 目录模块）：`install.rs` 本身已贴着行数上限，
//! 测试代码继续堆在里面只会把它顶破；分开也让"实现"与"验证"一眼分得开。
//!
//! 引用的是**父模块的私有函数**（`bak_path` / `staged_path` / `stage_payload` 等），
//! 所以必须留在 `install` 模块的子模块里（`super::*`）。

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mclink-install-{}-{}",
        tag,
        crate::plugin::crypto::random_hex(4)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn bak_path_keeps_extension_and_sits_next_to_exe() {
    let exe = Path::new("C:\\portable\\MC Link.exe");
    assert_eq!(bak_path(exe), Path::new("C:\\portable\\MC Link.exe.bak"));
}

#[test]
fn staged_path_sits_next_to_exe_and_keeps_exe_extension() {
    let exe = Path::new("C:\\portable\\MC Link.exe");
    assert_eq!(staged_path(exe), Path::new("C:\\portable\\MC Link.new.exe"));
}

/// `.bak` 是失败时唯一的退路：**没有成功标记就不许删**。
#[test]
fn bak_is_only_cleaned_after_a_successful_landing() {
    assert!(should_clean_bak(true));
    assert!(
        !should_clean_bak(false),
        "没有成功标记时必须保留 .bak（上次落地可能失败或被中断）"
    );
}

#[test]
fn scripts_keep_placeholders_filled_and_no_leftover_marker() {
    let (target, payload, staged, bak, log, ok, fail) = (
        Path::new("C:\\portable\\MC Link.exe"),
        Path::new("C:\\cache\\new.exe"),
        Path::new("C:\\portable\\MC Link.new.exe"),
        Path::new("C:\\portable\\MC Link.exe.bak"),
        Path::new("C:\\cache\\install.log"),
        Path::new("C:\\cache\\install.ok"),
        Path::new("C:\\cache\\install-failed.txt"),
    );
    for script in [
        portable_script(target, payload, staged, bak, log, ok, fail),
        installer_script(target, payload, log, ok, fail),
    ] {
        assert!(!script.contains("__"), "占位符未被全部替换: {}", script);
        assert!(script.contains(&std::process::id().to_string()));
        assert!(
            script.contains("Copy-Item") || script.contains("Start-Process"),
            "落地脚本应包含替换或安装动作"
        );
    }
    // 便携版必须备份旧 exe：替换失败时用户还能手动回退
    assert!(portable_script(target, payload, staged, bak, log, ok, fail)
        .contains("MC Link.exe.bak"));
}

/// 便携版落地脚本的三条硬要求：原子替换、失败回滚、失败后拉起原程序。
#[test]
fn portable_script_replaces_atomically_and_rolls_back() {
    let (target, payload, staged, bak, log, ok, fail) = (
        Path::new("C:\\portable\\MC Link.exe"),
        Path::new("C:\\cache\\new.exe"),
        Path::new("C:\\portable\\MC Link.new.exe"),
        Path::new("C:\\portable\\MC Link.exe.bak"),
        Path::new("C:\\cache\\install.log"),
        Path::new("C:\\cache\\install.ok"),
        Path::new("C:\\cache\\install-failed.txt"),
    );
    let script = portable_script(target, payload, staged, bak, log, ok, fail);
    // 不得再直接把更新包覆盖到目标 exe（旧实现就是这么做，出错会留下半截文件）
    assert!(
        !script.contains(&format!("Copy-Item -LiteralPath '{}'", payload.display())),
        "不得直接覆盖目标 exe，必须先落到暂存文件"
    );
    assert!(script.contains("File]::Replace"), "应使用原子替换");
    assert!(script.contains("MC Link.new.exe"), "应使用暂存文件");
    assert!(script.contains("restored from backup"), "失败时应回滚 .bak");
    assert!(script.contains("relaunched original"), "失败时应拉起原程序");
}

/// 失败标记要能被读回：PowerShell 写的是 UTF8 BOM，第一行是中文原因。
#[test]
fn failure_marker_round_trip() {
    let dir = temp_dir("fail-marker");
    let marker = marker_path(&dir, "install-failed.txt");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(
        &marker,
        "\u{feff}替换程序文件失败：磁盘空间不足\nC:\\portable\\MC Link.exe\n",
    )
    .unwrap();
    let failure = read_failure_marker(&dir).expect("应能读出失败原因");
    assert_eq!(failure.reason, "替换程序文件失败：磁盘空间不足");
    assert_eq!(failure.target, "C:\\portable\\MC Link.exe");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_failure_marker_means_no_failure() {
    let dir = temp_dir("no-fail-marker");
    assert!(read_failure_marker(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// 预检：更新包先复制到目标同目录的暂存文件，失败时目标位置保持原样。
#[test]
fn staging_copies_payload_next_to_target() {
    let dir = temp_dir("stage");
    let payload = dir.join("new.exe");
    std::fs::write(&payload, b"payload-bytes").unwrap();
    let staged = stage_payload(&dir.join("MC Link.exe"), &payload).unwrap();
    assert_eq!(staged, dir.join("MC Link.new.exe"));
    assert_eq!(std::fs::read(&staged).unwrap(), b"payload-bytes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ps_quote_escapes_single_quotes() {
    assert_eq!(
        ps_quote(Path::new("C:\\it's here\\a.exe")),
        "C:\\it''s here\\a.exe"
    );
}
