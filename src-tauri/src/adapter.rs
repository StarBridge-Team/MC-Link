use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AdapterStatus {
    pub installed: bool,
    pub running: bool,
    pub starting: bool,
    pub port: Option<u16>,
}

pub struct AdapterManager {
    running: Arc<AtomicBool>,
    pub adapter_dir: PathBuf,
    pub terracotta_port: Arc<Mutex<Option<u16>>>,
}

impl AdapterManager {
    pub fn new() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        let adapter_dir = exe_dir.join("Adapter");

        AdapterManager {
            running: Arc::new(AtomicBool::new(false)),
            adapter_dir,
            terracotta_port: Arc::new(Mutex::new(None)),
        }
    }

    fn status_path(&self) -> PathBuf {
        self.adapter_dir.join("Terracotta.json")
    }

    fn read_status(&self) -> AdapterStatus {
        let path = self.status_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(AdapterStatus { installed: false, running: false, starting: false, port: None })
    }

    fn write_status(&self, status: &AdapterStatus) {
        let path = self.status_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(status) {
            let _ = std::fs::write(&path, &json);
        }
    }

    fn is_port_alive(port: u16) -> bool {
        std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(2),
        ).is_ok()
    }

    pub fn get_status(&self) -> AdapterStatus {
        let port = *self.terracotta_port.lock().unwrap();
        if let Some(p) = port {
            if !Self::is_port_alive(p) {
                *self.terracotta_port.lock().unwrap() = None;
            }
        }

        let mut status = self.read_status();
        let port = *self.terracotta_port.lock().unwrap();
        status.running = port.is_some();
        status.port = port;
        self.write_status(&status);
        status
    }

    pub fn launch_all(&self) {
        self.running.store(true, Ordering::SeqCst);
        if self.terracotta_port.lock().unwrap().is_some() {
            return;
        }
        let status = self.read_status();
        if status.installed {
            self.spawn_terracotta_process();
        }
    }

    pub fn start_terracotta(&self) -> Result<String, String> {
        let status = self.get_status();
        if status.running {
            return Err("陶瓦联机已在运行中".to_string());
        }

        let mut status = self.read_status();
        status.starting = true;
        self.write_status(&status);

        if self.terracotta_port.lock().unwrap().is_none() {
            self.spawn_terracotta_process();
        }

        self.poll_terracotta_port();

        let port = *self.terracotta_port.lock().unwrap();
        match port {
            Some(p) => {
                let mut status = self.read_status();
                status.starting = false;
                status.running = true;
                status.port = Some(p);
                self.write_status(&status);
                Ok("陶瓦联机已启动".to_string())
            }
            None => {
                let mut status = self.read_status();
                status.starting = false;
                status.running = false;
                self.write_status(&status);
                Err("陶瓦联机启动超时".to_string())
            }
        }
    }

    fn spawn_terracotta_process(&self) {
        let terracotta_dir = self.adapter_dir.join("Terracotta");
        let exe_name = "terracotta-0.4.2-windows-x86_64.exe";
        let exe_path = terracotta_dir.join(exe_name);

        if !exe_path.exists() {
            let mut status = self.read_status();
            status.installed = false;
            self.write_status(&status);
            return;
        }

        let port_dir = self.adapter_dir.join("Terracotta.temp");
        let _ = std::fs::create_dir_all(&port_dir);
        let hmcl_file = port_dir.join("terracotta-port.json");
        let hmcl_str = hmcl_file.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&hmcl_file);

        let _ = std::process::Command::new(&exe_path)
            .args(["--hmcl", &hmcl_str])
            .current_dir(&terracotta_dir)
            .spawn();

        let status = self.read_status();
        let mut new_status = status.clone();
        new_status.installed = true;
        self.write_status(&new_status);
    }

    fn poll_terracotta_port(&self) {
        let port_dir = self.adapter_dir.join("Terracotta.temp");
        let hmcl_file = port_dir.join("terracotta-port.json");

        let start = Instant::now();
        let timeout = Duration::from_secs(12);
        loop {
            if start.elapsed() > timeout { break; }
            if let Ok(content) = std::fs::read_to_string(&hmcl_file) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(port) = json.get("port").and_then(|v| v.as_u64()) {
                        let port = port as u16;
                        if Self::is_port_alive(port) {
                            *self.terracotta_port.lock().unwrap() = Some(port);
                            println!("[适配器] 陶瓦联机端口: {}", port);
                            let _ = std::fs::remove_file(&hmcl_file);
                            break;
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    pub fn stop_terracotta(&self) -> Result<String, String> {
        let port = *self.terracotta_port.lock().unwrap();
        match port {
            Some(p) => {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(3))
                    .build().ok();
                if let Some(client) = client {
                    let url = format!("http://127.0.0.1:{}/panic?peaceful=true", p);
                    let _ = client.get(&url).send();
                }

                let wait_start = std::time::Instant::now();
                while wait_start.elapsed() < Duration::from_secs(5) {
                    if !Self::is_port_alive(p) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }

                *self.terracotta_port.lock().unwrap() = None;

                let mut status = self.read_status();
                status.running = false;
                status.starting = false;
                status.port = None;
                self.write_status(&status);
                Ok("陶瓦联机已停止".to_string())
            }
            None => Err("陶瓦联机未在运行".to_string())
        }
    }

    pub fn ensure_running(&self) -> Result<u16, String> {
        let port = *self.terracotta_port.lock().unwrap();
        if let Some(p) = port {
            return Ok(p);
        }

        let status = self.read_status();
        if !status.installed {
            return Err("陶瓦联机未安装".to_string());
        }

        self.spawn_terracotta_process();
        self.poll_terracotta_port();

        let port = *self.terracotta_port.lock().unwrap();
        match port {
            Some(p) => {
                let mut s = self.read_status();
                s.starting = false;
                s.running = true;
                s.port = Some(p);
                self.write_status(&s);
                Ok(p)
            }
            None => {
                let mut s = self.read_status();
                s.starting = false;
                self.write_status(&s);
                Err("陶瓦联机自动启动超时".to_string())
            }
        }
    }

    pub fn mark_installed(&self) {
        let mut status = self.read_status();
        status.installed = true;
        self.write_status(&status);
    }

    pub fn launch_terracotta_after_download(&self) {
        self.mark_installed();
        self.spawn_terracotta_process();
        self.poll_terracotta_port();
    }

    pub fn shutdown_all(&self) {
        self.running.store(false, Ordering::SeqCst);

        let port = *self.terracotta_port.lock().unwrap();
        if let Some(p) = port {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(3))
                .build().ok();
            if let Some(client) = client {
                let url = format!("http://127.0.0.1:{}/panic?peaceful=true", p);
                let _ = client.get(&url).send();
            }
            *self.terracotta_port.lock().unwrap() = None;
        }

        let mut status = self.read_status();
        status.running = false;
        status.starting = false;
        status.port = None;
        self.write_status(&status);
        println!("[适配器] 所有适配器已关闭");
    }
}
