//! 陶瓦房间码：`U/XXXX-XXXX-XXXX-XXXX`。
//!
//! # 这不是"我们的格式"
//!
//! 房间码必须与**上游陶瓦客户端**逐字节一致，否则本适配器就无法和真实的陶瓦用户
//! 互通（那是它存在的唯一理由）。因此这里的规则是照搬上游的：
//!
//! - 16 个字符，字符表为 34 进制（见 [`super::code`]），低位在前；
//! - 每 4 个字符一个连字符，整体加 `U/` 前缀 → 21 个字符；
//! - **数值必须能被 7 整除**（上游靠它挡掉手抄错误：随便改一位后大概率不再是 7 的倍数）。
//!
//! # 房间码里装了什么
//!
//! 只有一个 128 位数值。由它派生两件东西（都写进 EasyTier 的启动参数）：
//!
//! - 网络名 `scaffolding-mc-XXXXXXXX`（前 8 个字符）；
//! - 网络密钥 `XXXX-XXXX`（后 8 个字符）。
//!
//! 也就是说**知道房间码就等于知道密钥**——这是上游的既有设计，不是本适配器的取舍。

use crate::plugin::builtin::code;

/// 房间码前缀。
const PREFIX: &str = "U/";
/// 无分隔符的字符数。
const CODE_CHARS: usize = 16;
/// 每组的字符数。
const GROUP: usize = 4;
/// 网络名前缀（上游约定，访客与主机都靠它认房间）。
const NET_PREFIX: &str = "scaffolding-mc-";
/// 校验因子：上游用 7。
const CHECK_FACTOR: u128 = 7;

/// 一个可用的房间。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomCode {
    /// 展示形态：`U/XXXX-XXXX-XXXX-XXXX`。
    pub code: String,
    /// EasyTier 网络名。
    pub network_name: String,
    /// EasyTier 网络密钥。
    pub network_secret: String,
}

impl RoomCode {
    /// 生成一个新房间。
    pub fn create() -> Self {
        let random = u128::from_str_radix(&crate::plugin::crypto::random_hex(16), 16).unwrap_or(1);
        let space = 34u128.pow(CODE_CHARS as u32);
        let value = random % space;
        // 抹掉余数使它能被 7 整除：校验位不是"算出来的"，而是"取整出来的"
        let value = value - value % CHECK_FACTOR;
        Self::from_value(value)
    }

    /// 解析用户输入（容忍小写、前后杂字符、多余分隔符）。
    ///
    /// 与上游一致的宽松点：在一段文本里**找**符合形状的那 21 个字符，
    /// 因此"复制了一整句话"也能救回来。
    pub fn parse(text: &str) -> Option<Self> {
        let upper: Vec<char> = text.trim().to_ascii_uppercase().chars().collect();
        let width = PREFIX.len() + CODE_CHARS + CODE_CHARS / GROUP - 1;

        for window in upper.windows(width) {
            if window[0] != 'U' || window[1] != '/' {
                continue;
            }
            if let Some(value) = Self::to_value(&window[PREFIX.len()..]) {
                return Some(Self::from_value(value));
            }
        }
        None
    }

    /// 解析连字符分隔的 19 个字符（不含 `U/`）。
    fn to_value(slice: &[char]) -> Option<u128> {
        if slice.len() != CODE_CHARS + CODE_CHARS / GROUP - 1 {
            return None;
        }
        let mut digits = String::with_capacity(CODE_CHARS);
        for (index, c) in slice.iter().enumerate() {
            // 第 4/8/12 个字符之后是连字符，其余必须是字符表内的字符
            let is_dash_slot = index > 0 && index % (GROUP + 1) == GROUP;
            if is_dash_slot {
                if *c != '-' {
                    return None;
                }
                continue;
            }
            digits.push(*c);
        }
        let value = code::decode(&digits)?;
        (value % CHECK_FACTOR == 0).then_some(value)
    }

    /// 由数值派生展示形态、网络名与密钥。
    fn from_value(value: u128) -> Self {
        let digits = code::encode(value, CODE_CHARS);
        let network_name = format!("{}{}", NET_PREFIX, &digits[..CODE_CHARS / 2]);
        let network_secret = code::group(&digits[CODE_CHARS / 2..], GROUP);
        Self {
            code: format!("{}{}", PREFIX, code::group(&digits, GROUP)),
            network_name,
            network_secret,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_has_upstream_shape() {
        let room = RoomCode::create();
        assert_eq!(room.code.len(), 21, "应形如 U/XXXX-XXXX-XXXX-XXXX");
        assert!(room.code.starts_with("U/"));
        assert_eq!(room.code.matches('-').count(), 3);
        assert!(room.network_name.starts_with("scaffolding-mc-"));
        assert_eq!(room.network_name.len(), "scaffolding-mc-".len() + 8);
        assert_eq!(room.network_secret.len(), 9, "应形如 XXXX-XXXX");
        assert_eq!(room.network_secret.matches('-').count(), 1);
    }

    #[test]
    fn create_parse_roundtrip() {
        for _ in 0..50 {
            let room = RoomCode::create();
            let parsed = RoomCode::parse(&room.code).expect("自己生成的码必须能解析");
            assert_eq!(parsed, room);
        }
    }

    #[test]
    fn parse_tolerates_lowercase_and_padding_text() {
        let room = RoomCode::create();
        let messy = format!("  来玩：{} !! ", room.code.to_lowercase());
        assert_eq!(RoomCode::parse(&messy), Some(room));
    }

    #[test]
    fn parse_rejects_broken_codes() {
        assert!(RoomCode::parse("").is_none());
        assert!(RoomCode::parse("U/ABCD-EFGH-JKLM-NPQR").is_none(), "不是 7 的倍数应拒绝");
        // 连字符位置错了
        assert!(RoomCode::parse("U/ABCDE-FGH-JKLM-NPQR").is_none());
        // 混入了字符表之外的字符
        assert!(RoomCode::parse("U/ABCD-EFGH-JKLM-NPQ!").is_none());
    }

    #[test]
    fn value_must_be_multiple_of_seven() {
        // 直接构造一个不满足校验的值，确认 to_value 拒绝
        let slice: Vec<char> = "0000-0000-0000-0001".chars().collect();
        assert!(RoomCode::to_value(&slice).is_none());
        let zero: Vec<char> = "0000-0000-0000-0000".chars().collect();
        assert_eq!(RoomCode::to_value(&zero), Some(0));
    }

    #[test]
    fn derived_names_use_first_and_second_half() {
        // 值 7 → 数字串 "7000..."：前 8 位进网络名，后 8 位进密钥
        let room = RoomCode::from_value(7);
        assert_eq!(room.network_name, "scaffolding-mc-70000000");
        assert_eq!(room.network_secret, "0000-0000");
        assert_eq!(room.code, "U/7000-0000-0000-0000");
    }
}
