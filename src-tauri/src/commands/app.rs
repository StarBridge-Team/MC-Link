use std::sync::Arc;
use crate::config::{InitAppData, PrepareAppData, IpInfo};
use crate::assets::pull::SyncOutcome;
use crate::mgr::AppMgr;
use uapi_sdk_rust::services::GetNetworkIpinfoParams;
use uapi_sdk_rust::Client as UapiClient;

#[tauri::command]
pub(crate) fn get_app_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// 获取 Tauri 框架版本（从 Cargo.lock 编译时读取）
#[tauri::command]
pub(crate) fn get_tauri_version() -> String {
    let lock = include_str!("../../Cargo.lock");
    let mut in_tauri = false;
    for line in lock.lines() {
        let t = line.trim();
        if t == "name = \"tauri\"" {
            in_tauri = true;
        } else if in_tauri && t.starts_with("version = ") {
            return t.trim_start_matches("version = ")
                .trim_matches('"')
                .to_string();
        } else if in_tauri && t.starts_with('[') {
            break;
        }
    }
    "2.x".to_string()
}

#[tauri::command]
pub(crate) async fn get_ip_info(host: String) -> Result<IpInfo, String> {
    let client = UapiClient::builder().build().map_err(|e| format!("创建客户端失败: {}", e))?;
    let params = GetNetworkIpinfoParams::new(&host);
    let resp = client.network().get_network_ipinfo(params).await.map_err(|e| {
        let msg = e.to_string();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&msg);
        if let Ok(val) = parsed {
            val.get("message").and_then(|m| m.as_str()).unwrap_or(&msg).to_string()
        } else {
            msg
        }
    })?;
    Ok(IpInfo {
        region: resp.region.unwrap_or_default(),
        isp: resp.isp.unwrap_or_default(),
    })
}

/// 仅加载配置（轻量快速，无网络请求）
#[tauri::command]
pub(crate) fn init_app(mgr: tauri::State<'_, Arc<AppMgr>>) -> Result<InitAppData, String> {
    let pers = mgr.read().personalization().unwrap_or_default();
    let default_effect = crate::effect::get_default_effect();
    let app_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let tauri_version = get_tauri_version();
    Ok(InitAppData { personalization: pers, default_effect, app_version, tauri_version })
}

/// 准备应用：检查资产服务器 manifest 版本，不匹配则清理重下；匹配则跳过。
#[tauri::command]
pub(crate) async fn prepare_app(
    mgr: tauri::State<'_, Arc<AppMgr>>,
) -> Result<PrepareAppData, String> {
    let pers = mgr.read().personalization().unwrap_or_default();
    let default_effect = crate::effect::get_default_effect();
    let app_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let tauri_version = get_tauri_version();

    // sync_assets 内部对比 manifest.version：一致则跳过，不一致则清理重下
    let outcome = match mgr.pull().sync_assets().await {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[prepare_app] sync_assets 失败: {}", e);
            SyncOutcome::default()
        }
    };

    Ok(PrepareAppData {
        personalization: pers,
        default_effect,
        app_version,
        tauri_version,
        bootstrap_icons_ready: outcome.bi_ready,
        fonts_ready: outcome.fonts_ready,
        icon_ready: outcome.icon_ready,
    })
}
