//! 邮箱验证码模块 — SMTP 发送 + 校验码存储

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

use crate::config::SmtpConfig;
use mc_link_common::log::{log, LogLevel};

/// 验证码有效期（5 分钟）
const CODE_TTL: Duration = Duration::from_secs(300);

pub struct EmailVerifyState {
    codes: Mutex<HashMap<String, CodeEntry>>,
}

struct CodeEntry {
    code: String,
    created: Instant,
}

impl EmailVerifyState {
    pub fn new() -> Self {
        Self {
            codes: Mutex::new(HashMap::new()),
        }
    }

    /// 发送验证码到指定邮箱
    /// 如果 SMTP 未配置则返回错误
    pub fn send_code(&self, email: &str, smtp: &SmtpConfig) -> Result<String, String> {
        let code = generate_code();

        // 先存起来（后续 verify 时校验）
        {
            let mut map = self.codes.lock().map_err(|e| e.to_string())?;
            map.insert(email.to_string(), CodeEntry {
                code: code.clone(),
                created: Instant::now(),
            });
        }

        // 发送邮件
        if let Err(e) = send_smtp_email(email, &code, smtp) {
            log(LogLevel::Warn, &format!("SMTP 发送失败 ({}): {}", email, e));
            // 发送失败则移除验证码
            let mut map = self.codes.lock().map_err(|e| e.to_string())?;
            map.remove(email);
            return Err(format!("邮件发送失败: {}", e));
        }

        log(LogLevel::Info, &format!("验证码已发送至 {}", email));
        Ok(code) // 返回 code 方便测试，生产可去掉
    }

    /// 验证邮箱验证码
    pub fn verify_code(&self, email: &str, code: &str) -> bool {
        let mut map = match self.codes.lock() {
            Ok(m) => m,
            Err(_) => return false,
        };

        // 清理过期
        map.retain(|_, entry| entry.created.elapsed() < CODE_TTL);

        match map.remove(email) {
            Some(entry) if entry.code == code && entry.created.elapsed() < CODE_TTL => true,
            _ => false,
        }
    }

    /// 清理过期验证码
    pub fn cleanup(&self) {
        if let Ok(mut map) = self.codes.lock() {
            let before = map.len();
            map.retain(|_, entry| entry.created.elapsed() < CODE_TTL);
            let after = map.len();
            if before != after {
                log(LogLevel::Info, &format!("清理了 {} 个过期验证码", before - after));
            }
        }
    }
}

fn generate_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1000000))
}

fn send_smtp_email(to: &str, code: &str, smtp: &SmtpConfig) -> Result<(), Box<dyn std::error::Error>> {
    let email = Message::builder()
        .from(smtp.from_address.parse()?)
        .to(to.parse()?)
        .subject("MC Link — 邮箱验证码")
        .header(ContentType::TEXT_HTML)
        .body(format!(
            r#"<!DOCTYPE html>
<html><body style="font-family:sans-serif;padding:24px;background:#f5f5f5;">
<div style="max-width:480px;margin:0 auto;background:#fff;border-radius:12px;padding:32px;">
<h2 style="margin:0 0 16px;color:#333;">MC Link 验证码</h2>
<p style="color:#666;font-size:14px;">您正在使用邮箱登录 MC Link，验证码为：</p>
<div style="text-align:center;margin:20px 0;">
<span style="font-size:36px;letter-spacing:6px;font-weight:700;color:#4c9aff;">{}</span>
</div>
<p style="color:#999;font-size:12px;">验证码 5 分钟内有效，请勿泄露给他人。</p>
<p style="color:#999;font-size:12px;">如果您没有请求此验证码，请忽略此邮件。</p>
</div></body></html>"#,
            code
        ))?;

    let creds = Credentials::new(smtp.username.clone(), smtp.password.clone());

    let transport = SmtpTransport::starttls_relay(&smtp.host)?
        .port(smtp.port)
        .credentials(creds)
        .build();

    transport.send(&email)?;
    Ok(())
}