use std::net::{SocketAddr, TcpStream};

use mc_link_common::crypto;
use mc_link_common::utils::resolve_address as common_resolve_address;

pub fn read_packet(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    mc_link_common::protocol::read_packet(stream)
}

pub fn write_packet(stream: &mut TcpStream, data: &[u8]) -> std::io::Result<()> {
    mc_link_common::protocol::write_packet(stream, data)
}

pub fn resolve_address(addr_str: &str) -> Option<SocketAddr> {
    common_resolve_address(addr_str)
}

pub fn pack_packet(room: &str, password: &str, data: &[u8]) -> Vec<u8> {
    let encrypted = crypto::encrypt(data, password);
    let mut packet = Vec::with_capacity(2 + room.len() + password.len() + encrypted.len());
    packet.push(room.len() as u8);
    packet.extend_from_slice(room.as_bytes());
    packet.push(password.len() as u8);
    packet.extend_from_slice(password.as_bytes());
    packet.extend_from_slice(&encrypted);
    packet
}

pub fn try_decrypt_response(data: &[u8], password: &str) -> Option<Vec<u8>> {
    if data.len() < 2 {
        return None;
    }
    let room_len = data[0] as usize;
    if data.len() < 1 + room_len + 1 {
        return None;
    }
    let pass_len = data[1 + room_len] as usize;
    if data.len() < 1 + room_len + 1 + pass_len {
        return None;
    }
    let encrypted_start = 1 + room_len + 1 + pass_len;
    let encrypted = &data[encrypted_start..];
    crypto::decrypt(encrypted, password)
}

pub fn relay_test_packet(room: &str, password: &str) -> Vec<u8> {
    let test_data = b"TEST_PACKET";
    let mut packet = vec![0x41];
    let room_bytes = room.as_bytes();
    packet.push(room_bytes.len() as u8);
    packet.extend_from_slice(room_bytes);
    let pass_bytes = password.as_bytes();
    packet.push(pass_bytes.len() as u8);
    packet.extend_from_slice(pass_bytes);
    packet.extend_from_slice(&crypto::encrypt(test_data, password));
    packet
}