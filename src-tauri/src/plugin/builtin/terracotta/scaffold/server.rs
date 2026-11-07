//! 房主侧的 Scaffolding 服务端。
//!
//! 每个连接一条线程、循环处理"读一帧 → 交给人处理 → 写一帧"。陶瓦的协议本来就是
//! 请求-响应式，不需要并发调度：连接数等于房间人数（个位数）。

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{read_frame, write_response, Frame, PacketResponse, TIMEOUT};

/// 一帧请求的处理函数。返回 `Err` 会被翻译成状态码 255 的错误响应。
pub type Handler = Arc<dyn Fn(&Frame) -> io::Result<PacketResponse> + Send + Sync>;

/// 运行中的服务端。`Drop` 即停止接受新连接。
pub struct ScaffoldServer {
    port: u16,
    stop: Arc<AtomicBool>,
}

impl ScaffoldServer {
    /// 在 `port` 上启动（`0` 表示由系统分配），返回实际端口。
    pub fn start(port: u16, handler: Handler) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();

        std::thread::Builder::new()
            .name("terracotta-scaffold".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    if flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(mut stream) = stream else {
                        // 单次 accept 失败（例如对端在建连后立刻断开）不该终止整个服务
                        continue;
                    };
                    let handler = handler.clone();
                    let spawned = std::thread::Builder::new()
                        .name("terracotta-conn".to_string())
                        .spawn(move || {
                            if let Err(e) = serve(&mut stream, handler) {
                                // 对端断开是正常现象（关窗口、切网络），只留痕
                                println!("[陶瓦] 连接结束: {}", e);
                            }
                        });
                    if let Err(e) = spawned {
                        eprintln!("[陶瓦] 无法处理新连接: {}", e);
                    }
                }
            })?;

        Ok(Self { port, stop })
    }

    /// 实际监听端口（`port` 传 0 时才知道真实值）。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 停止接受新连接。
    ///
    /// accept 循环阻塞在 `incoming()` 上，光置标志位不会让它醒来；这里主动连一次
    /// 自己把它顶出来（比引入非阻塞 accept + 轮询简单得多）。
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)));
    }
}

impl Drop for ScaffoldServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 处理一个连接，直到对端断开或出错。
fn serve(stream: &mut TcpStream, handler: Handler) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.set_nodelay(true)?;

    loop {
        let frame = read_frame(stream)?;
        let response = match handler(&frame) {
            Ok(response) => response,
            Err(e) => PacketResponse::Fail {
                status: super::STATUS_UNIMPLEMENTED,
                data: format!("{:?}", e).into_bytes(),
            },
        };
        write_response(stream, &response)?;
    }
}

/// 取一个当前空闲的回环端口。
pub fn free_port() -> u16 {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(0)
}

/// 建连超时（供调用方估算重试次数）。
pub fn connect_timeout() -> Duration {
    Duration::from_secs(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::builtin::terracotta::scaffold::client::ClientSession;

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    #[test]
    fn serves_requests_and_stops() {
        let handler: Handler = Arc::new(|frame: &Frame| PacketResponse::ok(frame.body.clone()));
        let server = ScaffoldServer::start(0, handler).unwrap();
        assert!(server.port() > 0);

        let mut session = ClientSession::open(loopback(server.port())).unwrap();
        let response = session
            .call("c", "ping", |body| body.extend_from_slice(b"abc"))
            .expect("应收到响应");
        assert_eq!(response.data(), b"abc");

        server.stop();
    }

    #[test]
    fn handler_error_becomes_error_status() {
        let handler: Handler = Arc::new(|_: &Frame| {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "boom"))
        });
        let server = ScaffoldServer::start(0, handler).unwrap();

        // 直接走线格式：`ClientSession::call` 按设计把"对端返回失败"折叠成 None，
        // 那样就断言不到状态码本身了
        let mut stream = std::net::TcpStream::connect(loopback(server.port())).unwrap();
        let frame = Frame {
            namespace: "c".to_string(),
            path: "anything".to_string(),
            body: Vec::new(),
        };
        crate::plugin::builtin::terracotta::scaffold::write_frame(&mut stream, &frame).unwrap();
        let response = crate::plugin::builtin::terracotta::scaffold::read_response(&mut stream)
            .expect("应收到响应");
        assert_eq!(response.status(), super::super::STATUS_UNIMPLEMENTED);
        server.stop();
    }
}
