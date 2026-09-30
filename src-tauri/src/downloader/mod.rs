//! 下载能力。
//!
//! - [`verified`]：**当前唯一在用的下载链路**——流式、多镜像、体积上限、
//!   分片超时、SHA256 强制校验。适配器安装包与应用更新包都走它。
//! - [`verify`]：SHA256 计算与校验，被 `verified` 调用。
//!
//! # 关于 `chunk` / `downloader` / `policy`
//!
//! 这三个模块是一个**分片 + 断点续传**下载器（`Downloader`）。它原本只有一个调用方
//! （旧版更新包下载），该调用方已改用 `verified`，因此它现在是**零引用**代码。
//!
//! 之所以暂时保留而不是直接删：它的续传能力对"大体积、网络抖动"场景仍有价值
//! （`verified` 不续传）。但按项目的整洁规则，无调用方的代码不应长期留存，
//! 需要在两者中选一个：
//!
//! 1. 删除 `chunk` / `downloader` / `policy`（约 400 行，git 历史中可随时取回）；
//! 2. 或者把续传能力并入 `verified`，让它成为唯一实现，再删除这三个模块。
//!
//! 在做出决定前，用 `allow(dead_code)` 抑制告警，以免掩盖其他真正的死代码告警。

#[allow(dead_code)]
pub mod chunk;
#[allow(dead_code)]
pub mod downloader;
#[allow(dead_code)]
pub mod policy;
pub mod verified;
pub mod verify;
