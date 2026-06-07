use std::net::TcpStream;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json;
use crate::protocol;

const CENTRAL_SERVER_ADDR: &str = "mk.aini2.cn:8878";

fn load_central_server_addr() -> String {
    let config_paths = ["config.yml", "../config.yml"];
    for path in &config_paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if let Some(value) = line.strip_prefix("central_server:") {
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    if !value.is_empty() {
                        return value.to_string();
                    }
                }
            }
        }
    }
    CENTRAL_SERVER_ADDR.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    pub id: String,
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub name: String,
    pub password_hash: String,
    pub host_relay_id: String,
    pub created_at: u64,
}

fn serialize<T: Serialize>(data: &T) -> Vec<u8> {
    serde_json::to_vec(data).unwrap_or_default()
}

fn send_request(cmd: u8, data: &[u8]) -> Option<Vec<u8>> {
    let addr = load_central_server_addr();
    let central_addr = protocol::resolve_address(&addr)?;
    let mut stream = TcpStream::connect_timeout(&central_addr, Duration::from_secs(5)).ok()?;
    let mut packet = vec![cmd];
    packet.extend_from_slice(data);
    protocol::write_packet(&mut stream, &packet).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    protocol::read_packet(&mut stream).ok()
}

pub fn get_relays() -> Option<Vec<RelayInfo>> {
    let response = send_request(0x12, &[])?;
    if !response.is_empty() && response[0] == 0x13 {
        serde_json::from_slice(&response[1..]).ok()
    } else {
        None
    }
}

pub fn create_room(room_name: &str, password: &str, relay_id: &str) -> Option<RoomInfo> {
    let req = serde_json::json!({"room_name": room_name, "password": password, "relay_id": relay_id});
    let response = send_request(0x20, &serialize(&req))?;
    if response.len() > 1 && response[0] == 0x21 && response[1] == 0x00 {
        serde_json::from_slice(&response[2..]).ok()
    } else {
        None
    }
}

pub fn delete_room(room_name: &str) -> bool {
    let req = serde_json::json!({"room_name": room_name});
    if let Some(response) = send_request(0x24, &serialize(&req)) {
        return response.len() >= 2 && response[0] == 0x25 && response[1] == 0x00;
    }
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub name: String,
    pub role: String,
    pub joined_at: u64,
}

pub fn join_room(room_name: &str, player_name: &str, role: &str, relay_id: Option<&str>) -> bool {
    let mut req = serde_json::json!({"room_name": room_name, "player_name": player_name, "role": role});
    if let Some(rid) = relay_id {
        req["relay_id"] = serde_json::json!(rid);
    }
    if let Some(response) = send_request(0x26, &serialize(&req)) {
        return response.len() >= 2 && response[0] == 0x28 && response[1] == 0x00;
    }
    false
}

pub fn get_players(room_name: &str) -> Option<Vec<PlayerInfo>> {
    let req = serde_json::json!({"room_name": room_name});
    let response = send_request(0x27, &serialize(&req))?;
    if !response.is_empty() && response[0] == 0x29 {
        serde_json::from_slice(&response[1..]).ok()
    } else {
        None
    }
}

pub fn get_room(room_name: &str) -> Option<bool> {
    let req = serde_json::json!({"room_name": room_name});
    let response = send_request(0x22, &serialize(&req))?;
    if response.len() > 1 && response[0] == 0x23 {
        if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&response[1..]) {
            return data["exists"].as_bool();
        }
    }
    None
}