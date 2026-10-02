//! 构建脚本。
//!
//! 除 Tauri 自身的构建步骤外，这里还统一负责 **Windows 应用清单** 的嵌入。
//!
//! # 背景
//!
//! `tauri-build` 通过 `tauri-winres` → `embed-resource` 编译资源，而
//! `embed-resource` 只会输出 `cargo:rustc-link-arg-bins=...`，也就是说应用清单
//! 只被链接进 **bin**。测试目标（`cargo test --lib` 的单元测试二进制）拿不到清单，
//! Windows 就会把 `comctl32.dll` 绑定到 `System32` 下的 5.82 版本；而
//! `tauri-runtime-wry` 的错误对话框用到的 `TaskDialogIndirect` 只存在于 v6
//! 并排程序集里。后果是测试进程在**加载阶段**直接失败，一条测试都跑不起来：
//!
//! ```text
//! process didn't exit successfully: ...exe (exit code: 0xc0000139, STATUS_ENTRYPOINT_NOT_FOUND)
//! ```
//!
//! # 做法
//!
//! 关掉 `tauri-build` 自带的清单嵌入，改由本脚本用 `rustc-link-arg`（作用于**所有**
//! 目标）嵌入仓库内的 `windows-app.manifest`。这样 bin 与测试目标拿到的是同一份
//! 清单，也不会出现"同一目标嵌两份清单"的链接冲突。
//!
//! 图标与版本信息仍由 `tauri-winres` 编译进 `.res` 并链接到 bin，未受影响。

fn main() {
    emit_build_channel();

    let attributes = if is_windows_target() {
        embed_app_manifest();
        // 清单由本脚本统一嵌入，避免 bin 侧重复嵌入。
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        tauri_build::Attributes::new()
    };

    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("tauri-build 执行失败: {e}");
    }
}

/// 把"这份二进制是不是官方发布构建"的信息编译进程序。
///
/// 自动更新会替换应用自身，因此客户端必须先知道自己在被谁替换：
/// 开发构建与用户自行编译的版本都不允许被自动更新覆盖（详见 `src/build_channel.rs`）。
///
/// 这里只做"转发"：把 `PROFILE` 与外部注入的 `MC_LINK_BUILD_CHANNEL` 原样交给运行时，
/// 判定规则留在 Rust 代码里，这样它有单测覆盖，而不是埋在构建脚本里。
fn emit_build_channel() {
    // 环境变量变化时重新执行本脚本，从而刷新 rustc-env
    println!("cargo:rerun-if-env-changed=MC_LINK_BUILD_CHANNEL");
    println!("cargo:rerun-if-env-changed=PROFILE");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    // 发布流程（scripts/tauri-build.mjs）会注入 official；其他情况为空
    let stamped = std::env::var("MC_LINK_BUILD_CHANNEL").unwrap_or_default();

    println!("cargo:rustc-env=MC_LINK_PROFILE={profile}");
    println!("cargo:rustc-env=MC_LINK_STAMPED_CHANNEL={stamped}");
}

fn is_windows_target() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
}

/// 为所有目标（含测试）嵌入应用清单。
fn embed_app_manifest() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app.manifest");
    if !manifest.is_file() {
        panic!("缺少 Windows 应用清单 {}，构建无法继续", manifest.display());
    }

    println!("cargo:rerun-if-changed=windows-app.manifest");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
