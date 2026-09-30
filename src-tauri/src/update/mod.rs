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
//! # 前置闸门：构建渠道
//!
//! 能不能自动更新，先看这份二进制是什么来路（[`crate::build_channel`]）：
//!
//! | 渠道 | 能做什么 |
//! |---|---|
//! | `official`（发布流程产出） | 可自动更新 |
//! | `dev`（debug 构建） | 不检查、不更新；调试时可用 `MC_LINK_UPDATE_OVERRIDE=1` 强制开启 |
//! | `self-built`（自己编译的 release） | 只提示"官方有新版本"+ 手动下载链接，绝不替换文件 |
//!
//! 闸门在 `fetch`（决定是否发起检查、是否给可安装资产）与 `install`（落地前二次确认）两处，
//! 后者是防止绕过界面直接调用命令。
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
