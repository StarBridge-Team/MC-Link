//! Minecraft 离线模式 UUID。
//!
//! 算法（与 Java 版 `UUID.nameUUIDFromBytes` 一致）：
//!
//! ```text
//! md5("OfflinePlayer:" + name) → 16 字节
//! 第 7 字节高 4 位置 0011（version 3）
//! 第 9 字节高 2 位置 10（RFC 4122 variant）
//! ```
//!
//! **不要用 `Uuid::new_v3`**：它会把命名空间的字节前缀进哈希，得到的值与
//! Java 版完全不同，服务端就认不出同一个玩家。

use md5::{Digest, Md5};

pub fn offline_uuid(name: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{}", name).as_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    uuid::Uuid::from_bytes(bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_is_stable_and_version_3() {
        let a = offline_uuid("Steve");
        let b = offline_uuid("Steve");
        assert_eq!(a, b, "同一个名字必须得到同一个 UUID");
        assert_eq!(a.len(), 36);
        // version 3（MD5）与 RFC 4122 variant
        assert_eq!(&a[14..15], "3", "version 位必须是 3：{}", a);
        assert!(
            matches!(&a[19..20], "8" | "9" | "a" | "b"),
            "variant 位不合法：{}",
            a
        );
    }

    #[test]
    fn different_names_differ() {
        assert_ne!(offline_uuid("Steve"), offline_uuid("Alex"));
    }

    #[test]
    fn matches_known_offline_player() {
        // Java 版 `OfflinePlayer:Notch` 的公认取值。
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }
}
