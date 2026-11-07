//! 房间码的字符集与进制编解码（34 进制，去掉了容易看错的 `I` / `O`）。
//!
//! # 为什么是 34 进制
//!
//! 房间码要**人口述、手抄、截图**：`0/O`、`1/I/l` 这类混淆是真实投诉来源。
//! 陶瓦（Terracotta）用 `0123456789ABCDEFGHJKLMNPQRSTUVWXYZ` 这套 34 字符表，
//! 本项目的陶瓦适配器必须与它**逐字一致**才能互通，因此这里把字符表与编解码抽出来，
//! 供两个适配器共用：
//!
//! - 陶瓦：16 位（`U/XXXX-XXXX-XXXX-XXXX`），承载 128 位种子；
//! - 通用 EasyTier：4 位承载端口号。
//!
//! # 位序
//!
//! 低位在前（先输出 `value % 34`）。这不是随便定的：陶瓦的上游实现就是这个顺序，
//! 反过来会导致同一个码解出不同的网络名，表现为"码没错但连不上"。

/// 34 个字符：数字 + 大写字母（剔除 `I` 与 `O`）。
pub const ALPHABET: &[u8; 34] = b"0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// 把单个字符查成 0..34 的数值。
///
/// `I` 当 `1`、`O` 当 `0`：这两个字母不在字符表里，但用户会照着字形抄。
pub fn lookup(c: char) -> Option<u128> {
    let normalized = match c.to_ascii_uppercase() {
        'I' => '1',
        'O' => '0',
        other => other,
    };
    ALPHABET
        .iter()
        .position(|&b| b as char == normalized)
        .map(|index| index as u128)
}

/// 编码成 `width` 个字符（低位在前）。
///
/// `width` 不足以容纳 `value` 时仍按低位截断——调用方应先自行保证范围
/// （见 [`fits`]），这里不做静默的"多出来的高位丢弃"之外的额外处理。
pub fn encode(mut value: u128, width: usize) -> String {
    let mut out = String::with_capacity(width);
    for _ in 0..width {
        out.push(ALPHABET[(value % 34) as usize] as char);
        value /= 34;
    }
    out
}

/// 解码一串**不含分隔符**的字符（低位在前）。
pub fn decode(text: &str) -> Option<u128> {
    let mut value: u128 = 0;
    for c in text.chars().rev() {
        value = value.checked_mul(34)?.checked_add(lookup(c)?)?;
    }
    Some(value)
}

/// `value` 能否用 `width` 个字符表示。
pub fn fits(value: u128, width: usize) -> bool {
    let limit = 34u128.checked_pow(width as u32);
    match limit {
        Some(limit) => value < limit,
        // 34^25 以上溢出 u128：此时任何 u128 都放得下
        None => true,
    }
}

/// 每 `size` 个字符插一个连字符（只影响展示，不参与解码）。
pub fn group(text: &str, size: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + text.len() / size);
    for (index, c) in chars.iter().enumerate() {
        if index > 0 && index % size == 0 {
            out.push('-');
        }
        out.push(*c);
    }
    out
}

/// 归一化用户输入：转大写，丢掉一切非字符表内容（连字符、空格、粘贴带来的杂字符）。
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| lookup(*c).is_some())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphabet_is_34_unique_chars() {
        assert_eq!(ALPHABET.len(), 34);
        let mut seen = std::collections::HashSet::new();
        for byte in ALPHABET {
            assert!(seen.insert(*byte), "字符表不应重复: {}", *byte as char);
            assert!(!(*byte as char).is_ascii_lowercase());
        }
        assert!(!ALPHABET.contains(&b'I'));
        assert!(!ALPHABET.contains(&b'O'));
    }

    #[test]
    fn encode_decode_roundtrip() {
        for value in [0u128, 1, 33, 34, 35, 12345, u128::from(u64::MAX)] {
            let text = encode(value, 16);
            assert_eq!(text.len(), 16);
            assert_eq!(decode(&text), Some(value), "值 {} 往返失败: {}", value, text);
        }
    }

    #[test]
    fn low_digit_comes_first() {
        // 位序是上游契约：34 必须落在第二个字符上，否则互通时网络名会变
        assert_eq!(encode(34, 4), "0100");
        assert_eq!(encode(0, 4), "0000");
    }

    #[test]
    fn lookup_aliases_confusable_letters() {
        assert_eq!(lookup('I'), lookup('1'));
        assert_eq!(lookup('O'), lookup('0'));
        assert_eq!(lookup('o'), lookup('0'));
        assert_eq!(lookup('-'), None);
        // 表里 'U' 排在第 29 个（0 基 28）：剔除 I/O 后 A..H 占 10..17
        assert_eq!(lookup('U'), Some(28));
        assert_eq!(lookup('Z'), Some(33));
    }

    #[test]
    fn normalize_strips_separators_and_fixes_case() {
        assert_eq!(normalize("u/abcd-efgh"), "UABCDEFGH");
        assert_eq!(normalize("ab cd"), "ABCD");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn group_inserts_dashes_every_n() {
        assert_eq!(group("ABCDEFGH", 4), "ABCD-EFGH");
        assert_eq!(group("ABCD", 4), "ABCD");
        assert_eq!(group("ABCDE", 4), "ABCD-E");
    }

    #[test]
    fn fits_reports_capacity() {
        assert!(fits(33, 1));
        assert!(!fits(34, 1));
        assert!(fits(65535, 4));
        assert!(!fits(34u128.pow(4), 4));
    }
}
