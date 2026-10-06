//! 运行环境的**封装形态**识别（是否运行在只读沙箱里）。
//!
//! # 为什么需要它
//!
//! Flatpak 把应用装在只读的 `/app` 里，运行时、库、以及应用自身都由 `flatpak`
//! 统一管理。这带来一个与"安装版/便携版"不同的约束：
//!
//! - **应用不能自己更新自己**：`/app` 是只读绑定，且直接改动它会绕开 OSTree
//!   的版本记录，导致 `flatpak update` 之后的校验失败。更新必须由 `flatpak update`
//!   完成（或经 Flathub/自建 remote 的部署）。
//! - **资源服务器兜底也不能用**：那条路径最终会写应用目录，在沙箱里必然失败。
//!
//! 所以遇到 Flatpak 时应当**直接告诉用户去用 flatpak 更新**，而不是让他在应用内
//! 点"一键更新"然后失败——那是最难排查的一类体验问题。
//!
//! # 检测方式
//!
//! Flatpak 会在沙箱内放置 `/.flatpak-info`（INI 格式），这是官方约定的探测方法，
//! 由 `flatpak` 自己写入，应用无法伪造（沙箱外的普通进程看不到它）。
//! 参考 flatpak 文档的 "Detecting Flatpak" 一节。

/// Flatpak 沙箱的标志文件（沙箱内恒定路径）。
///
/// 只在 Linux 上参与编译：非 Linux 平台的判定恒为 `false`，用不到这个常量，
/// 无条件定义会得到 dead_code 告警。
#[cfg(target_os = "linux")]
const FLATPAK_INFO: &str = "/.flatpak-info";

/// 当前进程是否运行在 Flatpak 沙箱中。
///
/// 非 Linux 平台直接返回 `false`（该路径不可能存在）。
pub fn is_flatpak() -> bool {
    // 只在 Linux 上判断：其它平台的 `/.flatpak-info` 要么不存在，要么不该被解读。
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(FLATPAK_INFO).is_file()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// 面向界面的运行封装说明；`None` 表示无需特殊提示（常规安装/便携形态）。
///
/// 目前只有 Flatpak 一种。将来若加入 Snap 等只读沙箱，在这里返回对应说明即可，
/// 前端不必知道具体是哪种。
pub fn sandbox_hint() -> Option<SandboxInfo> {
    if is_flatpak() {
        return Some(SandboxInfo {
            kind: "flatpak".to_string(),
            // 提示用户应当用什么命令更新。
            update_command: "flatpak update".to_string(),
        });
    }
    None
}

/// 只读沙箱的信息（回传前端做展示）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SandboxInfo {
    /// 沙箱类型标识，目前恒为 `flatpak`。
    pub kind: String,
    /// 建议用户执行的更新命令。
    pub update_command: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 非 Linux 平台必须恒为 false —— 检测只在 Linux 上有意义。
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn sandbox_detection_is_linux_only() {
        assert!(!is_flatpak(), "非 Linux 平台不应判定为 Flatpak");
        assert!(sandbox_hint().is_none());
    }

    /// 未知环境（当前测试环境）不应被误判为沙箱。
    ///
    /// 这条在容器里跑时也成立：只有真正的 Flatpak 沙箱才会有 `/.flatpak-info`。
    #[test]
    fn default_environment_is_not_flatpak() {
        // 环境是否真的是 Flatpak 取决于运行位置；这里只断言"两者一致"——
        // 即 hint 与检测结果不会互相矛盾。
        assert_eq!(is_flatpak(), sandbox_hint().is_some());
    }
}
