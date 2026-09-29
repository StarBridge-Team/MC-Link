//! 资产服务器配置

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// 资产服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetsConfig {
    /// 监听地址
    #[serde(default = "default_bind")]
    pub bind: String,

    /// 监听端口
    #[serde(default = "default_port")]
    pub port: u16,

    /// 资产根目录（包含 Assets/ Pages/ Updates/ SettingMeta/ 子目录）
    #[serde(default = "default_root_dir")]
    pub root_dir: PathBuf,

    /// 跨域允许来源（* 表示任意，留空表示同源）
    #[serde(default)]
    pub cors_origin: String,

    /// 是否记录访问日志
    #[serde(default = "default_true")]
    pub access_log: bool,

    /// 上传接口令牌（留空表示关闭鉴权，任何人可上传；也可通过环境变量 ASSET_UPLOAD_TOKEN 设置）
    #[serde(default)]
    pub upload_token: String,
}

fn default_bind() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 8090 }
fn default_root_dir() -> PathBuf { PathBuf::from(".") }
fn default_true() -> bool { true }

impl Default for AssetsConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            root_dir: default_root_dir(),
            cors_origin: "*".to_string(),
            access_log: true,
            upload_token: String::new(),
        }
    }
}

impl AssetsConfig {
    /// 从 YAML 文件加载配置，文件不存在则返回默认值
    pub fn load(path: &std::path::Path) -> Self {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(content) => serde_yaml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("[配置] 解析 {} 失败: {}, 使用默认值", path.display(), e);
                Self::default()
            }),
            Err(_) => Self::default(),
        };
        // 环境变量覆盖上传令牌（优先级高于配置文件）
        if let Ok(env_tok) = std::env::var("ASSET_UPLOAD_TOKEN") {
            if !env_tok.is_empty() {
                cfg.upload_token = env_tok;
            }
        }
        cfg
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.bind, self.port)
    }

    pub fn assets_dir(&self) -> PathBuf { self.root_dir.join("Assets") }
    pub fn pages_dir(&self) -> PathBuf { self.root_dir.join("Pages") }
    pub fn updates_dir(&self) -> PathBuf { self.root_dir.join("Updates") }
    pub fn setting_meta_dir(&self) -> PathBuf { self.root_dir.join("SettingMeta") }
}
