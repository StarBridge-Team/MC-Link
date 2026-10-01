//! 法务文件（EULA 与许可证全文）的获取、校验与缓存。
//!
//! # 为什么条款放在资源服务器
//!
//! 条款文本会变，而客户端不能为了改一段文字就重新发版。因此服务器出一份
//! `legal/manifest.json` 声明文档、版本与各语言文件的 `sha256`，客户端按需拉取。
//!
//! # 三条硬规则
//!
//! 1. **哈希由后端算**：客户端只信自己从服务器拿到的正文，不信前端传进来的
//!    版本号或哈希。同意记录里落的 sha256 因此是可审计的。
//! 2. **拉不到不等于能跳过**：拉不到时界面只提示"请同意软件最终许可协议（EULA）"
//!    并给出[`EULA_FALLBACK_URL`]（编译期常量，不依赖网络），用户仍可据官网文本同意；
//!    此时落盘的同意记录会显式标注 `unfetched`，不假装知道条款内容。
//! 3. **缓存带校验**：缓存命中也要重算 sha256，防止本地文件被改过之后被当成"已同意的那份"。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::asset_server::{assets_server_url, join};

/// 拉不到条款时，界面上"请同意软件最终许可协议（EULA）"指向的官网。
///
/// 刻意是**编译期常量**而不是从服务器取：这条分支存在的意义正是"服务器拿不到"，
/// 再去服务器要链接就自相矛盾了。
pub const EULA_FALLBACK_URL: &str = "https://www.xigo.top/";

/// 拉取失败时写进同意记录的版本标记。
///
/// 让记录如实反映"同意时并没有拿到正文"——将来审计时能一眼看出差别，
/// 而不是伪装成一份正常的同意。
pub const UNFETCHED_VERSION: &str = "unfetched";

/// 服务器 `legal/manifest.json` 里的一份文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalDocument {
    /// 文档标识：`eula` / `gpl` / 将来的其它文档。
    pub id: String,
    /// 条款版本。实质变更时必须递增（客户端据此要求重新同意）。
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub title: String,
    /// 是否需要用户显式同意（`gpl` 只需展示，GPLv3 不要求用户"接受"）。
    ///
    /// 必须显式声明两个名字：服务器清单（`scripts/sync-assets.mjs` 产出）用 camelCase，
    /// 而手写/旧版清单可能是 snake_case。**漏了 rename 的后果很隐蔽**——字段恒为
    /// false，于是客户端找不出"需要同意的那份"，用户不用同意就能过引导。
    #[serde(default, rename = "requiresAcceptance", alias = "requires_acceptance")]
    pub requires_acceptance: bool,
    /// 语言 → 文件名。
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// 语言 → 该文件的小写十六进制 sha256。
    #[serde(default)]
    pub sha256: BTreeMap<String, String>,
}

/// 服务器 `legal/manifest.json`。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LegalManifest {
    #[serde(default)]
    pub documents: Vec<LegalDocument>,
}

impl LegalManifest {
    /// 找需要用户同意的那份条款（约定是 `requires_acceptance = true` 的那份）。
    pub fn acceptance_document(&self) -> Option<&LegalDocument> {
        self.documents.iter().find(|d| d.requires_acceptance)
    }

    /// 取指定 id 的文档（目前只有测试用它，等"关于"页需要单独取 GPL 时再公开使用）。
    #[cfg(test)]
    pub fn document(&self, id: &str) -> Option<&LegalDocument> {
        self.documents.iter().find(|d| d.id == id)
    }
}

/// 取出某语言的正文，并**用后端自己算的哈希**核对它与清单声明一致。
///
/// 返回 `(正文, 实际 sha256)`。清单里没给该语言的哈希时不校验（视为未声明，
/// 与资源同步那边的口径一致：缺 sha256 就不下载，这里因为要展示所以放行）。
fn verify_text(text: &str, expected: Option<&str>) -> Result<String, String> {
    let actual = sha256_hex(text.as_bytes());
    match expected.map(str::trim) {
        Some(exp) if !exp.is_empty() => {
            if actual.eq_ignore_ascii_case(exp) {
                Ok(actual)
            } else {
                Err(format!(
                    "条款正文哈希不符：清单声明 {exp}，实际 {actual}（拒绝使用这份正文）"
                ))
            }
        }
        _ => Ok(actual),
    }
}

/// 小写十六进制 SHA256。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 本地缓存目录（`<data_dir>/Assets/legal`）。
pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("Assets").join("legal")
}

