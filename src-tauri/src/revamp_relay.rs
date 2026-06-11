use std::io::{Read, Write};
use std::net::{TcpStream, Shutdown};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub struct RevampHostRelay {
    running: Arc<Mutex<bool>>,
}

impl RevampHostRelay {
    pub fn new() -> Self {
        Self {
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// 启动房主 TCP 中继
    /// 1. 连接本地 MC 127.0.0.1:{local_port}
    /// 2. 连接节点 {node_ip}:{node_port}
    /// 3. 双向转发
    pub fn start(
        &mut self,
        local_port: u16,
        node_ip: &str,
        node_port: &str,
        stop_signal: Arc<AtomicBool>,
    ) -> Result<String, String> {
        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = true;

        let addr = format!("{}:{}", node_ip, node_port);

        // 连接本地 MC
        let local = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", local_port).parse().unwrap(),
            Duration::from_secs(5),
        )
        .map_err(|e| format!("连接本地 MC 失败 (127.0.0.1:{}): {}", local_port, e))?;
        local.set_read_timeout(Some(Duration::from_secs(30))).ok();
        local.set_write_timeout(Some(Duration::from_secs(30))).ok();

        // 连接节点
        let node = TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_secs(10),
        )
        .map_err(|e| format!("连接节点失败 ({}): {}", addr, e))?;
        node.set_read_timeout(Some(Duration::from_secs(30))).ok();
        node.set_write_timeout(Some(Duration::from_secs(30))).ok();

        let stop = stop_signal.clone();
        let local_arc = Arc::new(Mutex::new(local));
        let node_arc = Arc::new(Mutex::new(node));

        // 本地 → 节点
        let s = stop.clone();
        let l = local_arc.clone();
        let n = node_arc.clone();
        thread::spawn(move || {
            forward_thread(l, n, s, "本地→节点");
        });

        // 节点 → 本地
        let s = stop.clone();
        let l = local_arc.clone();
        let n = node_arc.clone();
        thread::spawn(move || {
            forward_thread(n, l, s, "节点→本地");
        });

        Ok(format!("revamp 中继已启动: 127.0.0.1:{} ↔ {}", local_port, addr))
    }
}

fn forward_thread(src: Arc<Mutex<TcpStream>>, dst: Arc<Mutex<TcpStream>>, stop: Arc<AtomicBool>, _label: &'static str) {
    let mut buf = vec![0u8; 65536];
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let n = {
            let mut src_guard = match src.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            match src_guard.read(&mut buf) {
                Ok(0) => break,  // 连接关闭
                Ok(n) => n,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut
                    {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    break;
                }
            }
        };

        if stop.load(Ordering::Relaxed) {
            break;
        }

        let mut dst_guard = match dst.lock() {
            Ok(g) => g,
            Err(_) => break,
        };
        if dst_guard.write_all(&buf[..n]).is_err() {
            break;
        }
        if dst_guard.flush().is_err() {
            break;
        }
    }

    // 关闭连接
    if let Ok(src_guard) = src.lock() {
        src_guard.shutdown(Shutdown::Both).ok();
    }
    if let Ok(dst_guard) = dst.lock() {
        dst_guard.shutdown(Shutdown::Both).ok();
    }
}