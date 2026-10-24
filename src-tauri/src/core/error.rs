//! 统一错误类型。
//!
//! 命令层此前到处是 `Result<T, String>`：前端拿到的只有一个字符串，
//! 既没法按类型分支（"房间不存在"与"网络失败"要不同处理），也没法做 i18n。
//!
//! 这里给出 `{ code, message }` 两段式：`code` 是稳定的机器可读标识，
//! `message` 是给人看的兜底文案（前端可优先按 `code` 取本地化文案）。

use serde::Serialize;

/// 前端契约：`{ code, message }`，camelCase 与原样保持一致。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    /// 机器可读标识，kebab-case（如 `room-not-found`）。
    pub code: String,
    /// 兜底文案；前端可按 `code` 覆盖。
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// 参数不合法（前端传了越界端口、空房间码之类）。
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid-argument", message)
    }

    /// 目标不存在（会话已被清理、插件未注册）。
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not-found", message)
    }

    /// 内部失败（锁中毒、IO、意外分支）。
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::internal(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        Self::internal(value)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::internal(format!("序列化失败: {}", value))
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_camel_case_object() {
        let err = AppError::not_found("房间不存在");
        let value = serde_json::to_value(&err).expect("可序列化");
        assert_eq!(value["code"], "not-found");
        assert_eq!(value["message"], "房间不存在");
    }

    #[test]
    fn string_conversion_is_internal() {
        assert_eq!(AppError::from("boom").code, "internal");
    }
}