/// 拉取服务器上的法务清单。
pub async fn fetch_manifest(
    client: &reqwest::Client,
    data_dir: &Path,
) -> Result<LegalManifest, String> {
    let url = join(&assets_server_url(data_dir), "legal/manifest.json");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("请求 {url} 失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("拉取法务清单失败：HTTP {status}（{url}）"));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取法务清单失败: {e}"))?;
    serde_json::from_str::<LegalManifest>(&text)
        .map_err(|e| format!("解析法务清单失败: {e}"))
}

/// 取某文档某语言的正文。
///
/// 顺序：本地缓存（校验通过即用）→ 服务器。缓存校验失败会**丢弃并重新下载**，
/// 而不是把被改过的正文当作"用户同意过的那份"继续使用。
pub async fn fetch_document_text(
    client: &reqwest::Client,
    data_dir: &Path,
    doc: &LegalDocument,
    language: &str,
) -> Result<(String, String), String> {
    let file = doc
        .files
        .get(language)
        .or_else(|| doc.files.get("en-US"))
        .ok_or_else(|| format!("条款 {} 没有可用语言（{language}）", doc.id))?;
    let expected = doc
        .sha256
        .get(language)
        .or_else(|| doc.sha256.get("en-US"))
        .map(String::as_str);

    let cached = cache_dir(data_dir).join(file);
    if let Ok(text) = std::fs::read_to_string(&cached) {
        if let Ok(actual) = verify_text(&text, expected) {
            return Ok((text, actual));
        }
        eprintln!(
            "[法务] 缓存 {} 哈希不符或损坏，丢弃并重新下载",
            cached.display()
        );
    }

    let url = join(&assets_server_url(data_dir), &format!("legal/{file}"));
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("请求 {url} 失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("拉取条款正文失败：HTTP {status}（{url}）"));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取条款正文失败: {e}"))?;

    // 先校验再落缓存：不把哈希不符的内容写进本地
    let actual = verify_text(&text, expected)?;
    if let Some(parent) = cached.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&cached, &text) {
        // 缓存写不进去不影响本次展示，但要留痕（否则"为什么每次都重新下载"无从查起）
        eprintln!("[法务] 写入缓存 {} 失败: {e}", cached.display());
    }
    Ok((text, actual))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_when_expected_is_given() {
        let text = "hello";
        let actual = sha256_hex(text.as_bytes());
        assert_eq!(actual, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert!(verify_text(text, Some(&actual)).is_ok());
        // 大小写不敏感
        assert!(verify_text(text, Some(&actual.to_uppercase())).is_ok());
    }

    #[test]
    fn mismatched_hash_is_rejected() {
        let err = verify_text("hello", Some("deadbeef")).unwrap_err();
        assert!(err.contains("哈希不符"), "报错要说明原因: {err}");
    }

    #[test]
    fn missing_hash_is_tolerated() {
        // 清单没声明哈希时放行（与"缺 sha256 不下载"的资源同步口径区分开：
        // 条款是必须展示给用户看的，宁可不校验也不能让人看不到）
        assert!(verify_text("hello", None).is_ok());
        assert!(verify_text("hello", Some("   ")).is_ok());
    }

    #[test]
    fn acceptance_document_is_the_one_flagged() {
        let manifest = LegalManifest {
            documents: vec![
                LegalDocument {
                    id: "gpl".into(),
                    version: "3.0".into(),
                    title: "GPLv3".into(),
                    requires_acceptance: false,
                    files: BTreeMap::new(),
                    sha256: BTreeMap::new(),
                },
                LegalDocument {
                    id: "eula".into(),
                    version: "1.0".into(),
                    title: "EULA".into(),
                    requires_acceptance: true,
                    files: BTreeMap::new(),
                    sha256: BTreeMap::new(),
                },
            ],
        };
        assert_eq!(manifest.acceptance_document().map(|d| d.id.as_str()), Some("eula"));
        assert_eq!(manifest.document("gpl").map(|d| d.version.as_str()), Some("3.0"));
    }

    #[test]
    fn parses_server_manifest_shape() {
        // 与 scripts/sync-assets.mjs 的 prepareLegal 产出的字段形状保持一致
        let json = r#"{"documents":[
            {"id":"gpl","version":"3.0","title":"GNU General Public License v3",
             "requiresAcceptance":false,"files":{"en-US":"GPL-3.0.txt"},
             "sha256":{"en-US":"abc"}},
            {"id":"eula","version":"1.0","title":"End User License Agreement",
             "requiresAcceptance":true,"files":{"zh-CN":"EULA-zh-CN.md"},"sha256":{"zh-CN":"def"}}
        ]}"#;
        let manifest: LegalManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.documents.len(), 2);
        assert_eq!(manifest.acceptance_document().map(|d| d.id.as_str()), Some("eula"));
        assert_eq!(
            manifest.document("eula").and_then(|d| d.files.get("zh-CN").map(String::as_str)),
            Some("EULA-zh-CN.md")
        );
    }
}
