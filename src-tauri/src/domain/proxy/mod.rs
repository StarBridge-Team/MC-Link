//! GitHub 代理池。
//!
//! 两条来源：**内置 3 个默认源**（离线可用）+ **资源服务器下发的列表**
//! （按天缓存，拿不到就沿用上一次或内置）。
//!
//! 职责边界：本模块只回答"有哪些候选、按什么顺序试"，不负责下载。
//! 下载仍走 `downloader::verified`，它按候选顺序回退并保留续传进度。

pub mod pool;
pub mod probe;

pub use probe::ordered_channels;
