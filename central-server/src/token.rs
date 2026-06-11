//! Token 管理模块 — 生成、存储、验证

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use mc_link_common::log::{log, LogLevel};

#[derive(Debug, Serialize, Deserialize)]
struct TokenFile {
    token: String,
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 生成或加载现有 Token
pub fn load_or_generate_token() -> String {
    let token_path = exe_dir().join("token.json");
    let token_path_str = token_path.to_string_lossy().to_string();

    match fs::read_to_string(&token_path) {
        Ok(content) => {
            if let Ok(data) = serde_json::from_str::<TokenFile>(&content) {
                if !data.token.is_empty() && data.token.len() >= 16 {
                    log(LogLevel::Info, "已加载管理面板 Token");
                    return data.token;
                }
            }
            log(LogLevel::Warn, "Token 文件无效，重新生成...");
            generate_and_save(&token_path, &token_path_str)
        }
        Err(_) => {
            log(LogLevel::Info, "未找到 Token 文件，正在生成...");
            generate_and_save(&token_path, &token_path_str)
        }
    }
}

fn generate_and_save(path: &PathBuf, path_str: &str) -> String {
    let token = generate_token();
    let data = TokenFile { token: token.clone() };
    if let Ok(json) = serde_json::to_string_pretty(&data) {
        if fs::write(path, &json).is_ok() {
            log(LogLevel::Info, &format!("Token 已保存到 {}", path_str));
        }
    }
    println!();
    println!("===========================================");
    println!("  管理面板 Token: {}", token);
    println!("  请在登录页面输入此 Token");
    println!("===========================================");
    println!();
    token
}

/// 生成 32 字符的 Token（UUID 无连字符）
fn generate_token() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

/// 验证 Token
pub fn validate(token: &str, expected: &str) -> bool {
    token == expected
}