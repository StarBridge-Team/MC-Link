use super::PersonalizationSettings;
use crate::core::events::{topic, EventSink};
use crate::persist;
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::Mutex;

/// 设置变更的版本号。
///
/// **单调递增**：前端靠它去重（同一窗口可能重复收到同一条事件）与判序。
/// 与房间事件的 `rev` 一样，**在锁内推进**——两个并发的保存不能拿到同一个号，
/// 而且"发出去的顺序"必须与"`rev` 的顺序"一致（顺序即正确性）。
static SETTINGS_REV: Mutex<u64> = Mutex::new(0);

pub(crate) fn write_setting(data_dir: &Path, section: &str, content: &str) -> Result<(), String> {
    let path = super::section_path(data_dir, section)?;
    persist::atomic_write(&path, content.as_bytes())
}

pub(crate) fn write_personalization(
    data_dir: &Path,
    settings: PersonalizationSettings,
) -> Result<(), String> {
    let path = super::read::personalization_path(data_dir)?;
    // 先读旧值：广播的 `patch` 必须是"本次实际改动的字段"，而不是整份设置
    // （整份会让每个窗口把背景与材质都重算一遍）。
    let previous = persist::load_yaml::<PersonalizationSettings>(&path)
        .ok()
        .map(|loaded| loaded.value);
    persist::save_yaml(&path, &settings)?;
    // **保存成功之后**才广播：落盘失败时不能让别的窗口以为已经改好了。
    let patch = diff_settings(previous.as_ref(), &settings);
    if !patch.is_empty() {
        if let Some(sink) = crate::core::events::global_sink() {
            broadcast_patch(&*sink, patch);
        }
    }
    Ok(())
}

/// 计算"本次实际改动的字段"。
///
/// 返回的 key 是设置项的字段名（snake_case）——那正是 `personalization.yml` 里的 key，
/// 也是前端 `PersonalizationSettings` 的字段名（**冻结契约**，见 `config/mod.rs`）。
/// 事件负载的外层字段名（`rev` / `patch`）才是 camelCase 风格。
fn diff_settings(
    previous: Option<&PersonalizationSettings>,
    current: &PersonalizationSettings,
) -> Map<String, Value> {
    let mut patch = Map::new();
    let Ok(new_value) = serde_json::to_value(current) else {
        return patch;
    };
    let Some(new_map) = new_value.as_object() else {
        return patch;
    };
    let old_map = previous
        .and_then(|p| serde_json::to_value(p).ok())
        .and_then(|v| v.as_object().cloned());
    for (key, value) in new_map {
        // 没有旧值（首次保存）时，所有字段都算改动。
        let changed = match &old_map {
            Some(old) => old.get(key) != Some(value),
            None => true,
        };
        if changed {
            patch.insert(key.clone(), value.clone());
        }
    }
    patch
}

/// 广播设置变更：`{ rev, patch }`。
///
/// **持锁发**：保证"事件到达的顺序"与"`rev` 的顺序"一致（与房间事件同一规矩）。
fn broadcast_patch(sink: &dyn EventSink, patch: Map<String, Value>) -> u64 {
    let mut guard = SETTINGS_REV.lock().unwrap_or_else(|e| e.into_inner());
    *guard += 1;
    let rev = *guard;
    sink.emit(topic::APP_SETTINGS, json!({ "rev": rev, "patch": patch }));
    rev
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// 记录型 sink：断言"发了什么"。
    #[derive(Default)]
    struct Recorder {
        events: StdMutex<Vec<(String, Value)>>,
    }

    impl EventSink for Recorder {
        fn emit(&self, topic: &str, payload: Value) {
            self.events
                .lock()
                .unwrap()
                .push((topic.to_string(), payload));
        }
        fn emit_to(&self, target: &str, topic: &str, payload: Value) {
            self.events
                .lock()
                .unwrap()
                .push((format!("{}@{}", topic, target), payload));
        }
    }

    #[test]
    fn diff_contains_only_changed_fields() {
        let previous = PersonalizationSettings::default();
        let mut current = previous.clone();
        current.theme_color = "#ff0000".to_string();
        current.background_opacity = 42.0;

        let patch = diff_settings(Some(&previous), &current);
        assert_eq!(
            patch.len(),
            2,
            "只有真正变了的字段才该进 patch: {:?}",
            patch
        );
        assert_eq!(patch.get("theme_color"), Some(&json!("#ff0000")));
        assert_eq!(patch.get("background_opacity"), Some(&json!(42.0)));
    }

    #[test]
    fn identical_settings_produce_empty_patch() {
        let settings = PersonalizationSettings::default();
        assert!(diff_settings(Some(&settings), &settings).is_empty());
    }

    #[test]
    fn diff_without_previous_covers_every_field() {
        let current = PersonalizationSettings::default();
        let patch = diff_settings(None, &current);
        let total = serde_json::to_value(&current)
            .unwrap()
            .as_object()
            .unwrap()
            .len();
        assert_eq!(patch.len(), total, "没有旧值时整份设置都算改动");
    }

    #[test]
    fn broadcast_payload_carries_monotonic_rev_and_patch() {
        let recorder = Recorder::default();
        let mut patch = Map::new();
        patch.insert("theme_color".to_string(), json!("#123456"));

        let first = broadcast_patch(&recorder, patch.clone());
        let second = broadcast_patch(&recorder, patch);

        assert!(second > first, "rev 必须单调递增");
        let events = recorder.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, topic::APP_SETTINGS);
        assert_eq!(events[0].1["rev"], json!(first));
        assert_eq!(events[0].1["patch"]["theme_color"], json!("#123456"));
    }
}
