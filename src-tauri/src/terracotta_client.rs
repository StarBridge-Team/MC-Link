use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Meta {
    pub version: String,
    pub yggdrasil_port: u16,
}

#[derive(Deserialize, Serialize)]
pub struct StateResponse {
    pub state: String,
    pub index: u64,
    pub room: Option<String>,
    pub url: Option<String>,
    pub difficulty: Option<String>,
    #[serde(rename = "type")]
    pub exception_type: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum TerracottaState {
    Waiting,
    HostScanning,
    HostStarting,
    HostOk,
    GuestConnecting,
    GuestStarting,
    GuestOk,
    Exception,
    Unknown,
}

impl TerracottaState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, TerracottaState::HostOk | TerracottaState::GuestOk | TerracottaState::Exception)
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, TerracottaState::HostOk | TerracottaState::GuestOk)
    }
}

pub struct TerracottaClient {
    port: u16,
    client: reqwest::Client,
}

impl TerracottaClient {
    pub fn new(port: u16) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build().unwrap();
        TerracottaClient { port, client }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub async fn get_meta(&self) -> Result<Meta, String> {
        let url = format!("{}/meta", self.base_url());
        let resp = self.client.get(&url).send().await.map_err(|e| format!("连接陶瓦联机失败: {}", e))?;
        resp.json().await.map_err(|e| format!("解析元数据失败: {}", e))
    }

    pub async fn get_state(&self) -> Result<StateResponse, String> {
        let url = format!("{}/state", self.base_url());
        let resp = self.client.get(&url).send().await.map_err(|e| format!("获取状态失败: {}", e))?;
        resp.json().await.map_err(|e| format!("解析状态失败: {}", e))
    }

    pub async fn set_ide(&self) -> Result<(), String> {
        let url = format!("{}/state/ide", self.base_url());
        self.client.get(&url).send().await.map_err(|e| format!("重置状态失败: {}", e))?;
        Ok(())
    }

    pub async fn start_host(&self, room: Option<&str>, player: Option<&str>, public_nodes: &[String]) -> Result<(), String> {
        let url = format!("{}/state/scanning", self.base_url());
        let mut params = Vec::new();
        if let Some(r) = room { params.push(("room", r)); }
        if let Some(p) = player { params.push(("player", p)); }
        for node in public_nodes { params.push(("public_nodes", node)); }
        self.client.get(&url).query(&params).send().await
            .map_err(|e| format!("启动主机模式失败: {}", e))?;
        Ok(())
    }

    pub async fn start_guest(&self, room: &str, player: Option<&str>, public_nodes: &[String]) -> Result<(), String> {
        let url = format!("{}/state/guesting", self.base_url());
        let mut params = vec![("room", room)];
        if let Some(p) = player { params.push(("player", p)); }
        for node in public_nodes { params.push(("public_nodes", node)); }
        let resp = self.client.get(&url).query(&params).send().await
            .map_err(|e| format!("加入房间失败: {}", e))?;
        if resp.status().is_success() { Ok(()) }
        else { Err(format!("加入房间失败: {}", resp.status())) }
    }

    pub async fn panic(&self, peaceful: bool) -> Result<(), String> {
        let url = format!("{}/panic?peaceful={}", self.base_url(), peaceful);
        let _ = self.client.get(&url).send().await;
        Ok(())
    }

    pub async fn poll_state(&self, timeout_secs: u64, tick_ms: u64) -> Result<StateResponse, String> {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(timeout_secs) {
                return Err("等待陶瓦联机状态超时".to_string());
            }
            match self.get_state().await {
                Ok(state) => {
                    let ts = parse_state(&state.state);
                    if ts.is_terminal() || ts == TerracottaState::Unknown {
                        return Ok(state);
                    }
                }
                Err(_) => {
                    // 临时网络错误则继续轮询
                }
            }
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    }
}

pub fn parse_state(s: &str) -> TerracottaState {
    match s {
        "waiting" => TerracottaState::Waiting,
        "host-scanning" => TerracottaState::HostScanning,
        "host-starting" => TerracottaState::HostStarting,
        "host-ok" => TerracottaState::HostOk,
        "guest-connecting" => TerracottaState::GuestConnecting,
        "guest-starting" => TerracottaState::GuestStarting,
        "guest-ok" => TerracottaState::GuestOk,
        "exception" => TerracottaState::Exception,
        _ => TerracottaState::Unknown,
    }
}
