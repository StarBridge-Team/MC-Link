//! 配置管理模块

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::apnic::ApnicConfig;
use mc_link_common::log::{log, LogLevel};

const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0";
const DEFAULT_LISTEN_PORT: u16 = 8878;
const DEFAULT_EXTERNAL_PORT: u16 = 8878;
const DEFAULT_WEB_ADMIN_PORT: u16 = 3456;
const DEFAULT_WEB_ADMIN_BIND: &str = "0.0.0.0";

#[derive(Debug, Deserialize, Serialize)]
pub struct OAuthProviderConfig {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SmtpConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub from_address: String,
}

fn default_smtp_port() -> u16 { 587 }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ClientRelayConfig {
    /// 客户端中继最大带宽（bps），默认 5 Mbps
    #[serde(default = "default_client_relay_bandwidth")]
    pub max_bandwidth_bps: u64,
    /// 客户端中继最大连接数
    #[serde(default = "default_client_relay_connections")]
    pub max_connections: u32,
}

fn default_client_relay_bandwidth() -> u64 { 5_000_000 }
fn default_client_relay_connections() -> u32 { 8 }

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default = "default_external_port")]
    pub external_port: u16,
    #[serde(default = "default_web_admin_port")]
    pub web_admin_port: u16,
    #[serde(default = "default_web_admin_bind")]
    pub web_admin_bind: String,
    /// 账号服务 URL（用于 OAuth 登录后注册/登录）
    #[serde(default)]
    pub account_api_url: String,
    /// OAuth 提供商配置
    #[serde(default)]
    pub oauth: HashMap<String, OAuthProviderConfig>,
    /// SMTP 邮箱验证码配置
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
    /// 客户端中继限速配置
    #[serde(default)]
    pub client_relay: ClientRelayConfig,
    /// APNIC 亚太地区 IP 白名单
    #[serde(default)]
    pub apnic: ApnicConfig,
}

fn default_listen_addr() -> String { DEFAULT_LISTEN_ADDR.to_string() }
fn default_listen_port() -> u16 { DEFAULT_LISTEN_PORT }
fn default_external_port() -> u16 { DEFAULT_EXTERNAL_PORT }
fn default_web_admin_port() -> u16 { DEFAULT_WEB_ADMIN_PORT }
fn default_web_admin_bind() -> String { DEFAULT_WEB_ADMIN_BIND.to_string() }

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addr: DEFAULT_LISTEN_ADDR.to_string(),
            listen_port: DEFAULT_LISTEN_PORT,
            external_port: DEFAULT_EXTERNAL_PORT,
            web_admin_port: DEFAULT_WEB_ADMIN_PORT,
            web_admin_bind: DEFAULT_WEB_ADMIN_BIND.to_string(),
            account_api_url: String::new(),
            oauth: HashMap::new(),
            smtp: None,
            client_relay: ClientRelayConfig {
                max_bandwidth_bps: default_client_relay_bandwidth(),
                max_connections: default_client_relay_connections(),
            },
            apnic: ApnicConfig::default(),
        }
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn load_config() -> Config {
    let config_path = exe_dir().join("config.yaml");
    let config_path_str = config_path.to_string_lossy().to_string();
    match fs::read_to_string(&config_path) {
        Ok(content) => match serde_yaml::from_str(&content) {
            Ok(config) => {
                log(LogLevel::Info, &format!("已加载配置文件 {}", config_path_str));
                config
            }
            Err(e) => {
                log(LogLevel::Error, &format!("配置文件解析失败: {}", e));
                Config::default()
            }
        }
        Err(_) => {
            log(LogLevel::Info, "未找到配置文件，正在创建默认配置...");
            let default_config = Config::default();
            if let Ok(yaml) = serde_yaml::to_string(&default_config) {
                if fs::write(&config_path, yaml).is_ok() {
                    log(LogLevel::Info, &format!("已创建默认配置文件 {}", config_path_str));
                }
            }
            default_config
        }
    }
}