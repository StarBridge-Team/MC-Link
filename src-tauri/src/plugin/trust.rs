//! 插件包的签名与信任判定。
//!
//! # 信任必须来自外部
//!
//! 早期设计让 `plugin.json` 自带 `signature` 字段并据此提权，那等于把"自述即可信"
//! 写进权限模型：任何人填一对假公钥假签名就能拿到 [`crate::plugin::permission::TrustLevel::Verified`]
//! 的权限上限。现在信任只来自两处，**都在插件之外**：
//!
//! 1. 客户端内置的发布者公钥（[`PUBLISHER_PUBKEY`]）；
//! 2. 安装器对**整个插件包**的签名校验结果（[`Verdict::Verified`]）。
//!
//! 插件自身的任何字段都不再影响信任等级。
//!
//! # 为什么复用更新包的密钥
//!
//! Tauri 的更新签名用的就是 minisign（`.tauri/updater.key` + `tauri.conf.json` 的
//! `plugins.updater.pubkey`）。插件包复用同一套密钥格式与同一个签名工具
//! （`tauri signer`），好处是：一把钥匙、一份轮换流程、一套工具，而不是两套。
//! [`PUBLISHER_PUBKEY`] 应当与 `plugins.updater.pubkey` 填同一个值。
//!
//! # 未配置公钥时的行为
//!
//! 一律 [`Verdict::NotConfigured`]：可以安装，但**不授予任何信任**（停留在 `Unsigned`）。
//! 这是 fail-closed —— 没有密钥就不会因为"忘了配"而把某个插件提到最高权限。

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};
use std::path::Path;

const B64: base64::engine::general_purpose::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// 解析内置公钥。
///
/// 格式与 Tauri 更新插件的 `plugins.updater.pubkey` 一致：**整个 `.pub` 文件内容
/// 的 base64**（该文件是两行文本：`untrusted comment: …` + 密钥行）。
/// Tauri CLI 生成的 `.pub` 文件本身就是这个 base64 串，因此可以直接粘贴过来。
///
/// 注意不能直接用 `PublicKey::from_base64`——那个接口只接受**单独那行密钥**，
/// 会把整份文件的 base64 判为非法编码。
fn parse_public_key(b64: &str) -> Result<PublicKey, String> {
    let bytes = B64
        .decode(b64.trim())
        .map_err(|e| format!("公钥 base64 解码失败: {}", e))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("公钥不是合法 UTF-8: {}", e))?;
    PublicKey::decode(&text).map_err(|e| format!("公钥解析失败: {}", e))
}

/// 解析签名。
///
/// 接受两种写法，且**只有这两种**：
/// - 多行原文（`minisign` / `tauri signer sign` 写出的 `.sig` 文件内容）；
/// - 该原文的 base64（`tauri signer sign` 打印到终端的"Public signature"，也是
///   更新清单 `tauri.json` 里存的形态）。
///
/// 判别方式很直接：minisign 签名原文固定是 4 行，**含换行即视为原文**，否则视为 base64。
/// 放宽输入格式不会削弱安全性——签名终究要通过密码学校验，这里只是省掉调用方
/// 在两处不同格式之间转换的麻烦。
fn parse_signature(raw: &str) -> Result<Signature, String> {
    if raw.contains('\n') {
        return Signature::decode(raw).map_err(|e| format!("签名格式非法: {}", e));
    }
    let bytes = B64
        .decode(raw.trim())
        .map_err(|e| format!("签名 base64 解码失败: {}", e))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("签名不是合法 UTF-8: {}", e))?;
    Signature::decode(&text).map_err(|e| format!("签名格式非法: {}", e))
}

