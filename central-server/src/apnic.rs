//! APNIC 亚太地区 IP 白名单
//!
//! 每日 24:00 从 APNIC 下载最新分配数据，仅允许此名单内的 IP 连接。
//! 数据来源: http://ftp.apnic.net/apnic/stats/apnic/delegated-apnic-latest

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use mc_link_common::log::{log, LogLevel};

/// APNIC 白名单配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApnicConfig {
    pub enabled: bool,
    pub countries: Vec<String>,
}

impl Default for ApnicConfig {
    fn default() -> Self {
        Self { enabled: false, countries: Vec::new() }
    }
}

/// 解析后的 IP 范围
#[derive(Debug)]
struct IpRange {
    start: u128,
    end: u128,
}

/// APNIC 白名单（线程安全）
pub struct ApnicWhitelist {
    inner: RwLock<WhitelistInner>,
}

struct WhitelistInner {
    ipv4_ranges: Vec<IpRange>,
    ipv6_ranges: Vec<IpRange>,
    last_updated: String,
    config: ApnicConfig,
}

impl ApnicWhitelist {
    pub fn new(config: ApnicConfig) -> Arc<Self> {
        let enabled = config.enabled;
        let list = Arc::new(Self {
            inner: RwLock::new(WhitelistInner {
                ipv4_ranges: Vec::new(),
                ipv6_ranges: Vec::new(),
                last_updated: String::new(),
                config,
            }),
        });

        // 首次立即刷新
        if enabled {
            let list_clone = list.clone();
            std::thread::spawn(move || {
                list_clone.refresh();
            });
        }

        list
    }

    /// 启动每日刷新线程（在首次刷新完成后调用）
    pub fn start_daily_refresh(self: &Arc<Self>) {
        let enabled = self.inner.read().unwrap_or_else(|e| e.into_inner()).config.enabled;
        if !enabled {
            return;
        }

        let this = self.clone();
        std::thread::spawn(move || {
            loop {
                let secs_to_midnight = secs_until_midnight();
                log(LogLevel::Info, &format!(
                    "[APNIC] 下次刷新: {} 后",
                    format_duration(secs_to_midnight),
                ));

                std::thread::sleep(Duration::from_secs(secs_to_midnight));

                log(LogLevel::Info, "[APNIC] 开始每日刷新...");
                this.refresh();
            }
        });
    }

    /// 检查 IP 是否在白名单中
    pub fn is_allowed(&self, ip: IpAddr) -> bool {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        if !inner.config.enabled {
            return true;
        }
        if inner.ipv4_ranges.is_empty() && inner.ipv6_ranges.is_empty() {
            return false; // 白名单未加载时拒绝所有
        }

        match ip {
            IpAddr::V4(v4) => {
                let val = ipv4_to_u128(v4);
                is_in_ranges(&inner.ipv4_ranges, val)
            }
            IpAddr::V6(v6) => {
                let val = ipv6_to_u128(v6);
                is_in_ranges(&inner.ipv6_ranges, val)
            }
        }
    }

    /// 刷新白名单数据（下载 + 解析）
    fn refresh(&self) {
        let countries = {
            let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
            inner.config.countries.clone()
        };
        match download_and_parse(&countries) {
            Ok(Some((v4, v6, date))) => {
                let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
                inner.ipv4_ranges = v4;
                inner.ipv6_ranges = v6;
                inner.last_updated = date.clone();
                log(LogLevel::Info, &format!(
                    "[APNIC] 白名单已更新: IPv4={} 段, IPv6={} 段, 日期={}",
                    inner.ipv4_ranges.len(),
                    inner.ipv6_ranges.len(),
                    inner.last_updated,
                ));
            }
            Ok(None) => {
                log(LogLevel::Warn, "[APNIC] 下载成功但无有效数据");
            }
            Err(e) => {
                log(LogLevel::Error, &format!("[APNIC] 下载失败: {}", e));
            }
        }
    }
}

/// 下载并解析 APNIC 委托统计文件
fn download_and_parse(countries: &[String]) -> Result<Option<(Vec<IpRange>, Vec<IpRange>, String)>, String> {
    let url = "http://ftp.apnic.net/apnic/stats/apnic/delegated-apnic-latest";

    let response = reqwest::blocking::get(url)
        .map_err(|e| format!("HTTP 请求失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP 状态码: {}", response.status()));
    }

    let text = response.text()
        .map_err(|e| format!("读取响应体失败: {}", e))?;

    parse_delegated_file(&text, countries)
}

