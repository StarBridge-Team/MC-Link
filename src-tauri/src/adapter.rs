//! 内置适配器的运行载体：管理陶瓦 exe 的生命周期、端口发现与状态落盘。
//!
//! 本模块**不再直接对外暴露 Tauri 命令**。它现在只服务于
//! [`crate::plugin::builtin::terracotta::TerracottaProvider`]，由后者把它包装成
//! 标准的 `adapter.*` 能力。对外接口统一走插件管理器。

use crate::utils::lock_or_recover;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 适配器状态。
///
/// 字段名与磁盘上的 `Terracotta.json` 以及前端契约保持一致，请勿重命名。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AdapterStatus {
    pub installed: bool,
    pub running: bool,
    pub starting: bool,
    pub port: Option<u16>,
}

pub struct AdapterManager {
    pub adapter_dir: PathBuf,
    pub terracotta_port: Arc<Mutex<Option<u16>>>,
}

impl AdapterManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let adapter_dir = data_dir.join("Adapter");

        AdapterManager {
            adapter_dir,
            terracotta_port: Arc::new(Mutex::new(None)),
        }
    }

    fn status_path(&self) -> PathBuf {
        self.adapter_dir.join("Terracotta.json")
    }

    fn read_status(&self) -> AdapterStatus {
        // 文件损坏时由 persist 隔离原文件并返回默认值
        crate::persist::load_json::<AdapterStatus>(&self.status_path())
            .map(|loaded| loaded.value)
            .unwrap_or_default()
    }

    fn write_status(&self, status: &AdapterStatus) {
        // 旧实现用 `let _ =` 吞掉落盘错误，导致"状态改了但没保存"无从察觉
        if let Err(e) = crate::persist::save_json(&self.status_path(), status) {
            eprintln!("[适配器] 状态落盘失败: {}", e);
        }
    }

    fn is_port_alive(port: u16) -> bool {
        std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(2),
        )
        .is_ok()
    }

    pub fn get_status(&self) -> AdapterStatus {
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        if let Some(p) = port {
            if !Self::is_port_alive(p) {
                *lock_or_recover(&self.terracotta_port, "terracotta_port") = None;
            }
        }

        let mut status = self.read_status();
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        status.running = port.is_some();
        status.port = port;
        self.write_status(&status);
        status
    }

    pub fn launch_all(&self) {
        if lock_or_recover(&self.terracotta_port, "terracotta_port").is_some() {
            return;
        }
        let status = self.read_status();
        if status.installed {
            self.spawn_terracotta_process();
            self.poll_terracotta_port();
        } else {
            println!("[适配器] 未安装，跳过阻塞下载（将由前端触发下载）");
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
            if start.elapsed() > timeout {
                break;
            }
            if let Ok(content) = std::fs::read_to_string(&hmcl_file) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(port) = json.get("port").and_then(|v| v.as_u64()) {
                        let port = port as u16;
                        if Self::is_port_alive(port) {
                            *lock_or_recover(&self.terracotta_port, "terracotta_port") = Some(port);
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

    pub fn ensure_running(&self) -> Result<u16, String> {
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        if let Some(p) = port {
            return Ok(p);
        }

        let status = self.read_status();
        if !status.installed {
            return Err("陶瓦联机未安装".to_string());
        }

        self.spawn_terracotta_process();
        self.poll_terracotta_port();

        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
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
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        if let Some(p) = port {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .ok();
            if let Some(client) = client {
                let url = format!("http://127.0.0.1:{}/panic?peaceful=true", p);
                let _ = client.get(&url).send();
            }
            *lock_or_recover(&self.terracotta_port, "terracotta_port") = None;
        }

        let mut status = self.read_status();
        status.running = false;
        status.starting = false;
        status.port = None;
        self.write_status(&status);
        println!("[适配器] 所有适配器已关闭");
    }
}
