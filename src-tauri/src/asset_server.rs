//! 资产服务器地址解析
//!
//! 客户端所有外部资源（字体、图标、页面清单、更新包、设置元配置）都通过
//! 此模块解析统一地址。
//!
//! - 开发环境（debug）：固定指向本地 assets-server `http://localhost:54789`
//!   由 `pnpm tauri dev` 通过并行脚本启动。
//! - 生产环境（release）：完全硬编码 `https://mclinkassets.xigo.top:54789`，
//!   不读取任何本地配置覆盖。

/// 资产服务器地址。
///
/// - debug 构建：`http://localhost:54789`
/// - release 构建：`https://mclinkassets.xigo.top:54789`
pub fn assets_server_url(_data_dir: &std::path::Path) -> String {
    if cfg!(debug_assertions) {
        "http://localhost:54789".to_string()
    } else {
        "https://mclinkassets.xigo.top:54789".to_string()
    }
}

/// 拼接资产服务器 URL：`{base}/{path}`，自动处理末尾斜杠
pub fn join(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{}/{}", base, path)
}
