//! EasyTier 发行产物的固定表：版本、文件名、SHA256、体积。
//!
//! # 为什么把校验值写死在代码里
//!
//! 引擎是从**上游 GitHub** 直接下载的可执行文件，下载完就要被启动。若无校验，
//! "发布资产被替换/被中间人改写"就等价于任意代码执行。资源服务器那份清单是个
//! 更好的信任锚点，但这条链路刻意不走它（见 `docs/ARCHITECTURE.md` 的取舍）：
//!
//! - 常量表随代码一起过审、一起发版，改它必须改代码；
//! - 升级引擎 = 改 [`VERSION`] 与整张表，不存在"清单在服务端悄悄变了"的路径。
//!
//! # 表里的哈希从哪来
//!
//! 取自 GitHub Releases API 的 `digest` 字段（由 GitHub 在接收上传时计算），
//! 并与实际下载结果做过一次交叉核对（`windows-x86_64` 本地下载后复算一致）。
//!
//! # 平台标签不统一
//!
//! EasyTier 自己的产物命名并不遵守"os-arch"统一规则：macOS 用 `aarch64`，
//! Windows 用 `arm64`，Linux 用 `armv7hf`。因此这里按 `(os, arch)` 精确匹配，
//! **不**复用 [`crate::core::platform`] 的翻译规则——那套规则服务的是更新清单，
//! 两者混用会在换架构时静默取错包。

/// 锁定的上游版本（与产物文件名里的版本号同源）。
pub const VERSION: &str = "v2.6.4";

/// 上游发行页的下载根（`<base>/<version>/<file>`）。
const RELEASE_BASE: &str = "https://github.com/EasyTier/EasyTier/releases/download";

/// 一个平台的发行包。
pub struct Artifact {
    /// 上游 release 里的文件名，同时作为落地文件名。
    pub file: &'static str,
    /// 小写十六进制 SHA256（下载后强制校验）。
    pub sha256: &'static str,
    /// 字节数：既作进度分母的兜底，也用来提前发现"下到一半"。
    pub size: u64,
}

const WINDOWS_X86_64: Artifact = Artifact {
    file: "easytier-windows-x86_64-v2.6.4.zip",
    sha256: "27af91e270e554709b048bd32327fefd2dfce5062ae1e8701af7550c6f525f84",
    size: 32_657_720,
};

const WINDOWS_ARM64: Artifact = Artifact {
    file: "easytier-windows-arm64-v2.6.4.zip",
    sha256: "37023f8a3451c9234b17ee2089a03dc344ce90d803b5b359cb6c46682b0549b4",
    size: 30_136_892,
};

const LINUX_X86_64: Artifact = Artifact {
    file: "easytier-linux-x86_64-v2.6.4.zip",
    sha256: "61b659eaedba658fa66fe47d17e1426cdd77e5d02fa15fed447bb4357c09dfd6",
    size: 25_506_523,
};

const MACOS_X86_64: Artifact = Artifact {
    file: "easytier-macos-x86_64-v2.6.4.zip",
    sha256: "89fc28a6e6995259d76ce3f11775220e8a21c760e94df91a6a9db30a69b6982e",
    size: 33_733_545,
};

const MACOS_AARCH64: Artifact = Artifact {
    file: "easytier-macos-aarch64-v2.6.4.zip",
    sha256: "4be1882d1aa36d31c1d6ba0596f2cf8a097e371f8da124212324b2e0f8df7e4b",
    size: 30_174_017,
};

/// 当前平台对应的产物；没有登记的平台返回 `None`。
///
/// 返回 `None` 时**不降级、不猜**：调用方必须给出"该平台没有可用引擎"的明确错误。
/// 猜错的后果是下载一个架构不符的可执行文件，然后在 `spawn` 时失败，
/// 用户看到的却是"联机启动失败"这种毫无指向性的提示。
pub fn current() -> Option<&'static Artifact> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => &WINDOWS_X86_64,
        ("windows", "aarch64") => &WINDOWS_ARM64,
        ("linux", "x86_64") => &LINUX_X86_64,
        ("macos", "x86_64") => &MACOS_X86_64,
        ("macos", "aarch64") => &MACOS_AARCH64,
        _ => return None,
    })
}

/// 上游直连下载地址（不含代理）。
pub fn origin_url(artifact: &Artifact) -> String {
    format!("{}/{}/{}", RELEASE_BASE, VERSION, artifact.file)
}

/// 平台标识，仅用于错误文案（"本平台（windows-x86_64）没有可用的 EasyTier 产物"）。
pub fn platform_label() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_artifact_is_self_consistent() {
        for artifact in [
            &WINDOWS_X86_64,
            &WINDOWS_ARM64,
            &LINUX_X86_64,
            &MACOS_X86_64,
            &MACOS_AARCH64,
        ] {
            assert_eq!(artifact.sha256.len(), 64, "{}", artifact.file);
            assert!(artifact.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert_eq!(
                artifact.sha256,
                artifact.sha256.to_ascii_lowercase(),
                "校验值必须是小写十六进制：{}",
                artifact.file
            );
            assert!(artifact.size > 0);
            // 文件名里的版本号必须与 VERSION 一致，否则升级时容易只改一处
            assert!(
                artifact.file.contains(VERSION),
                "{} 与 VERSION={} 不一致",
                artifact.file,
                VERSION
            );
        }
    }

    #[test]
    fn origin_url_points_at_the_release() {
        assert_eq!(
            origin_url(&WINDOWS_X86_64),
            "https://github.com/EasyTier/EasyTier/releases/download/v2.6.4/easytier-windows-x86_64-v2.6.4.zip"
        );
    }

    #[test]
    fn current_platform_is_mapped_or_explicitly_absent() {
        // 只要映射到了就必须是同一张表里的条目；没映射到则必须是 None（不猜）
        match current() {
            Some(a) => assert!(a.file.starts_with("easytier-")),
            None => assert!(!platform_label().is_empty()),
        }
    }
}
