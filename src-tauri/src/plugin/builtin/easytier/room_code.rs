//! 通用 EasyTier 房间的房间码。
//!
//! # 格式
//!
//! `<网络名 8 字符>-<密钥 8 字符>-<端口 4 字符>`，字符表与陶瓦共用（34 进制、去掉 I/O），
//! 每 4 个字符一个连字符，因此看起来像 `ABCD-EFGH-JKLM-2556`。
//!
//! # 为什么把端口写进码里
//!
//! 访客要把房主的游戏端口接出来（`127.0.0.1:<本地> → <房主 IP>:<端口>`），而这条转发
//! **必须在启动参数里给**。端口若不在码里，访客就得先连上一次、再重启内核补上转发
//! （多约十秒），所以直接把它编进房间码。
//!
//! 代价是房间码比陶瓦的长两个字符，且**不能**与上游陶瓦互通——那是
//! [`super::super::terracotta`] 那条链路负责的事，两者刻意分开。

use crate::plugin::builtin::code;

use super::{NET_CHARS, NET_PREFIX, PORT_CHARS, SECRET_CHARS};

/// 房间码：房主生成、访客原样输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomCode {
    /// 网络名（`mclink-XXXXXXXX`）。
    pub network_name: String,
    /// 网络密钥。
    pub network_secret: String,
    /// 房主侧游戏端口。
    pub port: u16,
}

impl RoomCode {
    /// 生成一个随机房间码。
    pub fn generate(port: u16) -> Self {
        let suffix = code::encode(
            u128::from_str_radix(&crate::plugin::crypto::random_hex(8), 16).unwrap_or(1),
            NET_CHARS,
        );
        let secret = code::encode(
            u128::from_str_radix(&crate::plugin::crypto::random_hex(8), 16).unwrap_or(2),
            SECRET_CHARS,
        );
        Self {
            network_name: format!("{}{}", NET_PREFIX, suffix),
            network_secret: secret,
            port,
        }
    }

    /// 解析用户输入（容忍小写、空格、多余分隔符）。
    pub fn parse(text: &str) -> Option<Self> {
        let normalized = code::normalize(text);
        let expected = NET_CHARS + SECRET_CHARS + PORT_CHARS;
        if normalized.chars().count() != expected {
            return None;
        }
        let mut chars = normalized.chars();
        let net: String = chars.by_ref().take(NET_CHARS).collect();
        let secret: String = chars.by_ref().take(SECRET_CHARS).collect();
        let port_text: String = chars.collect();

        let port = code::decode(&port_text)?;
        if port == 0 || port > u16::MAX as u128 {
            return None;
        }

        Some(Self {
            network_name: format!("{}{}", NET_PREFIX, net),
            network_secret: secret,
            port: port as u16,
        })
    }

    /// 给用户看的形态：`XXXX-XXXX-XXXX-XXXX`（4 字符一组）。
    pub fn display(&self) -> String {
        let suffix = self.network_name.trim_start_matches(NET_PREFIX);
        let port = code::encode(u128::from(self.port), PORT_CHARS);
        format!(
            "{}-{}-{}",
            code::group(suffix, 4),
            code::group(&self.network_secret, 4),
            port
        )
    }
}