//! 房间成员档案，以及本机"机器标识"。
//!
//! # 为什么要有 machine id
//!
//! 陶瓦的成员列表靠它去重与识别自己：同名的两个玩家必须能区分，否则"谁是谁"只能靠
//! 猜。它必须**跨会话稳定**（重开应用还是同一个人），因此落在磁盘上，而不是每次随机。
//!
//! # 字段形状是互通契约
//!
//! `c:player_profiles_list` 的响应就是 `[{machine_id, name, vendor, kind}]`，
//! 上游客户端会按 `machine_id` 做二分查找并校验 `kind` 的合法性。多一个字段没事，
//! **少一个或改名就会让上游判定"响应非法"并断开**。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

/// 成员身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProfileKind {
    /// 房主。
    Host,
    /// 本机（只在访客侧存在）。
    Local,
    /// 其他访客。
    Guest,
}

impl ProfileKind {
    /// 上游在 `player_profiles_list` 里只接受 `HOST` / `GUEST`。
    pub fn wire_name(self) -> &'static str {
        match self {
            ProfileKind::Host => "HOST",
            ProfileKind::Local => "LOCAL",
            ProfileKind::Guest => "GUEST",
        }
    }
}

/// 一条成员档案。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub machine_id: String,
    pub name: String,
    pub vendor: String,
    pub kind: ProfileKind,
}

/// 本机机器标识的落地文件（相对数据目录）。
const MACHINE_ID_FILE: &str = "terracotta-machine-id";
/// 机器标识的字节数（与上游一致：16 字节 → 32 位十六进制）。
const MACHINE_ID_BYTES: usize = 16;

/// 本应用对外自称的版本串。
///
/// 上游把它写进 profile 的 `vendor` 字段，用于在成员列表里显示"对方是什么客户端"。
/// 保留 `EasyTier` 版本号能让排查"是不是内核不一致"更快。
pub fn vendor() -> String {
    format!(
        "MC Link {}, EasyTier {}",
        env!("CARGO_PKG_VERSION"),
        crate::engine::easytier::artifact::VERSION
    )
}

/// 读取（必要时生成）本机机器标识。
///
/// 读写失败一律退化为**本次运行内的随机值**：机器标识用于去重，不是安全边界，
/// 因此"读不到就换一个"比"整个适配器不可用"更合理。退化的后果是同机重启后
/// 在成员列表里换了身份，用户只会在极端情况下察觉。
pub fn machine_id(data_dir: &Path) -> String {
    let path = data_dir.join("Engine").join(MACHINE_ID_FILE);

    if let Ok(text) = std::fs::read_to_string(&path) {
        let trimmed = text.trim();
        if trimmed.len() == MACHINE_ID_BYTES * 2 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
            return trimmed.to_ascii_lowercase();
        }
        eprintln!("[陶瓦] 机器标识文件内容非法（{}），将重新生成", trimmed.len());
    }

    let generated = crate::plugin::crypto::random_hex(MACHINE_ID_BYTES);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("[陶瓦] 创建数据目录失败（{}）: {}", parent.display(), e);
            return generated;
        }
    }
    if let Err(e) = std::fs::write(&path, &generated) {
        eprintln!("[陶瓦] 写入机器标识失败（{}）: {}", path.display(), e);
    }
    generated
}

/// 解析成员列表响应体。
pub fn parse_profiles(bytes: &[u8]) -> Option<Vec<Profile>> {
    serde_json::from_slice::<Vec<Profile>>(bytes).ok()
}

/// 生成 `c:player_profiles_list` 的响应体（只允许 `HOST` / `GUEST`）。
pub fn encode_profiles(profiles: &[Profile]) -> Value {
    Value::Array(
        profiles
            .iter()
            .filter(|profile| profile.kind != ProfileKind::Local)
            .map(|profile| {
                json!({
                    "machine_id": profile.machine_id,
                    "name": profile.name,
                    "vendor": profile.vendor,
                    "kind": profile.kind.wire_name(),
                })
            })
            .collect(),
    )
}

/// 转成前端契约的成员形状（见 `mgr/room/helpers.rs::room_members`）。
pub fn to_players(profiles: &[Profile], machine_id: &str) -> Vec<Value> {
    profiles
        .iter()
        .map(|profile| {
            json!({
                "name": profile.name,
                "kind": profile.kind.wire_name(),
                "machineId": profile.machine_id,
                "self": profile.machine_id == machine_id,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclink-terracotta-{}-{}",
            name,
            crate::plugin::crypto::random_hex(4)
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn profile(id: &str, kind: ProfileKind) -> Profile {
        Profile {
            machine_id: id.to_string(),
            name: format!("player-{}", id),
            vendor: "test".to_string(),
            kind,
        }
    }

    #[test]
    fn kind_serializes_as_upstream_expects() {
        assert_eq!(
            serde_json::to_string(&ProfileKind::Host).unwrap(),
            "\"HOST\""
        );
        assert_eq!(
            serde_json::to_string(&ProfileKind::Guest).unwrap(),
            "\"GUEST\""
        );
        assert_eq!(
            serde_json::to_string(&ProfileKind::Local).unwrap(),
            "\"LOCAL\""
        );
    }

    #[test]
    fn machine_id_is_stable_across_calls() {
        let dir = temp_dir("machine");
        let first = machine_id(&dir);
        let second = machine_id(&dir);
        assert_eq!(first.len(), MACHINE_ID_BYTES * 2);
        assert_eq!(first, second, "机器标识应落盘并在下次读取时复用");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn machine_id_regenerates_when_file_is_corrupt() {
        let dir = temp_dir("corrupt");
        let path = dir.join("Engine").join(MACHINE_ID_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not-a-machine-id").unwrap();

        let id = machine_id(&dir);
        assert_eq!(id.len(), MACHINE_ID_BYTES * 2, "坏文件应被重新生成覆盖");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encode_profiles_drops_local_and_uses_wire_names() {
        let encoded = encode_profiles(&[
            profile("aa", ProfileKind::Host),
            profile("bb", ProfileKind::Local),
            profile("cc", ProfileKind::Guest),
        ]);
        let array = encoded.as_array().unwrap();
        assert_eq!(array.len(), 2, "LOCAL 不应出现在主机路径上");
        assert_eq!(array[0]["kind"], "HOST");
        assert_eq!(array[1]["kind"], "GUEST");
        assert_eq!(array[0]["machine_id"], "aa");
    }

    #[test]
    fn parse_profiles_accepts_upstream_payload() {
        let raw = br#"[{"machine_id":"aa","name":"Steve","vendor":"Terracotta","kind":"HOST"}]"#;
        let parsed = parse_profiles(raw).expect("应能解析上游负载");
        assert_eq!(parsed[0].kind, ProfileKind::Host);
        assert_eq!(parsed[0].name, "Steve");
        assert!(parse_profiles(b"{}").is_none(), "非数组应判为非法");
    }

    #[test]
    fn to_players_marks_self() {
        let players = to_players(
            &[profile("aa", ProfileKind::Host), profile("bb", ProfileKind::Local)],
            "bb",
        );
        assert_eq!(players[0]["self"], false);
        assert_eq!(players[1]["self"], true);
        assert_eq!(players[1]["machineId"], "bb");
    }
}
