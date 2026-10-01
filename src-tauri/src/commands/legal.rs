//! 法务（EULA / 许可证）与"首个游戏"相关的命令。
//!
//! # 为什么"同意"不经过前端
//!
//! 前端只负责展示正文和"用户点了同意"这一事实。**版本号、正文哈希、时间戳
//! 全部由命令层自己取、自己算、自己生成**——前端拿不到也伪造不了
//! "我同意的是哪一版条款"。这样落盘的记录才是可审计的法律事实。
//!
//! # 拉不到条款时
//!
//! 不是错误，也不允许跳过：返回 `eula = null` 并带上[`crate::legal::EULA_FALLBACK_URL`]，
//! 界面提示"请同意软件最终许可协议（EULA）"并给官网链接；用户据此同意后，
//! 记录里会如实标注 `unfetched`（不知道正文内容，而不是假装知道）。

use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::legal::{self, EULA_FALLBACK_URL, UNFETCHED_VERSION};
use crate::mgr::AppMgr;
use crate::plugin::manager::PluginManager;
use crate::plugin::manifest::PluginKind;
use crate::setup::{EulaConsent, SetupState};

/// 一份可展示的法务文档。
#[derive(Debug, Clone, Serialize)]
pub struct LegalDocView {
    pub id: String,
    pub version: String,
    pub title: String,
    /// 是否需要用户显式同意（GPLv3 一类只需展示）。
    pub requires_acceptance: bool,
    /// 正文（Markdown 或纯文本）。
    pub text: String,
    /// 后端自己算出的 sha256（前端只用于展示与核对）。
    pub sha256: String,
}

/// 交给界面的一份完整法务状态。
#[derive(Debug, Clone, Serialize)]
pub struct LegalBundle {
    pub language: String,
    /// 需要用户同意的条款；`None` 表示拉不到，界面走"提示 + 官网链接"分支。
    pub eula: Option<LegalDocView>,
    /// 只需展示的附带文档（GPLv3 全文等）。
    pub attachments: Vec<LegalDocView>,
    /// 已记录的同意（来自 `Setting/setup.yml`）。
    pub accepted: Option<EulaConsent>,
    /// 当前是否需要用户（重新）同意：没同意过、或服务器版本/正文已变。
    pub needs_consent: bool,
    /// 拉不到条款时，界面按钮指向的官网。
    pub fallback_url: String,
    /// 拉取失败原因（放在次要位置展示，便于用户自查网络）。
    pub error: Option<String>,
}

/// 读取法务状态：清单 + 正文 + 已同意情况。
///
/// 拉不到条款**不是错误**：返回可用的兜底信息，让界面能给出官网链接。
#[tauri::command]
pub(crate) async fn legal_fetch_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    language: String,
) -> Result<LegalBundle, String> {
    let mgr = mgr.inner().clone();
    let data_dir = mgr.data_dir();
    let accepted = mgr.read().setup_state()?.eula_accepted;
    let client = reqwest::Client::new();

    let mut bundle = LegalBundle {
        language: language.clone(),
        eula: None,
        attachments: Vec::new(),
        accepted: accepted.clone(),
        // 拿不到条款一律视为"需要同意"：宁可多问一次，也不默认用户同意
        needs_consent: true,
        fallback_url: EULA_FALLBACK_URL.to_string(),
        error: None,
    };

    let manifest = match legal::fetch_manifest(&client, &data_dir).await {
        Ok(manifest) => manifest,
        Err(err) => {
            bundle.error = Some(err);
            return Ok(bundle);
        }
    };

    for doc in &manifest.documents {
        match legal::fetch_document_text(&client, &data_dir, doc, &language).await {
            Ok((text, sha256)) => {
                let view = LegalDocView {
                    id: doc.id.clone(),
                    version: doc.version.clone(),
                    title: doc.title.clone(),
                    requires_acceptance: doc.requires_acceptance,
                    text,
                    sha256,
                };
                if doc.requires_acceptance {
                    bundle.eula = Some(view);
                } else {
                    bundle.attachments.push(view);
                }
            }
            Err(err) => {
                if doc.requires_acceptance {
                    bundle.error = Some(err);
                } else {
                    // 附带文档（GPLv3）拉不到不影响主流程，但要留痕
                    eprintln!("[法务] 附带文档 {} 拉取失败: {err}", doc.id);
                }
            }
        }
    }

    bundle.needs_consent = match (&bundle.eula, &accepted) {
        (Some(doc), Some(acc)) => {
            acc.version != doc.version || !acc.sha256.eq_ignore_ascii_case(&doc.sha256)
        }
        // 有正文但没同意过，或正文都拿不到：都需要走同意流程
        _ => true,
    };

    Ok(bundle)
}

