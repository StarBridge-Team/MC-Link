//! P2P 连接管理器
//!
//! 托管 wgp-core 的后台 P2P 连接任务，保存停止标志与运行状态，
//! 供 `start/stop/status` 命令访问；预留 `p2p_socket` 以便后续端口转发扩展。

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::Instant;

use serde::Serialize;

use wgp_core::connect::ConnectResult;
use wgp_core::p2p::socket::UdpSocketManager;

/// 连接状态机
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStage {
    /// 空闲，未连接
    Idle,
    /// 连接中
    Connecting,
    /// 已建立 P2P 通道
    Connected,
    /// 连接失败 / 已断开
    Disconnected,
}

/// 当前连接状态快照
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStatus {
    pub stage: ConnectionStage,
    pub mode: Option<String>,
    pub code: Option<String>,
    pub started_at_ms: Option<u64>,
    pub peer_addr: Option<String>,
    pub local_nat: Option<String>,
    pub peer_nat: Option<String>,
    pub success_layer: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub last_error: Option<String>,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self {
            stage: ConnectionStage::Idle,
            mode: None,
            code: None,
            started_at_ms: None,
            peer_addr: None,
            local_nat: None,
            peer_nat: None,
            success_layer: None,
            elapsed_ms: None,
            last_error: None,
        }
    }
}

/// 已建立连接的句柄（保留 socket 供后续桥接扩展）
#[allow(dead_code)]
pub struct ConnectionHandle {
    pub p2p_socket: UdpSocketManager,
    pub peer_addr: SocketAddr,
}

/// 全局连接管理器
pub struct ConnectionManager {
    stop_flag: Arc<AtomicBool>,
    status: SyncMutex<ConnectionStatus>,
    handle: SyncMutex<Option<ConnectionHandle>>,
    /// 防止重复发起连接
    running: AtomicBool,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            status: SyncMutex::new(ConnectionStatus::default()),
            handle: SyncMutex::new(None),
            running: AtomicBool::new(false),
        }
    }

    /// 尝试占用连接槽位，返回停止标志与启动时刻
    pub fn begin(&self, mode: &str, code: &str) -> Result<(Arc<AtomicBool>, Instant), String> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err("已有连接任务正在运行".into());
        }
        // 清理上一轮状态
        self.stop_flag.store(false, Ordering::Release);
        *self.handle.lock().unwrap() = None;
        let now = Instant::now();
        {
            let mut s = self.status.lock().unwrap();
            *s = ConnectionStatus {
                stage: ConnectionStage::Connecting,
                mode: Some(mode.to_string()),
                code: Some(code.to_string()),
                started_at_ms: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                ),
                peer_addr: None,
                local_nat: None,
                peer_nat: None,
                success_layer: None,
                elapsed_ms: None,
                last_error: None,
            };
        }
        Ok((self.stop_flag.clone(), now))
    }

    #[allow(dead_code)]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }


    /// 标记连接成功并保存句柄
    pub fn set_connected(&self, conn: ConnectResult, elapsed_ms: u64) {
        let peer_addr = conn.peer_addr.to_string();
        let local_nat = conn.local_nat.name().to_string();
        let peer_nat = conn.peer_nat.name().to_string();
        let success_layer = conn.success_layer.name().to_string();
        {
            let mut s = self.status.lock().unwrap();
            s.stage = ConnectionStage::Connected;
            s.peer_addr = Some(peer_addr.clone());
            s.local_nat = Some(local_nat.clone());
            s.peer_nat = Some(peer_nat.clone());
            s.success_layer = Some(success_layer.clone());
            s.elapsed_ms = Some(elapsed_ms);
            s.last_error = None;
        }
        *self.handle.lock().unwrap() = Some(ConnectionHandle {
            p2p_socket: conn.p2p_socket,
            peer_addr: conn.peer_addr,
        });
        self.running.store(true, Ordering::SeqCst);
    }

    /// 标记连接失败
    pub fn set_failed(&self, err: &str, elapsed_ms: Option<u64>) {
        {
            let mut s = self.status.lock().unwrap();
            s.stage = ConnectionStage::Disconnected;
            s.last_error = Some(err.to_string());
            if elapsed_ms.is_some() {
                s.elapsed_ms = elapsed_ms;
            }
        }
        *self.handle.lock().unwrap() = None;
        self.running.store(false, Ordering::SeqCst);
    }

    /// 请求停止当前连接任务
    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::Release);
        let mut s = self.status.lock().unwrap();
        if s.stage == ConnectionStage::Connecting || s.stage == ConnectionStage::Connected {
            s.stage = ConnectionStage::Disconnected;
        }
        self.running.store(false, Ordering::SeqCst);
        *self.handle.lock().unwrap() = None;
    }

    /// 任务自然结束（成功后保持连接态，失败置为断开）
    pub fn task_ended(&self, connected: bool) {
        if !connected {
            let mut s = self.status.lock().unwrap();
            if s.stage == ConnectionStage::Connecting {
                s.stage = ConnectionStage::Disconnected;
            }
            self.running.store(false, Ordering::SeqCst);
        }
    }

    pub fn status_snapshot(&self) -> ConnectionStatus {
        let mut s = self.status.lock().unwrap().clone();
        if s.stage == ConnectionStage::Connecting || s.stage == ConnectionStage::Connected {
            if let Some(start) = s.started_at_ms {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(start);
                s.elapsed_ms = Some(now.saturating_sub(start));
            }
        }
        s
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