/// 内置的发布者公钥（minisign `.pub` 文件内容，base64）。
///
/// 当前值与 `tauri.conf.json` 的 `plugins.updater.pubkey` **同一个**：同一把密钥
/// 既签更新包也签插件包。两处分居源码与配置，换密钥时最容易漏改一处，因此
/// `publisher_pubkey_matches_updater_config` 这条用例专门盯着它们不漂移。
///
/// 留空表示尚未配置：插件可以安装，但都停留在 `Unsigned`（fail-closed）。
const PUBLISHER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ3NTRBMjI0Q0UwMkZFRTUKUldUbC9nTE9KS0pVUjg4R3R0S0xQL2pIZjRVSFRYUVE2S3l2WlpTL21CSURZL1RSVlp2VXpKZDgK";

/// 客户端是否已配置发布者公钥。
pub fn signing_configured() -> bool {
    !PUBLISHER_PUBKEY.trim().is_empty()
}

/// 验签结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 签名有效，且客户端配置了可信公钥 → 可授予 `Verified`。
    Verified,
    /// 客户端未配置发布者公钥，签名校验不可用。
    ///
    /// 此时**不授予信任**，但也不拒绝安装——否则在配好密钥之前没人能用插件，
    /// 而权限模型本身已经把 `Unsigned` 限制在合理范围内。
    NotConfigured,
    /// 签名缺失、格式非法或校验不通过 → 必须拒绝安装。
    Invalid(String),
}

impl Verdict {
    /// 是否可以不安装？只有校验失败才拒绝。
    pub fn acceptable(&self) -> bool {
        !matches!(self, Verdict::Invalid(_))
    }

    /// 是否授予 `Verified`。
    pub fn grants_verified(&self) -> bool {
        matches!(self, Verdict::Verified)
    }
}

/// 用内置公钥校验插件包。
///
/// `signature` 是包旁 `.sig` 文件的内容（minisign 签名，可能多行）。
pub fn verify_package(package: &[u8], signature: Option<&str>) -> Verdict {
    verify_with(PUBLISHER_PUBKEY, package, signature)
}

