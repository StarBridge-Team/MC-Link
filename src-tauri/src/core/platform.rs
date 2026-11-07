//! 平台标识：`<os>-<arch>`，用于在发布清单里挑选对应平台的产物。
//!
//! # 为什么是独立模块
//!
//! 这个字符串曾经挂在 `assets/adapter.rs`（陶瓦安装包）下，但它的消费者一直是两类：
//! 更新包清单（`update/fetch.rs`）与联机引擎产物（`engine/easytier/artifact.rs`）。
//! 挂在一个具体业务文件里，等于让"引擎挑包"依赖"陶瓦安装包"这个已删除的模块。
//!
//! # 架构标签必须与上游产物一致
//!
//! 上游（陶瓦、EasyTier）的发布产物用 `arm64` / `armv7` 命名架构，而 Rust 的
//! [`std::env::consts::ARCH`] 用 `aarch64` / `arm`。不映射就对不上包，
//! 表现为"清单里没有当前平台的产物"。操作系统的取值两边一致
//! （`windows` / `linux` / `macos` / `freebsd`），无需映射。
//!
//! 注意 EasyTier 自己的产物命名并不完全遵守这套标签（macOS 用 `aarch64`、
//! Windows 用 `arm64`），因此它另有一张显式的产物表，见
//! [`crate::engine::easytier::artifact`]——那张表才是权威，本模块只服务清单级通用标识。

/// 当前平台标识，形如 `windows-x86_64`。
pub fn current_platform() -> String {
    label(std::env::consts::OS, std::env::consts::ARCH)
}

/// 由操作系统与架构拼出平台标识（纯函数，便于单测覆盖各分支）。
pub fn label(os: &str, arch: &str) -> String {
    format!("{}-{}", os, upstream_arch_label(arch))
}

/// 把 Rust 的架构名映射成上游产物使用的架构标签。
fn upstream_arch_label(arch: &str) -> &str {
    match arch {
        "aarch64" => "arm64",
        "arm" => "armv7",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_matches_current_target() {
        // 形如 windows-x86_64 / linux-x86_64 / macos-arm64
        let p = current_platform();
        assert!(p.contains('-'), "平台标识应形如 <os>-<arch>，实际: {}", p);
    }

    #[test]
    fn arch_label_follows_upstream_artifact_naming() {
        // 上游产物用 arm64 / armv7，Rust 用 aarch64 / arm —— 不映射就对不上包
        assert_eq!(label("linux", "aarch64"), "linux-arm64");
        assert_eq!(label("macos", "aarch64"), "macos-arm64");
        assert_eq!(label("linux", "arm"), "linux-armv7");
        // 其余架构原样透传
        assert_eq!(label("windows", "x86_64"), "windows-x86_64");
        assert_eq!(label("linux", "x86"), "linux-x86");
        assert_eq!(label("linux", "riscv64"), "linux-riscv64");
        assert_eq!(label("linux", "loongarch64"), "linux-loongarch64");
    }

    #[test]
    fn windows_x86_64_platform_key_is_unchanged() {
        // 既有更新清单条目（windows-x86_64）必须继续有效
        assert_eq!(label("windows", "x86_64"), "windows-x86_64");
    }
}
