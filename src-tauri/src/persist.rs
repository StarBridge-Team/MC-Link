//! 统一持久化层。
//!
//! 所有**用户数据**（设置 yml、适配器状态、插件注册表）的读写都必须经由此模块。
//! 集中在一处的原因：
//!
//! 1. **原子写入**：先写同目录的 `.tmp` 再 `rename` 覆盖。此前各处直接用
//!    `std::fs::write`，进程在写盘中途被强杀会留下半截文件。
//! 2. **损坏隔离**：解析失败时把原文件改名为 `.corrupt-<时间戳>`，**绝不覆盖**。
//!    旧逻辑是"解析失败 → 静默写入默认值"，用户数据被静默清空且无法取证。
//! 3. **统一序列化与错误措辞**：YAML / JSON 各自只在这里解析一次。
//!
//! 注意：`Assets/` 属于**缓存**（可重新下载），不走这里；本模块只负责丢了就找不回来的数据。

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// 读取结果。
///
/// `recovered == true` 表示原文件损坏：已被隔离为备份、本次使用默认值。
#[derive(Debug, Clone)]
pub struct Loaded<T> {
    pub value: T,
    pub recovered: bool,
    pub backup: Option<PathBuf>,
}

/// 读取 YAML。文件不存在 → `T::default()`；损坏 → 隔离原文件并返回默认值。
pub fn load_yaml<T: DeserializeOwned + Default>(path: &Path) -> Result<Loaded<T>, String> {
    load_with(path, |text| {
        serde_yaml::from_str::<T>(text).map_err(|e| e.to_string())
    })
}

/// 原子写入 YAML。
pub fn save_yaml<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_yaml::to_string(value).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, text.as_bytes())
}

/// 读取 JSON。文件不存在 → `T::default()`；损坏 → 隔离原文件并返回默认值。
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> Result<Loaded<T>, String> {
    load_with(path, |text| {
        serde_json::from_str::<T>(text).map_err(|e| e.to_string())
    })
}

/// 原子写入 JSON（pretty 格式，便于人工排查）。
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, text.as_bytes())
}

/// 原子写入原始字节：写 `.tmp` → `sync_all` → `rename` 覆盖目标。
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录 {} 失败: {}", parent.display(), e))?;
    }

    let tmp = tmp_path(path);
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| format!("创建临时文件 {} 失败: {}", tmp.display(), e))?;
        file.write_all(bytes)
            .map_err(|e| format!("写入临时文件 {} 失败: {}", tmp.display(), e))?;
        file.sync_all()
            .map_err(|e| format!("刷新临时文件 {} 失败: {}", tmp.display(), e))?;
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("替换 {} 失败: {}", path.display(), e)
    })?;

    if cfg!(debug_assertions) {
        eprintln!("[persist] 已保存 {}（{} 字节）", path.display(), bytes.len());
    }
    Ok(())
}

fn load_with<T, F>(path: &Path, parse: F) -> Result<Loaded<T>, String>
where
    T: Default,
    F: FnOnce(&str) -> Result<T, String>,
{
    if !path.exists() {
        return Ok(Loaded {
            value: T::default(),
            recovered: false,
            backup: None,
        });
    }

    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;

    // 空文件通常意味着上一次写入被中断（旧实现直接写目标文件所致），按损坏处理
    if text.trim().is_empty() {
        let backup = quarantine(path);
        eprintln!(
            "[persist] {} 为空（可能上次写入被中断），已隔离并改用默认值",
            path.display()
        );
        return Ok(Loaded {
            value: T::default(),
            recovered: true,
            backup,
        });
    }

    match parse(&text) {
        Ok(value) => Ok(Loaded {
            value,
            recovered: false,
            backup: None,
        }),
        Err(e) => {
            let backup = quarantine(path);
            eprintln!(
                "[persist] {} 解析失败（{}），已隔离为 {} 并改用默认值；原文件未被覆盖",
                path.display(),
                e,
                backup
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "（隔离失败，已就地保留）".to_string())
            );
            Ok(Loaded {
                value: T::default(),
                recovered: true,
                backup,
            })
        }
    }
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

/// 把损坏文件改名为 `<原名>.corrupt-<秒级时间戳>`（尽力而为，失败不阻断启动）。
fn quarantine(path: &Path) -> Option<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut s = path.as_os_str().to_os_string();
    s.push(format!(".corrupt-{}", stamp));
    let backup = PathBuf::from(s);

    match std::fs::rename(path, &backup) {
        Ok(()) => Some(backup),
        Err(e) => {
            eprintln!("[persist] 隔离 {} 失败: {}", path.display(), e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        count: u32,
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mc-link-persist-{}-{}", tag, std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_file_yields_default_without_recovery() {
        let path = temp_dir("missing").join("nope.yml");
        let loaded = load_yaml::<Sample>(&path).unwrap();
        assert_eq!(loaded.value, Sample::default());
        assert!(!loaded.recovered);
    }

    #[test]
    fn round_trip_preserves_values() {
        let path = temp_dir("roundtrip").join("sample.yml");
        let value = Sample { name: "mc".into(), count: 7 };
        save_yaml(&path, &value).unwrap();

        let loaded = load_yaml::<Sample>(&path).unwrap();
        assert_eq!(loaded.value, value);
        assert!(!loaded.recovered);
    }

    #[test]
    fn save_overwrites_existing_file() {
        let path = temp_dir("overwrite").join("sample.yml");
        save_yaml(&path, &Sample { name: "a".into(), count: 1 }).unwrap();
        save_yaml(&path, &Sample { name: "b".into(), count: 2 }).unwrap();

        let loaded = load_yaml::<Sample>(&path).unwrap();
        assert_eq!(loaded.value.name, "b");
    }

    #[test]
    fn corrupt_file_is_quarantined_and_never_overwritten() {
        let dir = temp_dir("corrupt");
        let path = dir.join("sample.yml");
        std::fs::write(&path, "this: [is: not: valid: yaml").unwrap();

        let loaded = load_yaml::<Sample>(&path).unwrap();
        assert!(loaded.recovered, "损坏文件应报告 recovered");
        assert_eq!(loaded.value, Sample::default());

        let backup = loaded.backup.expect("应产生备份文件");
        assert!(backup.exists(), "备份应保留");
        assert!(
            std::fs::read_to_string(&backup).unwrap().contains("not: valid"),
            "备份内容应为原始损坏内容"
        );
        assert!(!path.exists(), "损坏的原文件应已被隔离改名");
    }

    #[test]
    fn empty_file_is_treated_as_corrupt() {
        let dir = temp_dir("empty");
        let path = dir.join("sample.yml");
        std::fs::write(&path, "   \n").unwrap();

        let loaded = load_yaml::<Sample>(&path).unwrap();
        assert!(loaded.recovered);
    }

    #[test]
    fn json_round_trip_and_corruption() {
        let dir = temp_dir("json");
        let path = dir.join("state.json");
        let value = Sample { name: "p".into(), count: 3 };
        save_json(&path, &value).unwrap();
        assert_eq!(load_json::<Sample>(&path).unwrap().value, value);

        std::fs::write(&path, "{ broken").unwrap();
        let loaded = load_json::<Sample>(&path).unwrap();
        assert!(loaded.recovered);
        assert_eq!(loaded.value, Sample::default());
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let dir = temp_dir("tmp");
        let path = dir.join("data.bin");
        atomic_write(&path, b"hello").unwrap();
        assert!(path.exists());
        assert!(!tmp_path(&path).exists(), "临时文件应已被 rename 消耗");
    }
}
