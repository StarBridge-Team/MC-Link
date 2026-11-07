//! 访客侧的 Scaffolding 客户端。
//!
//! 一条连接、一个后台线程、一个请求队列：协议是严格的一问一答，因此不需要多路复用，
//! 只需要把"发送"交给线程、"等待"留给调用方。
//!
//! # 为什么用线程而不是直接同步收发
//!
//! 房主可能随时消失。同步收发会让调用方在 `read` 上等到 64 秒超时；而这条连接同时
//! 还承担心跳（`c:player_ping`），必须能在读阻塞时被判定为"已死"。这里让线程持有
//! socket，连接一断就把 `closed` 置位，调用方立刻知道要收尾。

use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

use super::{read_response, write_frame, Frame, PacketResponse, TIMEOUT};

/// 调用方等待单条响应的轮询间隔。
const WAIT_TICK: Duration = Duration::from_millis(100);

/// 一次待发送的请求。
struct Pending {
    frame: Frame,
    reply: Box<dyn FnOnce(Option<PacketResponse>) + Send + 'static>,
}

/// 与房主之间的会话。
pub struct ClientSession {
    sender: mpsc::Sender<Pending>,
    /// 一旦被置位表示连接已断（远端关闭、超时、写失败都算）。
    closed: Arc<OnceLock<()>>,
}

impl ClientSession {
    /// 建立连接。
    pub fn open(address: SocketAddr) -> io::Result<Self> {
        let mut socket = TcpStream::connect_timeout(&address, TIMEOUT)?;
        socket.set_read_timeout(Some(TIMEOUT))?;
        socket.set_write_timeout(Some(TIMEOUT))?;
        socket.set_nodelay(true)?;

        let (sender, receiver) = mpsc::channel::<Pending>();
        let closed = Arc::new(OnceLock::new());
        let closed_worker = closed.clone();

        std::thread::Builder::new()
            .name("terracotta-client".to_string())
            .spawn(move || {
                while let Ok(pending) = receiver.recv() {
                    match exchange(&mut socket, &pending.frame) {
                        Ok(response) => (pending.reply)(Some(response)),
                        Err(e) => {
                            println!("[陶瓦] 会话已断开: {}", e);
                            let _ = closed_worker.set(());
                            (pending.reply)(None);
                            return;
                        }
                    }
                }
            })?;

        Ok(Self { sender, closed })
    }

    /// 连接是否仍然可用。
    pub fn is_alive(&self) -> bool {
        self.closed.get().is_none()
    }

    /// 同步调用一个方法；连接已断或对端返回失败时返回 `None`。
    ///
    /// `encode` 负责往正文里写参数。
    pub fn call(
        &mut self,
        namespace: &str,
        path: &str,
        encode: impl FnOnce(&mut Vec<u8>),
    ) -> Option<PacketResponse> {
        let (tx, rx) = mpsc::channel();
        let frame = {
            let mut body = Vec::new();
            encode(&mut body);
            Frame {
                namespace: namespace.to_string(),
                path: path.to_string(),
                body,
            }
        };

        if self
            .sender
            .send(Pending {
                frame,
                reply: Box::new(move |response| {
                    let _ = tx.send(response);
                }),
            })
            .is_err()
        {
            return None;
        }

        loop {
            match rx.recv_timeout(WAIT_TICK) {
                Ok(Some(PacketResponse::Ok { data })) => return Some(PacketResponse::Ok { data }),
                Ok(Some(PacketResponse::Fail { status, data })) => {
                    println!(
                        "[陶瓦] {}:{} 返回失败（{}）: {}",
                        namespace,
                        path,
                        status,
                        String::from_utf8_lossy(&data)
                    );
                    return None;
                }
                Ok(None) => return None,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !self.is_alive() {
                        return None;
                    }
                }
                // 回复通道被丢弃只可能是工作线程已退出
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    /// 便捷封装：调用并返回正文。
    pub fn call_body(
        &mut self,
        namespace: &str,
        path: &str,
        encode: impl FnOnce(&mut Vec<u8>),
    ) -> Option<Vec<u8>> {
        self.call(namespace, path, encode)
            .map(|response| match response {
                PacketResponse::Ok { data } | PacketResponse::Fail { data, .. } => data,
            })
    }
}

/// 一次完整的收发。
fn exchange(socket: &mut TcpStream, frame: &Frame) -> io::Result<PacketResponse> {
    write_frame(socket, frame)?;
    // 显式 flush：`write_frame` 已经 flush，但这里保留一步避免将来改动踩坑
    socket.flush()?;
    read_response(socket)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener};

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    #[test]
    fn open_fails_fast_on_a_released_port() {
        // "刚释放的端口连不上"是概率性的：极端情况下会有别的进程恰好抢到它。
        // 多试几次、只要命中一次失败即可——这样断言既稳定，又真的在测错误路径。
        let mut saw_failure = false;
        for _ in 0..20 {
            let port = {
                let listener = TcpListener::bind(loopback(0)).unwrap();
                listener.local_addr().unwrap().port()
            };
            if ClientSession::open(loopback(port)).is_err() {
                saw_failure = true;
                break;
            }
        }
        assert!(saw_failure, "连一个没人监听的端口必须报错，而不是挂住");
    }

    #[test]
    fn call_returns_none_when_peer_vanishes() {
        let listener = TcpListener::bind(loopback(0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        // 接受连接后立刻关闭，模拟房主消失
        std::thread::spawn(move || {
            let _ = listener.accept();
        });

        let mut session = ClientSession::open(loopback(port)).unwrap();
        let response = session.call("c", "ping", |_| {});
        assert!(response.is_none(), "对端断开后不应返回响应");
    }
}
