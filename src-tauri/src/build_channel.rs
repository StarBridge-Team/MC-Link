//! 构建渠道——这份二进制是"开发构建 / 官方发布 / 自行构建"，以及据此决定的
//! **自动更新许可**。
//!
//! # 为什么必须有这个概念
//!
//! 自动更新会**替换应用自身**。少了渠道判定，会出现两类事故：
//!
//! 1. **开发构建被替换**：`pnpm tauri dev` 或 debug 编译跑在 `target/debug` 下，
//!    一旦被换成发布版，调试环境当场报废；而且开发构建随时在变，
//!    "当前是否是最新版本"这个判断对它根本没有意义。
//! 2. **自行构建的版本被静默替换**：用户或第三方从源码编出来的 release，
//!    被官方包覆盖等于抹掉别人的构建成果（也可能覆盖掉他们自己的修改）。
//!
//! 因此约定：**只有由本项目发布流程产出的 release 构建才允许自动更新**——
//! `scripts/tauri-build.mjs` 在以发布流程调用 `tauri build` 时会注入
//! `MC_LINK_BUILD_CHANNEL=official`，经 `build.rs` 转发到这里。
//!
//! 其余渠道一律**只提供"发现新版本 + 手动下载链接"**，绝不自动替换文件。
//!
//! # 判定优先级
//!
//! `debug` 优先于标记：即使带着 official 标记，debug 构建也仍被视为开发构建。
//! 否则 `cargo build` 时残留的环境变量就能把开发环境变成可自更新目标。

use serde::Serialize;

/// 构建渠道。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildChannel {
    /// 开发构建：debug 编译（含 `pnpm tauri dev`）。
    Dev,
    /// 官方发布构建：由 `scripts/tauri-build.mjs` 的发布流程产出。
    Official,
    /// 自行构建：release 编译，但未经发布流程标记
    /// （本机 `cargo build --release`、`pnpm tauri build --no-release`、fork 自行编译等）。
    SelfBuilt,
}

impl BuildChannel {
    /// 机器可读标识（与前端 `build_channel` 字段一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            BuildChannel::Dev => "dev",
            BuildChannel::Official => "official",
            BuildChannel::SelfBuilt => "self-built",
        }
    }

    /// 面向用户的中文说明。
    pub fn display_name(&self) -> &'static str {
        match self {
            BuildChannel::Dev => "开发构建",
            BuildChannel::Official => "官方版本",
            BuildChannel::SelfBuilt => "自行构建的版本",
        }
    }
}

/// 开发期强制开启自动更新的环境变量名。
///
/// 只对**开发构建**生效：它存在的意义是调试更新流程本身。
/// 自行构建的 release 版本不认这个变量——那类二进制可能已经分发给别人，
/// 不能让一个环境变量就绕过自更新限制。
pub const OVERRIDE_ENV: &str = "MC_LINK_UPDATE_OVERRIDE";

/// 依据构建脚本转发的两个原始输入判定渠道（纯函数，便于单测）。
fn resolve_channel(stamped: &str, profile: &str) -> BuildChannel {
    if profile == "debug" {
        return BuildChannel::Dev;
    }
    if stamped == "official" {
        BuildChannel::Official
    } else {
        BuildChannel::SelfBuilt
    }
}

/// 本次构建的渠道。
pub fn channel() -> BuildChannel {
    resolve_channel(
        option_env!("MC_LINK_STAMPED_CHANNEL").unwrap_or(""),
        option_env!("MC_LINK_PROFILE").unwrap_or("release"),
    )
}

/// 是否允许自动替换程序文件（纯函数，便于单测）。
pub fn update_allowed_for(channel: BuildChannel, override_enabled: bool) -> bool {
    match channel {
        BuildChannel::Official => true,
        BuildChannel::Dev => override_enabled,
        BuildChannel::SelfBuilt => false,
    }
}

/// 当前进程是否允许自动更新。
pub fn update_allowed() -> bool {
    update_allowed_for(channel(), override_requested())
}

/// 用户是否通过环境变量要求强制开启。
fn override_requested() -> bool {
    std::env::var(OVERRIDE_ENV)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_profile_is_dev_even_with_official_stamp() {
        // 残留的 official 标记不能把开发环境变成可自更新目标
        assert_eq!(resolve_channel("official", "debug"), BuildChannel::Dev);
        assert_eq!(resolve_channel("", "debug"), BuildChannel::Dev);
    }

    #[test]
    fn release_without_stamp_is_self_built() {
        assert_eq!(resolve_channel("", "release"), BuildChannel::SelfBuilt);
        assert_eq!(resolve_channel("dev", "release"), BuildChannel::SelfBuilt);
    }

    #[test]
    fn release_with_official_stamp_is_official() {
        assert_eq!(
            resolve_channel("official", "release"),
            BuildChannel::Official
        );
    }

    #[test]
    fn unknown_stamp_is_treated_as_self_built() {
        // 认不出的标记一律按"非官方"处理：宁可让用户手动更新，也不要误替换
        assert_eq!(
            resolve_channel("nightly", "release"),
            BuildChannel::SelfBuilt
        );
    }

    #[test]
    fn update_permission_truth_table() {
        assert!(update_allowed_for(BuildChannel::Official, false));
        assert!(update_allowed_for(BuildChannel::Official, true));
        assert!(!update_allowed_for(BuildChannel::Dev, false));
        assert!(update_allowed_for(BuildChannel::Dev, true));
        // 自构建版本不给环境变量口子
        assert!(!update_allowed_for(BuildChannel::SelfBuilt, false));
        assert!(!update_allowed_for(BuildChannel::SelfBuilt, true));
    }

    #[test]
    fn actual_build_agrees_with_profile() {
        // 保证 build.rs 转发链路真的生效：debug 与否必须与渠道判定一致
        assert_eq!(channel() == BuildChannel::Dev, cfg!(debug_assertions));

        // 两个输入都必须被 build.rs 转发进来，否则判定会悄悄退化成默认值
        //
        // 不用 `option_env!(..).expect(..)`：那是编译期常量上的 expect，
        // clippy 的 unconditional_panic 会（正确地）判定为必然恐慌点。
        let profile = option_env!("MC_LINK_PROFILE");
        assert!(profile.is_some(), "build.rs 未转发 PROFILE");
        assert!(
            matches!(profile, Some("debug") | Some("release")),
            "PROFILE 取值异常: {profile:?}"
        );
        assert!(
            option_env!("MC_LINK_STAMPED_CHANNEL").is_some(),
            "build.rs 未转发构建标记"
        );
    }

    #[test]
    fn reports_channel_for_diagnostics() {
        // 排查"为什么这个构建不能自动更新"时执行：
        //   cargo test --release reports_channel_for_diagnostics -- --nocapture
        println!("build_channel = {}", channel().as_str());
        println!("update_allowed = {}", update_allowed());
    }
}
