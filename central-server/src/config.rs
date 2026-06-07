//! 配置管理模块

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use mc_link_common::log::{log, LogLevel};

const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0";
const DEFAULT_LISTEN_PORT: u16 = 8878;
const DEFAULT_EXTERNAL_PORT: u16 = 8878;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default = "default_external_port")]
    pub external_port: u16,
}

fn default_listen_addr() -> String { DEFAULT_LISTEN_ADDR.to_string() }
fn default_listen_port() -> u16 { DEFAULT_LISTEN_PORT }
fn default_external_port() -> u16 { DEFAULT_EXTERNAL_PORT }

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addr: DEFAULT_LISTEN_ADDR.to_string(),
            listen_port: DEFAULT_LISTEN_PORT,
            external_port: DEFAULT_EXTERNAL_PORT,
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