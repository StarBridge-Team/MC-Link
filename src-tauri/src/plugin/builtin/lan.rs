//! 局域网 MOTD 广播的**共享契约**：耦合器发、扫描器认。
//!
//! # 为什么必须有这一层
//!
//! Minecraft Java 版的"局域网世界"发现协议就是一条 UDP 多播报文：
//!
//! ```text
//! [MOTD]<motd>[/MOTD][AD]<端口>[/AD]
//! ```
//!
//! 发（`minecraft_coupler`）与收（`minecraft_scanner`）原本各写一份字符串拼接与截取，
//! 后果是两边会**各自漂移**：改了 MOTD 文案，扫描器仍然按旧文案过滤，于是"自己的广播"
//! 变成一台可被发现的"局域网游戏"——用户还能拿它再建一次房，而那只是一个指向隧道的端口。
//!
//! 因此格式、解析与"这是不是我们自己发的"三件事只在这里定义。
//!
//! # 端口是谁的端口
//!
//! 报文里的端口是**本机玩家应该连的那个端口**：
//!
//! - 房主：自己的游戏端口；
//! - 访客：引擎把房主端口接出来后的**本机端口**（`127.0.0.1:<port>`）。
//!
//! 选错端口的表现是"局域网列表里能看到 MC Link，但点进去连不上"，因此房间编排
//! （[`crate::mgr::room`]）必须把**适配器回报的**端口传进来，不能用房主的游戏端口顶替。

/// 我们自己广播的 MOTD 前缀。扫描器据此排除"自己的广播"。
pub const MOTD: &str = "MC Link";

/// 拼一条广播报文。
pub fn format_advert(motd: &str, port: u16) -> String {
    format!("[MOTD]{}[/MOTD][AD]{}[/AD]", motd, port)
}

/// 解析一条广播报文；格式不符或端口非法时返回 `None`。
pub fn parse_advert(data: &str) -> Option<(String, u16)> {
    let motd = extract_tag(data, "[MOTD]", "[/MOTD]")?;
    let port = extract_tag(data, "[AD]", "[/AD]")?;
    // 端口必须是 1..=65535：`0` 与越界值都是坏报文
    let port = port.trim().parse::<u16>().ok().filter(|p| *p > 0)?;
    Some((motd, port))
}

/// 这条广播是不是我们自己发的（避免把隧道当成一台本地游戏）。
///
/// 用前缀而不是相等：MOTD 里可能带上房间码之类的后缀，而**过滤**必须比**展示**宽松，
/// 否则一旦文案带上后缀，自己的广播就会重新混进扫描结果。
pub fn is_ours(motd: &str) -> bool {
    motd.trim_start().starts_with(MOTD)
}

/// 取 `[start]…[end]` 之间的内容。
fn extract_tag(data: &str, start: &str, end: &str) -> Option<String> {
    let from = data.find(start)? + start.len();
    let rest = &data[from..];
    let to = rest.find(end)?;
    Some(rest[..to].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advert_roundtrip() {
        let message = format_advert(MOTD, 25565);
        assert_eq!(message, "[MOTD]MC Link[/MOTD][AD]25565[/AD]");
        assert_eq!(parse_advert(&message), Some((MOTD.to_string(), 25565)));
    }

    #[test]
    fn parse_tolerates_extra_tags_and_suffix() {
        // 别的工具可能在报文里追加字段；只要两个标签在，就该解析得出来
        let message = "[MOTD]MC Link · U/ABCD-EFGH-JKLM-NPQR[/MOTD][AD]25566[/AD]";
        assert_eq!(
            parse_advert(message),
            Some((
                "MC Link · U/ABCD-EFGH-JKLM-NPQR".to_string(),
                25566
            ))
        );
    }

    #[test]
    fn parse_rejects_broken_messages() {
        assert!(parse_advert("").is_none());
        assert!(parse_advert("[MOTD]x[/MOTD]").is_none(), "缺 [AD] 段");
        assert!(parse_advert("[MOTD]x[/MOTD][AD]0[/AD]").is_none(), "端口 0 非法");
        assert!(
            parse_advert("[MOTD]x[/MOTD][AD]70000[/AD]").is_none(),
            "越界端口非法"
        );
        assert!(parse_advert("[MOTD]x[/MOTD][AD]abc[/AD]").is_none());
    }

    #[test]
    fn ours_is_detected_by_prefix() {
        assert!(is_ours(MOTD));
        assert!(is_ours("MC Link · 房间 U/7000"));
        assert!(is_ours("   MC Link"));
        assert!(!is_ours("Steve 的世界"));
        assert!(!is_ours(""));
    }
}
