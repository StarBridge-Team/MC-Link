//! 清单的'展示维度'归一化：工作方式、平台、标签。
//!
//! 这些函数的共同点是**宽容**：未知取值只丢弃并留痕，绝不让整个插件加载失败。
//! 它们与结构校验（`manifest::validate`）是两条路，混在一个文件里会让'哪条该严格'
//! 变得难以判断。

use serde::{Deserialize, Serialize};
/// 工作方式：插件靠什么把局域网暴露给对端。
///
/// 与自由 `tags` 的区别在于**它必须可靠**——界面上的"按方式筛选"与 OOBE 的
/// 推荐展示都直接依赖它，所以这里是枚举而非字符串：写错的取值会在解析期被拒，
/// 不会变成筛选栏里第四个莫名的选项。
///
/// 刻意与"具体用什么软件"解耦：`bundled` 只说明"打包了第三方本体"，
/// 具体是哪个软件由插件自己声明（`tags` / 清单其它字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionMethod {
    /// 捆绑第三方软件本体（把别人家的整包带进来）。
    #[serde(rename = "bundled")]
    Bundled,
    /// P2P 打洞直连。
    #[serde(rename = "p2p")]
    P2p,
    /// 中继转发。
    #[serde(rename = "relay")]
    Relay,
    /// 内网映射 / 端口映射。
    #[serde(rename = "port-mapping")]
    PortMapping,
}

impl ConnectionMethod {
    /// 全部取值，供筛选界面与校验使用。
    pub const ALL: &'static [ConnectionMethod] = &[
        ConnectionMethod::Bundled,
        ConnectionMethod::P2p,
        ConnectionMethod::Relay,
        ConnectionMethod::PortMapping,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionMethod::Bundled => "bundled",
            ConnectionMethod::P2p => "p2p",
            ConnectionMethod::Relay => "relay",
            ConnectionMethod::PortMapping => "port-mapping",
        }
    }

    /// 从字符串解析（大小写不敏感，接受下划线写法）。
    pub fn parse(input: &str) -> Option<Self> {
        let normalized = input.trim().to_ascii_lowercase().replace('_', "-");
        ConnectionMethod::ALL
            .iter()
            .copied()
            .find(|m| m.as_str() == normalized)
    }
}

/// 已知平台取值。未知值在**展示层**被丢弃并留痕，但不会让插件加载失败——
/// 开发者写错一个平台名，不该让整个插件不可用。
pub const KNOWN_PLATFORMS: &[&str] = &["windows", "linux", "macos"];

/// 归一化平台取值：去空白、小写、去重，并丢掉不在 [`KNOWN_PLATFORMS`] 里的。
pub fn normalize_platforms(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let value = item.trim().to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        if !KNOWN_PLATFORMS.contains(&value.as_str()) {
            eprintln!("[插件] 未知平台「{value}」已忽略（可选：windows/linux/macos）");
            continue;
        }
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

/// 归一化工作方式：丢掉未知取值并留痕。
///
/// 与 [`normalize_platforms`] 同口径——写错的取值只影响这一项，不会让插件不可用。
pub fn normalize_methods(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        match ConnectionMethod::parse(item) {
            Some(method) => {
                let value = method.as_str().to_string();
                if !out.contains(&value) {
                    out.push(value);
                }
            }
            None => eprintln!(
                "[插件] 未知工作方式「{}」已忽略（可选：bundled/p2p/relay/port-mapping）",
                item.trim()
            ),
        }
    }
    out
}

/// 归一化自由标签：去空白、去重、限长限量。
///
/// **不做白名单**（开发者可自定义，中文标签是允许的），只做防脏数据的清洗：
/// 一个插件写 200 个标签会让筛选栏彻底不可用。
pub fn normalize_tags(raw: &[String]) -> Vec<String> {
    const MAX_TAGS: usize = 12;
    const MAX_LEN: usize = 24;
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let value = item.trim();
        if value.is_empty() {
            continue;
        }
        let value: String = value.chars().take(MAX_LEN).collect();
        if !out.contains(&value) {
            out.push(value);
        }
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
}

