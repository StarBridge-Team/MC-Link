# MC Link

我的世界联机工具 — 无需公网 IP、无需端口映射、无需内网穿透即可远程联机。

> **架构状态**
> 1. 联机核心已从"自建中继转发"迁移为基于**折跃门协议（WGP）**的 P2P 直连。
>    旧中继实现（`mc-link-relay/`、`mc-link-chat/`）已停止维护并移出主分支，
>    源码保留在 `archive/legacy-relay` 分支与 `legacy-relay-stack-v0.4.0` tag。
> 2. 认证实现已整体移除（客户端登录 / Token 校验 / 桌面登录轮询，以及 `central-server/`）。
>    现阶段客户端不含登录功能；后续鉴权将由 **OpenXigoID** 与 **Prism Auth** 承担。
>    相关源码保留在 `legacy-auth-central-v0.4.0` tag。

## 架构

```
房主 Minecraft <──> MC Link 客户端(房主) ──┐
                                          ├── 折跃门协议 P2P 直连 (wgp-core)
成员 Minecraft <──> MC Link 客户端(成员) ──┘
```

- **客户端** (`src-tauri` + `src`)：Tauri 2 桌面应用，包含房主模式与成员模式
- **P2P 核心**：`wgp-core`（折跃门协议），位于独立仓库 `mc-link-core`

> 依赖位置：`wgp-core` 以跨目录 path 依赖引入，本机开发需把 `mc-link-core` 放在本仓库上一级的 `Rust/` 目录下。

## 快速开始

### 下载客户端

从 [Releases](https://github.com/DogerMMC/mc-link/releases) 下载最新版本 `mc-link-v0.x.x.exe`，直接运行即可。

### 使用

1. **房主**：启动 MC Link → 点击"创建房间"→ 选房间 → 点"开始联机"
2. **成员**：启动 MC Link → 输入房间名和密码 → 点"开始联机"→ 打开 Minecraft 连接显示的地址

## 技术栈

- **客户端**: Tauri 2 + Vue 3 + TypeScript + Rust
- **P2P 核心**: 折跃门协议（`wgp-core`，独立仓库 `mc-link-core`）

## 版本历史

| 版本 | 说明 |
|------|------|
| 0.2.8 | 自动寻找可用本地端口 |
| 0.2.7 | MC_READY 通知机制，等成员MC连上后再连房主 |
| 0.2.6 | 移除 WebRTC，托盘菜单适配深浅色模式 |
| 0.2.5 | 修复中继连接非阻塞模式导致断开 |
| 0.2.4 | 修复热键冲突导致崩溃 |
| 0.2.3 | 初始中继架构 |