/// 实际实现：公钥显式传入，便于测试用固定向量驱动**真实**的校验路径。
fn verify_with(pubkey_b64: &str, package: &[u8], signature: Option<&str>) -> Verdict {
    let pubkey_b64 = pubkey_b64.trim();
    if pubkey_b64.is_empty() {
        return Verdict::NotConfigured;
    }

    let Some(signature) = signature.map(str::trim).filter(|s| !s.is_empty()) else {
        return Verdict::Invalid("插件包缺少签名文件".to_string());
    };

    let public_key = match parse_public_key(pubkey_b64) {
        Ok(k) => k,
        Err(e) => return Verdict::Invalid(format!("内置公钥无法解析: {}", e)),
    };
    let signature = match parse_signature(signature) {
        Ok(s) => s,
        Err(e) => return Verdict::Invalid(e),
    };

    // `allow_legacy = false`：只接受 minisign 的现行格式，
    // 不开"兼容旧式签名"的口子。
    match public_key.verify(package, &signature, false) {
        Ok(()) => Verdict::Verified,
        Err(e) => Verdict::Invalid(format!("签名校验未通过: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// 目录级校验：把"整包验签"落到"插件目录"上
// ---------------------------------------------------------------------------
//
// 插件是按**目录**安装的（`<data_dir>/Plugins/<id>/`），而签名只能覆盖字节。
// 约定的桥接方式是目录内放两个文件：
//
//     integrity.txt   每行 `<sha256><两个空格><相对路径>`，按路径升序，LF，UTF-8 无 BOM
//     integrity.sig   `integrity.txt` 的 minisign 签名（原文或 base64 均可）
//
// 校验分两步，**缺一不可**：
//   1. 用发布者公钥验 `integrity.txt` 的签名 —— 证明这份清单确实来自发布者；
//   2. 逐条核对清单内每个文件的哈希，**并确认目录内没有清单之外的文件**。
//
// 第 2 步的后半句才是这张清单的价值所在：只签清单却不检查"有没有多余文件"的话，
// 往已签名的插件目录里塞一个额外的 exe，签名依旧"有效"。

/// 完整性清单文件名。
pub const INTEGRITY_FILE: &str = "integrity.txt";
/// 完整性清单的签名文件名。
pub const INTEGRITY_SIG_FILE: &str = "integrity.sig";
/// 允许存在但不必出现在清单里的文件（由核心生成，不属于发布内容）。
const UNLISTED_ALLOWED: &[&str] = &["secret.key", INTEGRITY_FILE, INTEGRITY_SIG_FILE];

/// 校验一个插件目录可否授予 `Verified`。
///
/// 目录内没有 `integrity.txt` → [`Verdict::NotConfigured`]：视为"未验签"，
/// 插件仍可被用户**显式启用**，但只拿最小权限。这样既能对已有安装 fail-closed，
/// 又不会因为本次收紧就让它们直接消失；待安装器落地后，可在安装入口把
/// "缺签名"进一步升级为拒绝安装。
pub fn verify_directory(dir: &Path) -> Verdict {
    verify_directory_with(PUBLISHER_PUBKEY, dir)
}

fn verify_directory_with(pubkey_b64: &str, dir: &Path) -> Verdict {
    let list_path = dir.join(INTEGRITY_FILE);
    if !list_path.is_file() {
        return Verdict::NotConfigured;
    }
    let list_text = match std::fs::read_to_string(&list_path) {
        Ok(t) => t,
        Err(e) => return Verdict::Invalid(format!("读取 {} 失败: {}", INTEGRITY_FILE, e)),
    };
    let signature = std::fs::read_to_string(dir.join(INTEGRITY_SIG_FILE)).ok();

    // 清单在但签名不在，属于"半截状态"，按拒绝处理（正常包不会长这样）
    match verify_with(pubkey_b64, list_text.as_bytes(), signature.as_deref()) {
        Verdict::Verified => {}
        other => return other,
    }

    match check_integrity_list(dir, &list_text) {
        Ok(()) => Verdict::Verified,
        Err(e) => Verdict::Invalid(e),
    }
}

/// 逐条核对完整性清单，并确认目录内没有清单之外的文件。
fn check_integrity_list(dir: &Path, text: &str) -> Result<(), String> {
    let mut listed: Vec<String> = Vec::new();

    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let (Some(hash), Some(rel)) = (parts.next(), parts.next()) else {
            return Err(format!(
                "{} 第 {} 行格式非法（应为 `<sha256> <相对路径>`）",
                INTEGRITY_FILE,
                idx + 1
            ));
        };
        let rel = rel.trim().replace('\\', "/");
        validate_rel_path(&rel)?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "{} 第 {} 行的 sha256 非法（应为 64 位十六进制）",
                INTEGRITY_FILE,
                idx + 1
            ));
        }

        let actual = sha256_of_file(&dir.join(&rel))
            .map_err(|e| format!("清单里的 {}：{}", rel, e))?;
        if !actual.eq_ignore_ascii_case(hash) {
            return Err(format!("{} 的内容与清单不符（已损坏或被人改动）", rel));
        }
        listed.push(rel);
    }

    if listed.is_empty() {
        return Err(format!("{} 里没有任何文件条目", INTEGRITY_FILE));
    }

    for found in walk_files(dir, dir)? {
        let rel = found.replace('\\', "/");
        if UNLISTED_ALLOWED.contains(&rel.as_str()) || listed.contains(&rel) {
            continue;
        }
        return Err(format!(
            "目录内存在清单之外的文件: {}（已签名的包不得夹带其他文件）",
            rel
        ));
    }

    Ok(())
}

/// 拒绝绝对路径与 `..`：清单不能指向插件目录之外。
fn validate_rel_path(rel: &str) -> Result<(), String> {
    let bad = rel.is_empty()
        || rel.starts_with('/')
        || rel.starts_with('\\')
        || rel.contains(':')
        || rel.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..");
    if bad {
        return Err(format!("{} 里的路径非法: {}", INTEGRITY_FILE, rel));
    }
    Ok(())
}

/// 递归列出目录下所有文件（相对 `base` 的路径）。
fn walk_files(base: &Path, dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("读取目录 {} 失败: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(base, &path)?);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().to_string());
        }
    }
    Ok(out)
}

