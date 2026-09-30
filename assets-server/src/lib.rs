//! MC Link 资产服务器
//!
//! 提供静态文件服务：
//! - Bootstrap Icons 字体/CSS
//! - Poppins 字体/CSS
//! - 缓存页面清单与 HTML 片段
//! - 更新包清单与安装包
//! - 设置项元配置（驱动前端设置页渲染）
//!
//! 路由约定（除 Pages/SettingMeta 外，其余路径均位于 Assets/ 下，
//! 与 `POST /upload?path=<相对路径>` 的写入位置一致）：
//!   /bootstrap-icons/*    → Assets/bootstrap-icons/
//!   /fonts/*              → Assets/fonts/
//!   /icons/*              → Assets/icons/
//!   /adapter/manifest.json → Assets/adapter/manifest.json
//!   /adapter/<file>       → Assets/adapter/<file>
//!   /update/latest.json   → Assets/update/latest.json（回退：Updates/）
//!   /update/<file>        → Assets/update/<file>（回退：Updates/）
//!   /pages/manifest.json  → Pages/manifest.json
//!   /pages/<name>         → Pages/<name>.html
//!   /settings/manifest.json → SettingMeta/manifest.json
//!   /settings/meta/<s>    → SettingMeta/<s>.yml
//!   /settings/meta        → 列出所有可用 section
//!   /health               → {"status":"ok"}

mod config;
mod server;
mod meta;
mod prepare;

pub use config::AssetsConfig;
pub use server::run_server;
pub use prepare::prepare_from_npm;
