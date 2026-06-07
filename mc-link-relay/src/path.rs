use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::protocol::{read_packet, write_packet};

/// 从中继服务器获取中继列表
pub fn get_relays_from_central(central_addr: &SocketAddr) -> Option<Vec<serde_json::Value>> {
    let mut stream = TcpStream::connect_timeout(central_addr, Duration::from_secs(5)).ok()?;
    write_packet(&mut stream, &[0x12]).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let response = read_packet(&mut stream).ok()?;
    if response.is_empty() || response[0] != 0x13 {
        return None;
    }
    serde_json::from_slice(&response[1..]).ok()
}

/// 探测中继服务器的延迟和丢包
pub fn probe_relay(addr: &SocketAddr) -> Option<(u64, f32)> {
    let mut total_latency: u64 = 0;
    let mut loss_count: u32 = 0;
    let samples: u32 = 3;

    for _ in 0..samples {
        let start = std::time::SystemTime::now();
        match TcpStream::connect_timeout(addr, Duration::from_secs(3)) {
            Ok(mut s) => {
                write_packet(&mut s, &[0x32]).ok()?;
                s.set_read_timeout(Some(Duration::from_secs(2))).ok();
                read_packet(&mut s).ok()?;
                if let Ok(elapsed) = start.elapsed() {
                    total_latency += elapsed.as_millis() as u64;
                }
            }
            Err(_) => {
                loss_count += 1;
            }
        }
    }

    let success_count = samples - loss_count;
    if success_count == 0 {
        return None;
    }
    let avg_latency = total_latency / success_count as u64;
    let packet_loss = loss_count as f32 / samples as f32;
    Some((avg_latency, packet_loss))
}