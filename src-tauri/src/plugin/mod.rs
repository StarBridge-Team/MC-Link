//! MC Link 插件子系统与多游戏适配架构。
//!
//! # 分层
//!
//! ```text
//! 前端 / 其他 Tauri 命令
//!        │
//!        ▼
//!   PluginManager  ── 调度中枢：房间编排、能力路由、插件生命周期
//!        │
//!        ├── CapabilityRouter ── 按「能力 + 游戏」选出可用插件
//!        │
//!        ├── SessionHandle ───── 外部插件：回环 WebSocket + AES-256-GCM
//!        └── BuiltinProvider ─── 内置插件：进程内直调（陶瓦适配器走这条路）
//! ```
//!
//! # 三类插件能力
//!
//! | 种类 | 职责 | 典型实现 |
//! |---|---|---|
//! | [`manifest::PluginKind::Adapter`] | 打洞/直连，屏蔽 NAT 差异 | 陶瓦联机、ZeroTier、自研打洞 |
//! | [`manifest::PluginKind::Detector`] | 扫描本机游戏实例并回传连接信息 | 进程枚举 + 端口探测 + 局域网嗅探 |
//! | [`manifest::PluginKind::Coupler`] | 让游戏真正接入 MC Link 调度 | 启动器注入、servers.dat 改写、参数托管 |
//!
//! # 安全基线
//!
//! 1. 网关只监听 `127.0.0.1`，且连接必须携带一次性启动令牌；
//! 2. 握手采用 PSK 挑战-应答，双向证明，派生独立的会话密钥；
//! 3. 会话期全部报文经 AES-256-GCM 加密封装，AAD 绑定版本/会话/序号；
//! 4. 权限在核心进程内判定，未签名插件的权限被信任度封顶；
//! 5. 限流、帧大小上限、心跳失联、重放缓释全部在核心侧强制执行。
//!
//! # 关于 `dead_code`
//!
//! 本子系统对外提供的是一份**扩展接口**：协议方法名与事件主题常量、能力负载
//! 类型（[`protocol::GameInfo`] 等）、权限辅助函数、注册表查询方法等，都需要
//! 先于其消费者（检测类 / 耦合类插件、插件管理界面）存在。若为了消除警告而
//! 删除它们，等于把接口规范砍成当前调用点的补集，后续接入方将无从对齐。
//!
//! 因此这里显式、且仅在此处允许未引用项。真正的死代码仍然应当删除——
//! 判据是"它是否属于对外接口的一部分"，而不是"编译器有没有抱怨"。
#![allow(dead_code)]

pub mod auth;
pub mod builtin;
pub mod capability;
pub mod commands;
pub mod connect;
pub mod crypto;
pub mod fs_secure;
pub mod game;
pub mod gateway;
pub mod launcher;
pub mod manager;
pub mod manifest;
pub mod permission;
pub mod protocol;
pub mod registry;
pub mod router;
pub mod session;
pub mod trust;
pub mod utils;

pub use manager::PluginManager;
