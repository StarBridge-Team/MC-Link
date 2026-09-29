use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct DesktopInitResult {
    pub code: i32,
    pub session: Option<String>,
    pub expires_in: Option<u64>,
    pub error: String,
}

#[derive(Serialize)]
pub(crate) struct DesktopPollResult {
    pub code: i32,
    pub status: String,
    pub encrypted_token: Option<String>,
    pub token: Option<String>,
    pub error: String,
}

#[derive(Serialize)]
pub(crate) struct DesktopDecryptResult {
    pub code: i32,
    pub token: Option<String>,
    pub error: String,
}

#[derive(Serialize)]
pub(crate) struct UserInfoResult {
    pub id: Option<i64>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct MeResult {
    pub code: i32,
    pub user: Option<UserInfoResult>,
    pub error: String,
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) fn account_login(username: String, password_hashed: String) -> Result<crate::account::AccountResponse, String> {
    // 保留旧接口：直接调用中央服务器的 direct-register API
    let url = format!("{}/api/auth/direct-register", "http://localhost:3456");
    let body = serde_json::json!({
        "username": username,
        "password_hashed": password_hashed,
    });

    let resp = crate::account::get_http_client_ref()
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("网络错误: {}", e))?;

    let text = resp.text().map_err(|e| format!("读取响应失败: {}", e))?;
    crate::account::parse_central_response(&text)
}

#[tauri::command]
pub(crate) fn account_verify(token: String) -> Result<crate::account::AccountResponse, String> {
    crate::account::verify(&token)
}

#[tauri::command]
pub(crate) fn account_ping() -> bool {
    crate::account::ping().unwrap_or(false)
}

/// 桌面登录初始化
#[tauri::command]
pub(crate) fn desktop_login_init(app_id: String) -> Result<DesktopInitResult, String> {
    match crate::account::desktop_init(&app_id) {
        Ok(resp) => Ok(DesktopInitResult {
            code: resp.code,
            session: resp.session,
            expires_in: resp.expires_in,
            error: resp.error,
        }),
        Err(e) => Ok(DesktopInitResult {
            code: 1,
            session: None,
            expires_in: None,
            error: e,
        }),
    }
}

/// 桌面登录轮询
#[tauri::command]
pub(crate) fn desktop_login_poll(app_id: String, session: String) -> Result<DesktopPollResult, String> {
    match crate::account::desktop_poll(&app_id, &session) {
        Ok(resp) => {
            if resp.code == 0 && resp.status == "authorized" {
                if let Some(ref enc_token) = resp.encrypted_token {
                    match crate::account::desktop_decrypt_token(&session, enc_token) {
                        Ok(token) => Ok(DesktopPollResult {
                            code: 0,
                            status: "authorized".to_string(),
                            encrypted_token: Some(enc_token.clone()),
                            token: Some(token),
                            error: String::new(),
                        }),
                        Err(e) => Ok(DesktopPollResult {
                            code: 1,
                            status: "authorized".to_string(),
                            encrypted_token: Some(enc_token.clone()),
                            token: None,
                            error: format!("解密失败: {}", e),
                        }),
                    }
                } else {
                    Ok(DesktopPollResult {
                        code: 0,
                        status: "authorized".to_string(),
                        encrypted_token: None,
                        token: None,
                        error: "缺少加密 token".to_string(),
                    })
                }
            } else {
                Ok(DesktopPollResult {
                    code: resp.code,
                    status: resp.status,
                    encrypted_token: None,
                    token: None,
                    error: resp.error,
                })
            }
        }
        Err(e) => Ok(DesktopPollResult {
            code: 1,
            status: "error".to_string(),
            encrypted_token: None,
            token: None,
            error: e,
        }),
    }
}

/// 获取当前用户信息
#[tauri::command]
pub(crate) fn account_get_me(token: String) -> Result<MeResult, String> {
    match crate::account::get_me(&token) {
        Ok(resp) => Ok(MeResult {
            code: resp.code,
            user: resp.user.map(|u| UserInfoResult {
                id: u.id,
                username: u.username,
                email: u.email,
                role: u.role,
                avatar: u.avatar,
            }),
            error: resp.error,
        }),
        Err(e) => Ok(MeResult {
            code: 1,
            user: None,
            error: e,
        }),
    }
}

/// 获取头像（代理下载，避免跨域）
#[tauri::command]
pub(crate) fn account_get_avatar(token: String, avatar_path: String) -> Result<String, String> {
    crate::account::get_avatar(&token, &avatar_path)
}
