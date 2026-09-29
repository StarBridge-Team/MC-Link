//! TCP 流双向桥接
//! 从独立项目缝合：E:\Work Files\Code\Rust\illusion\src\client\proxy.rs

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use mc_link_common::log::{log, LogLevel};

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_conn_id() -> u64 {
    NEXT_CONN_ID.fetch_add(1, Ordering::SeqCst)
}

/// 双向桥接两个 TCP 流，直到任一端关闭
pub fn bridge(a: TcpStream, b: TcpStream, label: &str) {
    let _ = a.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = b.set_read_timeout(Some(Duration::from_secs(30)));

    let total_a = Arc::new(AtomicU64::new(0));
    let total_b = Arc::new(AtomicU64::new(0));
    let label_owned = label.to_string();

    let (mut a_r, mut a_w) = (a.try_clone().unwrap(), a.try_clone().unwrap());
    let (mut b_r, mut b_w) = (b.try_clone().unwrap(), b.try_clone().unwrap());

    let t_a = total_a.clone();
    let t_b = total_b.clone();
    let lbl1 = label_owned.clone();

    let h1 = thread::spawn(move || {
        let mut buf = [0u8; 65535];
        loop {
            match a_r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    t_a.fetch_add(n as u64, Ordering::Relaxed);
                    if b_w.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(_) => break,
            }
        }
        log(LogLevel::Info, &format!("[Illusion/桥接/{}] 方向A→B 结束", lbl1));
    });

    let h2 = thread::spawn(move || {
        let mut buf = [0u8; 65535];
        loop {
            match b_r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    t_b.fetch_add(n as u64, Ordering::Relaxed);
                    if a_w.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(_) => break,
            }
        }
        log(LogLevel::Info, &format!("[Illusion/桥接/{}] 方向B→A 结束", label_owned));
    });

    let _ = h1.join();
    let _ = h2.join();

    let total = total_a.load(Ordering::Relaxed) + total_b.load(Ordering::Relaxed);
    log(LogLevel::Info, &format!("[Illusion/桥接/{}] 桥接结束，总流量 {} 字节", label, total));
}
