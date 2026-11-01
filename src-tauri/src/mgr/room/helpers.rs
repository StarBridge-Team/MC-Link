//! 纯函数：适配器返回值 → 房间契约字段。
//!
//! 拆出来是为了守住单文件行数上限：这些函数既不碰会话状态也不调插件，
//! 留在 `room.rs` 里只会把它撑破。

use serde_json::Value;

use crate::domain::room::RoomMember;

/// 把适配器自报的字段列表规范化成前端契约（camelCase）。
///
/// 外部插件可以按任意命名返回：内置陶瓦用 `autofill_from_invite`，前端契约读的是
/// `autofillFromInvite`。不转换时该字段恒为 `undefined`，前端只能退化成
/// "取第一个字段当邀请码"——适配器一旦声明多个字段就会写错位置。
pub(super) fn normalize_fields(value: Option<Value>) -> Value {
    let Some(Value::Array(items)) = value else {
        return Value::Array(vec![]);
    };
    Value::Array(
        items
            .into_iter()
            .map(|item| {
                let Value::Object(mut map) = item else {
                    return item;
                };
                for (from, to) in [
                    ("autofill_from_invite", "autofillFromInvite"),
                    ("max_length", "maxLength"),
                ] {
                    if let Some(v) = map.remove(from) {
                        map.insert(to.to_string(), v);
                    }
                }
                Value::Object(map)
            })
            .collect(),
    )
}

/// 端口越界一律判无效：`as u16` 会把 70000 静默截断成 4464，
/// 于是耦合器往一个没人监听的端口广播，且没有任何报错。
pub(super) fn port_from_value(value: &Value) -> Option<u16> {
    let p = value.as_u64()?;
    (1..=65535).contains(&p).then_some(p as u16)
}

/// 从适配器返回值里提取成员列表。只认统一约定 `players` 数组，拿不到就是空数组。
pub(super) fn room_members(value: &Value) -> Vec<RoomMember> {
    value
        .get("players")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|p| {
                    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let machine_id = p.get("machineId").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() && machine_id.is_empty() {
                        return None;
                    }
                    Some(RoomMember {
                        name: name.to_string(),
                        kind: p.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        machine_id: machine_id.to_string(),
                        is_self: p.get("self").and_then(|v| v.as_bool()).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_fields_renames_known_snake_case_keys() {
        let raw = json!([{ "key": "code", "autofill_from_invite": true, "max_length": 32 }]);
        let out = normalize_fields(Some(raw));
        assert_eq!(out[0]["autofillFromInvite"], true);
        assert_eq!(out[0]["maxLength"], 32);
        assert!(out[0].get("autofill_from_invite").is_none());
    }

    #[test]
    fn normalize_fields_defaults_to_empty_array() {
        assert_eq!(normalize_fields(None), json!([]));
    }

    #[test]
    fn port_rejects_out_of_range() {
        assert_eq!(port_from_value(&json!(25565)), Some(25565));
        assert_eq!(port_from_value(&json!(70000)), None);
        assert_eq!(port_from_value(&json!(0)), None);
        assert_eq!(port_from_value(&json!("25565")), None);
    }

    #[test]
    fn members_skip_anonymous_entries() {
        let raw = json!({ "players": [
            { "name": "Steve", "machineId": "m1", "self": true },
            { "name": "", "machineId": "" }
        ]});
        let members = room_members(&raw);
        assert_eq!(members.len(), 1);
        assert!(members[0].is_self);
    }
}