/// 记录用户同意条款。
///
/// 版本与哈希**由本命令自己去服务器取并计算**，不接受前端传入；
/// 取不到时如实记为 `unfetched`——记录里能一眼看出"同意时并没有拿到正文"。
#[tauri::command]
pub(crate) async fn accept_eula_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    language: String,
) -> Result<SetupState, String> {
    let mgr = mgr.inner().clone();
    let data_dir = mgr.data_dir();
    let client = reqwest::Client::new();

    let mut version = UNFETCHED_VERSION.to_string();
    let mut sha256 = UNFETCHED_VERSION.to_string();

    match legal::fetch_manifest(&client, &data_dir).await {
        Ok(manifest) => {
            if let Some(doc) = manifest.acceptance_document() {
                // 版本号拿到了就记真版本：即使正文没取到，也知道用户同意的是哪一版
                version = doc.version.clone();
                match legal::fetch_document_text(&client, &data_dir, doc, &language).await {
                    Ok((_, actual)) => sha256 = actual,
                    Err(err) => eprintln!("[法务] 同意时取正文失败，哈希记 unfetched: {err}"),
                }
            }
        }
        Err(err) => eprintln!("[法务] 同意时取清单失败，全部记 unfetched: {err}"),
    }

    mgr.write().accept_eula(&version, &sha256)?;
    mgr.read().setup_state()
}

/// 选择首个游戏（校验该 id 是否真实存在）。
#[tauri::command]
pub(crate) fn set_first_game_command(
    mgr: tauri::State<'_, Arc<AppMgr>>,
    plugins: tauri::State<'_, Arc<PluginManager>>,
    game_id: String,
) -> Result<SetupState, String> {
    let game_id = game_id.trim();
    if !plugins.games().iter().any(|g| g.id == game_id) {
        return Err(format!("未知的游戏 id「{game_id}」"));
    }
    mgr.write().set_game(game_id)?;
    mgr.read().setup_state()
}

/// 为某个游戏推荐"三个器"：适配 / 检测 / 耦合各一条。
///
/// 复用路由引擎（`PluginManager::plan`）而不是在这里另写一套评分逻辑——
/// 否则插件管理界面与 OOBE 会给出不一样的推荐，用户一眼就能看出矛盾。
#[tauri::command]
pub(crate) fn game_recommend_command(
    plugins: tauri::State<'_, Arc<PluginManager>>,
    game_id: String,
) -> Result<Value, String> {
    let game_id = game_id.trim();
    let profile = plugins
        .games()
        .into_iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| format!("未知的游戏 id「{game_id}」"))?;

    let mut recommendations = Map::new();
    for kind in [
        PluginKind::Adapter,
        PluginKind::Detector,
        PluginKind::Coupler,
    ] {
        let plan = plugins.plan(kind, Some(game_id));
        recommendations.insert(
            kind.as_str().to_string(),
            json!({
                "primary": plan.primary(),
                "candidates": plan
                    .candidates
                    .iter()
                    .map(|c| json!({
                        "pluginId": c.plugin_id,
                        "reason": c.reason,
                        "score": c.score,
                    }))
                    .collect::<Vec<_>>(),
            }),
        );
    }

    Ok(json!({
        "gameId": profile.id,
        "gameName": profile.name,
        "aliases": profile.aliases,
        "requiresCoupler": profile.requires_coupler,
        "recommendations": Value::Object(recommendations),
    }))
}
