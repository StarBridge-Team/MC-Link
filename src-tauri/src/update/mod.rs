//! 应用更新：清单 → 下载 → 安装。
//!
//! # 目录划分
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`model`] | 清单模型、版本比较、按安装形态选资产（纯逻辑，有单测） |
//! | [`fetch`] | 从资源服务器拉清单并判断"是否需要更新" |
//! | [`download`] | 下载与缓存（复用 [`crate::downloader::verified`] 的校验链路） |
//! | [`install`] | 落地：便携版替换 exe，安装版交给安装器，完成后重启 |
//! | [`commands`] | 5 个 Tauri 命令 |
//!
//! # 两种安装形态
//!
//! | 形态 | 数据目录 | 自动更新方式 |
//! |---|---|---|
//! | 便携版 | exe 同目录 | 直接替换 exe，随后重启 |
//! | 安装版 | 系统 app_data_dir | 交给 NSIS 安装器，装完重启 |
//!
//! 形态判定在 [`crate::datadir::install_mode`]。清单按 `platform` + `kind` 同时提供两份资产，
//! 客户端只挑与自身形态匹配的那一份，因此**同一份清单同时服务两种发行方式**。
//!
//! # 发布流程（脚本侧）
//!
//! `pnpm build:release` 在 `tauri build` 之后会执行 `scripts/make-update.mjs`：
//! 把安装包与便携版 exe 复制进 `assets-server/Assets/update/`、生成 `latest.json`，
//! 再由 `scripts/sync-assets.mjs` 上传。详见该脚本头部注释。

mod commands;
mod download;
mod fetch;
mod install;
mod model;

// 只导出模块外部真正用到的类型：其余由子模块内部按路径引用，避免污染上层命名空间
pub(crate) use model::{CheckUpdateResult, DownloadUpdateResult, UpdateAsset};

pub(crate) use commands::*;
pub(crate) use download::{clear_update_cache, download_asset};
pub(crate) use fetch::check_update;
pub(crate) use install::cleanup_leftovers;