/// 流式计算文件 sha256。
///
/// 刻意不整文件读入内存：插件产物可能十几 MB，而校验发生在启动路径上。
fn sha256_of_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("打开失败: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf).map_err(|e| format!("读取失败: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试专用的固定向量。
    ///
    /// 密钥对是用 `pnpm exec tauri signer generate` 在临时目录里现场生成的一次性密钥，
    /// **私钥未进入仓库**；这里只固化"公钥 + 消息 + 签名"三元组，用来驱动真实的
    /// minisign 校验路径。同一条消息换一把密钥签名必然校验失败，因此这些向量
    /// 能有效锁住"校验确实在做事"。
    const VECTOR_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEI3REExMTMxMkExNzgyQjEKUldTeGdoY3FNUkhhdDRWejM1S3J3Q0E2VW9UdkxnU09WWWJ3STY4dWxIQmNOUEFWYUhiQUF5UWQK";
    const VECTOR_MESSAGE: &[u8] = b"mclink-test-vector-v1";
    const VECTOR_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTeGdoY3FNUkhhdDF5MVluemo5VVF0K1dNNkhTeDRhTUtKOVZlMGVJZHFLUjVZN2Z2eDRKZUlVYllnUnRybDRmSWk4aUJmOXNBdk9DcHVMeFRVdVhLZHZwS0RoS0VWOGdrPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwODIxOTA1CWZpbGU6bXNnLmJpbgpSazQ1UjkzZXJGd2tsOEQ0dW1zcnZZWWxEYXh5eXIyRnQwanhiL2xPNWdLbnYyQUVlOEtHYnJqMDlzbHBSVVAzS1RmOVZrTWZKWisxRUdRcG52R3BBQT09Cg==";

    /// `.sig` 文件里的**多行原文**形态（`tauri signer sign` 写盘的就是这个）。
    const VECTOR_SIGNATURE_TEXT: &str = "untrusted comment: signature from tauri secret key\nRUSxghcqMRHat1y1Ynzj9UQt+WM6HSx4aMKJ9Ve0eIdqKR5Y7fvx4JeIUbYgRtrl4fIi8iBf9sAvOCpuLxTUuXKdvpKDhKEV8gk=\ntrusted comment: timestamp:1790821905\tfile:msg.bin\nRk45R93erFwkl8D4umsrvYYlDaxyyr2Ft0jxb/lO5gKnv2AEe8KGbrj09slpRUP3KTf9VkMfJZ+1EGQpnvGpAA==";

    #[test]
    fn valid_signature_is_verified() {
        assert_eq!(
            verify_with(VECTOR_PUBKEY, VECTOR_MESSAGE, Some(VECTOR_SIGNATURE)),
            Verdict::Verified
        );
        assert!(Verdict::Verified.grants_verified());
        assert!(Verdict::Verified.acceptable());
    }

    /// 同一份签名的两种常见写法（base64 与 `.sig` 原文）都必须能通过。
    /// 更新清单里存 base64，而 `tauri signer sign` 落盘写原文，两边都要能用。
    #[test]
    fn accepts_both_signature_encodings() {
        assert_eq!(
            verify_with(VECTOR_PUBKEY, VECTOR_MESSAGE, Some(VECTOR_SIGNATURE_TEXT)),
            Verdict::Verified,
            "多行原文形态应通过"
        );
        assert_eq!(
            verify_with(VECTOR_PUBKEY, VECTOR_MESSAGE, Some(VECTOR_SIGNATURE)),
            Verdict::Verified,
            "base64 形态应通过"
        );
    }

    /// 最关键的一条：包被改动过，签名就不能再通过。
    #[test]
    fn tampered_package_is_rejected() {
        let mut tampered = VECTOR_MESSAGE.to_vec();
        tampered.push(b'!');
        match verify_with(VECTOR_PUBKEY, &tampered, Some(VECTOR_SIGNATURE)) {
            Verdict::Invalid(_) => {}
            other => panic!("被篡改的包必须被拒绝，实际得到 {:?}", other),
        }
        // 拒绝的结论必须让调用方无法继续安装
        assert!(!verify_with(VECTOR_PUBKEY, &tampered, Some(VECTOR_SIGNATURE)).acceptable());
    }

    /// 另一把真实密钥的公钥（同样是现场生成、私钥未入库）。
    ///
    /// 这里必须用**另一把真钥匙**，不能靠在 base64 尾部追加字符来伪造：
    /// base64 解码器会忽略末尾那个凑不成组的字符，解出来的还是原来那 42 字节密钥，
    /// 于是"换了一把钥匙"的假设根本不成立（这一点是实际写测试时踩出来的）。
    const OTHER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk5NDE3NzVERUU2OTdBOEYKUldTUGVtbnVYWGRCbWJxeUlGdjhDZldkaWJUaUMvVzVLbkZ2L2NWNmxDSWZaMkhBMjNiSGc2amwK";

    /// 别的发布者签出来的包，不得通过。
    #[test]
    fn signature_from_other_publisher_is_rejected() {
        match verify_with(OTHER_PUBKEY, VECTOR_MESSAGE, Some(VECTOR_SIGNATURE)) {
            Verdict::Invalid(_) => {}
            other => panic!("非发布者公钥不得通过，实际得到 {:?}", other),
        }
        // 反向确认：这两把公钥确实不同（否则上面的断言毫无意义）
        assert_ne!(OTHER_PUBKEY, VECTOR_PUBKEY);
    }

    #[test]
    fn missing_or_garbage_signature_is_rejected() {
        assert!(matches!(
            verify_with(VECTOR_PUBKEY, VECTOR_MESSAGE, None),
            Verdict::Invalid(_)
        ));
        assert!(matches!(
            verify_with(VECTOR_PUBKEY, VECTOR_MESSAGE, Some("   ")),
            Verdict::Invalid(_)
        ));
        assert!(matches!(
            verify_with(VECTOR_PUBKEY, VECTOR_MESSAGE, Some("not-base64!!")),
            Verdict::Invalid(_)
        ));
    }

    /// fail-closed：没配公钥时，签名再"像样"也不授予信任。
    #[test]
    fn unconfigured_pubkey_grants_nothing() {
        let verdict = verify_with("", VECTOR_MESSAGE, Some(VECTOR_SIGNATURE));
        assert_eq!(verdict, Verdict::NotConfigured);
        assert!(!verdict.grants_verified());
        // 允许安装（否则配密钥前无人能用插件），但权限仍是 Unsigned
        assert!(verdict.acceptable());
    }

    /// 出厂状态只认发布者的签名：测试向量用的是另一把密钥，必须被判为无效。
    /// 未配置公钥时应是 `NotConfigured`（不授予信任，但也不炸）。
    #[test]
    fn shipping_default_only_trusts_the_publisher() {
        let verdict = verify_package(VECTOR_MESSAGE, Some(VECTOR_SIGNATURE));
        if signing_configured() {
            assert!(
                matches!(verdict, Verdict::Invalid(_)),
                "非发布者密钥签出来的包必须被拒绝，实际 {:?}",
                verdict
            );
            assert!(!verdict.grants_verified());
        } else {
            assert_eq!(verdict, Verdict::NotConfigured);
        }
    }

    /// 内置公钥必须与 `tauri.conf.json` 的 `plugins.updater.pubkey` 同值。
    ///
    /// 两处分居源码与配置，是换密钥时最容易漏改一处的地方。漏了不会报错，
    /// 只会表现为"插件包/更新包签名莫名其妙校验不过"——正是要提前拦住的类型。
    #[test]
    fn publisher_pubkey_matches_updater_config() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        let conf = std::fs::read_to_string(&path).expect("读取 tauri.conf.json 失败");
        let json: serde_json::Value =
            serde_json::from_str(&conf).expect("tauri.conf.json 不是合法 JSON");
        let updater = json["plugins"]["updater"]["pubkey"].as_str().unwrap_or("").trim();
        assert_eq!(
            PUBLISHER_PUBKEY.trim(),
            updater,
            "插件发布者公钥与更新公钥必须同值（一把钥匙签两种包）"
        );
    }

    // ---- 目录级校验 ----

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mclink-trust-{}-{}",
            tag,
            crate::plugin::crypto::random_hex(6)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(dir: &Path, rel: &str, content: &[u8]) {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full, content).unwrap();
    }

    fn entry(dir: &Path, rel: &str) -> String {
        format!("{}  {}", sha256_of_file(&dir.join(rel)).unwrap(), rel)
    }

    #[test]
    fn intact_directory_passes_the_list_check() {
        let dir = temp_dir("intact");
        write_file(&dir, "plugin.json", b"{}");
        write_file(&dir, "bin/adapter.exe", b"binary");
        let list = format!("{}\n{}\n", entry(&dir, "bin/adapter.exe"), entry(&dir, "plugin.json"));

        assert!(check_integrity_list(&dir, &list).is_ok());

        // 核心生成的 secret.key 不在清单里也应放行
        write_file(&dir, "secret.key", b"0011");
        assert!(check_integrity_list(&dir, &list).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modified_file_breaks_the_list_check() {
        let dir = temp_dir("tamper");
        write_file(&dir, "plugin.json", b"{}");
        let list = format!("{}\n", entry(&dir, "plugin.json"));

        // 签名之后文件被改
        write_file(&dir, "plugin.json", b"{\"evil\":1}");
        let err = check_integrity_list(&dir, &list).unwrap_err();
        assert!(err.contains("与清单不符"), "实际: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 这条是整张清单的核心价值：夹带一个清单外的 exe 必须被拒。
    #[test]
    fn smuggled_file_breaks_the_list_check() {
        let dir = temp_dir("smuggle");
        write_file(&dir, "plugin.json", b"{}");
        let list = format!("{}\n", entry(&dir, "plugin.json"));

        write_file(&dir, "bin/extra.exe", b"payload");
        let err = check_integrity_list(&dir, &list).unwrap_err();
        assert!(err.contains("清单之外"), "实际: {}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_or_escaping_list_is_rejected() {
        let dir = temp_dir("malformed");
        write_file(&dir, "plugin.json", b"{}");

        // 格式非法
        assert!(check_integrity_list(&dir, "no-hash-here\n").is_err());
        // sha256 长度不对
        assert!(check_integrity_list(&dir, "abc  plugin.json\n").is_err());
        // 路径逃逸
        let escape = format!("{}  ../outside.txt\n", "a".repeat(64));
        let err = check_integrity_list(&dir, &escape).unwrap_err();
        assert!(err.contains("路径非法"), "实际: {}", err);
        // 空清单
        assert!(check_integrity_list(&dir, "\n\n").is_err());
        // 清单里的文件不存在
        let missing = format!("{}  bin/gone.exe\n", "b".repeat(64));
        assert!(check_integrity_list(&dir, &missing).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn directory_without_integrity_files_is_unsigned_not_invalid() {
        // 已有安装（本次收紧前装的插件）没有清单文件：必须是"未验签"而不是"拒绝"，
        // 否则升级后所有第三方插件会直接消失。
        let dir = temp_dir("nolist");
        write_file(&dir, "plugin.json", b"{}");
        let verdict = verify_directory_with(VECTOR_PUBKEY, &dir);
        assert_eq!(verdict, Verdict::NotConfigured);
        assert!(!verdict.grants_verified());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_without_signature_is_invalid() {
        // 半截状态（有清单没签名）按拒绝处理
        let dir = temp_dir("nosig");
        write_file(&dir, "plugin.json", b"{}");
        write_file(&dir, INTEGRITY_FILE, format!("{}\n", entry(&dir, "plugin.json")).as_bytes());
        let verdict = verify_directory_with(VECTOR_PUBKEY, &dir);
        assert!(matches!(verdict, Verdict::Invalid(_)), "实际 {:?}", verdict);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
