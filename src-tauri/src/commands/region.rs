//! 地区访问限制：判定当前用户是否在**中国大陆**。
//!
//! # 数据来源：复用已有的 uapi，不额外接地理定位服务
//!
//! `get_network_myip` 直接返回公网 IP 与「地理位置（格式：国家 省份 城市）」，
//! 与本项目展示房间房主地区用的是**同一个服务**（`get_ip_info`）。这样既不用新增
//! 依赖、也不用新增密钥，更不必把用户 IP 再送给第二家服务。
//!
//! # 判定范围：只拦中国大陆
//!
//! 需求是「仅针对中国大陆生效，其他地区用户不受影响」，所以判定**刻意保守**：
//!
//! - 只有**明确识别为中国、且不是港澳台**才拦；
//! - 识别不出来（`region` 为空、服务不可达、格式看不懂、开关没配）一律**不拦**。
//!
//! 理由：漏拦一次只是限制没生效，误拦会让其他地区的用户直接失去功能——后者是
//! 不可接受的产品事故，前者可以靠服务恢复自愈。所以这里失败一律 open。
//!
//! 港澳台在 API 里可能写成 `中国 香港`，也可能写成 `中国香港`，两种都要认出来。

use serde::Serialize;
use uapi_sdk_rust::services::GetNetworkMyipParams;
use uapi_sdk_rust::Client as UapiClient;

/// 地区判定结果。前端只看 `mainland_china`，其余字段用于排查误判。
#[derive(Serialize)]
pub struct RegionInfo {
    /// 公网 IP（服务端视角）。
    pub ip: String,
    /// 服务返回的原始地区串（`国家 省份 城市`），**不要**在前端做二次判断。
    pub region: String,
    /// 是否为中国大陆。
    pub mainland_china: bool,
}

/// 是否属于**中国大陆**。
///
/// 纯函数，便于单测覆盖各种写法（这是唯一会把用户锁在门外的判断，
/// 错一次就是一次线上事故）。
pub fn is_mainland_china(region: &str) -> bool {
    let text = region.trim();
    if text.is_empty() {
        return false;
    }
    let lower = text.to_lowercase();

    // 国家只看**第一个词**，且英文要**整词**匹配：
    //
    // - 不用 `contains("中国")`：`美国 纽约 中国城` 这类地名会被误判；
    // - 不用 `starts_with("china")`：`Chinatown, San Francisco` 会被误判。
    //
    // 中文用前缀（`中国广东` 这种不带空格的写法也认），英文用整词。
    let head = text.split_whitespace().next().unwrap_or_default();
    let country_is_cn = head.starts_with("中国")
        || head.starts_with("中國")
        || matches!(head.to_lowercase().as_str(), "china" | "cn" | "prc");
    if !country_is_cn {
        return false;
    }

    // 港澳台不属于"中国大陆"，两种写法（`中国 香港` / `中国香港`）都要过滤。
    const EXCLUDED: [&str; 8] = [
        "香港",
        "澳门",
        "澳門",
        "台湾",
        "臺灣",
        "hong kong",
        "macao",
        "taiwan",
    ];
    !EXCLUDED.iter().any(|key| lower.contains(key))
}

/// 读取当前用户的地区并判定是否为中国大陆。
///
/// # 出错时前端应该怎么办
///
/// 调用方（`stores/region.ts`）把错误当成「未知地区」处理 = 不拦。
/// 这里不返回"看起来像中国大陆"的兜底值，也不吞错误：错误原样上抛，
/// 由前端决定降级方式，日志里也留得下原因。
#[tauri::command]
pub async fn get_region_info() -> Result<RegionInfo, String> {
    let client = UapiClient::builder()
        .build()
        .map_err(|e| format!("创建客户端失败: {}", e))?;

    let resp = client
        .network()
        .get_network_myip(GetNetworkMyipParams::new())
        .await
        .map_err(|e| {
            // 服务把原因放在 JSON 的 `message` 里，直接 to_string 会带一坨转义，
            // 与 `get_ip_info` 的处理保持一致。
            let msg = e.to_string();
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&msg);
            match parsed {
                Ok(val) => val
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(&msg)
                    .to_string(),
                Err(_) => msg,
            }
        })?;

    let region = resp.region.unwrap_or_default();
    Ok(RegionInfo {
        ip: resp.ip.unwrap_or_default(),
        mainland_china: is_mainland_china(&region),
        region,
    })
}

#[cfg(test)]
mod tests {
    use super::is_mainland_china;

    #[test]
    fn mainland_provinces_are_blocked() {
        assert!(is_mainland_china("中国 广东 深圳"));
        assert!(is_mainland_china("中国 北京"));
        assert!(is_mainland_china("中国 新疆 乌鲁木齐"));
    }

    /// 港澳台必须放行：需求是"仅针对中国大陆"。
    #[test]
    fn hong_kong_macau_taiwan_are_allowed() {
        assert!(!is_mainland_china("中国 香港"));
        assert!(!is_mainland_china("中国香港"));
        assert!(!is_mainland_china("中国 澳门"));
        assert!(!is_mainland_china("中国 台湾 台北"));
        assert!(!is_mainland_china("Taiwan"));
    }

    #[test]
    fn other_countries_are_allowed() {
        assert!(!is_mainland_china("美国 加利福尼亚 洛杉矶"));
        assert!(!is_mainland_china("日本 东京"));
        assert!(!is_mainland_china("United States California"));
    }

    /// 英文国家名要**整词**匹配：`Chinatown` 不是中国。
    #[test]
    fn chinatown_is_not_china() {
        assert!(!is_mainland_china("Chinatown, San Francisco"));
    }

    /// 识别不出来就不拦——宁可漏拦，不可误拦。
    #[test]
    fn unknown_or_empty_is_allowed() {
        assert!(!is_mainland_china(""));
        assert!(!is_mainland_china("   "));
        assert!(!is_mainland_china("未知"));
    }

    /// 国家段必须在开头：地名里出现"中国"不能算。
    #[test]
    fn country_must_be_the_leading_token() {
        assert!(!is_mainland_china("美国 纽约 中国城"));
    }

    #[test]
    fn english_spellings_are_recognized() {
        assert!(is_mainland_china("China Guangdong Shenzhen"));
        assert!(is_mainland_china("CN Guangdong"));
    }
}