/// 解析 APNIC 委托文件内容
fn parse_delegated_file(content: &str, countries: &[String]) -> Result<Option<(Vec<IpRange>, Vec<IpRange>, String)>, String> {
    let mut ipv4 = Vec::new();
    let mut ipv6 = Vec::new();
    let mut file_date = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            // 提取日期注释: # apnic|...|20250101
            if line.starts_with("# apnic|") || line.starts_with("# apnic,") {
                if let Some(date_str) = line.rsplit('|').next().or_else(|| line.rsplit(',').next()) {
                    let date = date_str.trim();
                    if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) {
                        file_date = date.to_string();
                    }
                }
            }
            continue;
        }

        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 7 {
            continue;
        }

        let registry = parts[0];
        if registry != "apnic" {
            continue;
        }

        let country = parts[1];
        let ip_type = parts[2];
        let start_str = parts[3];
        let count_str = parts[4];
        let status = parts[6];

        // 仅处理已分配的记录
        if status != "allocated" && status != "assigned" {
            continue;
        }

        // 国家过滤
        if !countries.is_empty() && !countries.iter().any(|c| c.eq_ignore_ascii_case(country)) {
            continue;
        }

        match ip_type {
            "ipv4" => {
                if let (Some(start), Ok(count)) = (parse_ipv4(start_str), count_str.parse::<u32>()) {
                    let start_val = ipv4_to_u128(start);
                    let end_val = start_val.saturating_add(count as u128 - 1);
                    ipv4.push(IpRange { start: start_val, end: end_val });
                }
            }
            "ipv6" => {
                if let Some(prefix) = parse_ipv6_prefix(start_str) {
                    if let Ok(prefix_len) = count_str.parse::<u32>() {
                        if prefix_len <= 128 {
                            let start_val = ipv6_to_u128(prefix);
                            let host_bits = 128 - prefix_len;
                            let mask = if host_bits >= 128 { u128::MAX } else { (1u128 << host_bits) - 1 };
                            let end_val = start_val | mask;
                            ipv6.push(IpRange { start: start_val, end: end_val });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // 按起始地址排序，便于二分查找
    ipv4.sort_by_key(|r| r.start);
    ipv6.sort_by_key(|r| r.start);

    if ipv4.is_empty() && ipv6.is_empty() {
        return Ok(None);
    }

    log(LogLevel::Info, &format!(
        "[APNIC] 解析完成: IPv4={} 段, IPv6={} 段, 日期={}",
        ipv4.len(), ipv6.len(), file_date
    ));

    Ok(Some((ipv4, ipv6, file_date)))
}

/// 二分查找 IP 是否在某个范围列表中
fn is_in_ranges(ranges: &[IpRange], val: u128) -> bool {
    if ranges.is_empty() {
        return false;
    }

    // 二分查找最后一个 start <= val 的范围
    let idx = match ranges.binary_search_by_key(&val, |r| r.start) {
        Ok(i) => i,
        Err(0) => return false, // val 比所有 start 都小
        Err(i) => i - 1,       // 最后一个 start < val 的位置
    };

    idx < ranges.len() && val <= ranges[idx].end
}

/// 将 IPv4 地址转为 u128（低 32 位）
fn ipv4_to_u128(ip: Ipv4Addr) -> u128 {
    u128::from(u32::from(ip))
}

/// 将 IPv6 地址转为 u128
fn ipv6_to_u128(ip: Ipv6Addr) -> u128 {
    u128::from(ip)
}

/// 解析 IPv4 地址字符串
fn parse_ipv4(s: &str) -> Option<Ipv4Addr> {
    Ipv4Addr::from_str(s).ok()
}

/// 解析 IPv6 前缀（网络地址）
fn parse_ipv6_prefix(s: &str) -> Option<Ipv6Addr> {
    Ipv6Addr::from_str(s).ok()
}

/// 计算到次日午夜（北京时间 24:00）的秒数
fn secs_until_midnight() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // UTC+8 (北京时间)
    let beijing = now + 8 * 3600;
    let secs_today = beijing % 86400;
    let remaining = 86400 - secs_today;
    remaining.max(60) // 至少 60 秒
}

/// 格式化持续时间
fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    if hours > 0 {
        format!("{}时{}分{}秒", hours, mins, secs)
    } else if mins > 0 {
        format!("{}分{}秒", mins, secs)
    } else {
        format!("{}秒", secs)
    }
}
