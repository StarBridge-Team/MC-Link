//! 内置适配器的运行载体：管理陶瓦 exe 的生命周期、端口发现与状态落盘。
//!
//! 本模块**不再直接对外暴露 Tauri 命令**。它现在只服务于
//! [`crate::plugin::builtin::terracotta::TerracottaProvider`]，由后者把它包装成
//! 标准的 `adapter.*` 能力。对外接口统一走插件管理器。
//!
//! # 并发模型
//!
//! 旧实现是 `Arc<Mutex<AdapterManager>>`：一把**全局锁**包住整个管理器。
//! 那把锁并不保护任何真正会变的状态（管理器只有一个只读路径，和一个自带锁的
//! 端口槽），却把"拉起进程 + 12 秒端口探测"整体排他化了：
//!
//! - 一次端口探测最长 12 秒，期间**所有**适配器调用都被堵在锁上；
//! - 探测发生在 tokio worker 上时，会把该 worker 卡住 12 秒；
//! - 探测发生在主线程（退出路径）上时，整个界面会无响应 12 秒。
//!
//! 现在：
//!
//! - 管理器本身**不再被任何锁包住**（调用方直接持有 `Arc<AdapterManager>`）；
//! - 会变的端口值由 `terracotta_port` 自己的锁保护，且只在读写端口值的那几行持有；
//! - "拉起进程 + 端口探测"由 [`probe::LaunchSlot`] 去重：并发调用只会真正拉起一次，
//!   其余调用共享同一份结果；
//! - 端口探测在**后台线程**里进行，调用方只等结果，等待期间不持有任何锁。

mod exe;
mod probe;

use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::utils::lock_or_recover;
use exe::{describe_dir, find_terracotta_exe};
#[cfg(unix)]
use exe::{is_executable_file, make_executable};
use probe::{is_port_alive, poll_port, LaunchSlot, PORT_PROBE_TIMEOUT};

/// 等待端口探测结果的宽限时间（探测本身最多 12 秒）。
const PROBE_WAIT_SLACK: Duration = Duration::from_secs(2);

/// 退出路径上通知陶瓦"和平退出"的请求超时。
///
/// 旧实现是 3 秒：这个请求跑在**主线程**的退出路径上（`exit_app`），
/// 适配器不响应时整个界面会冻结 3 秒。本地回环请求正常情况下毫秒级返回，
/// 1 秒足够；超时即视为失败并忽略（退出不能被适配器拖住）。
const SHUTDOWN_HTTP_TIMEOUT: Duration = Duration::from_secs(1);

/// 适配器状态。
///
/// 字段名与磁盘上的 `Terracotta.json` 以及前端契约保持一致，请勿重命名。
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AdapterStatus {
    pub installed: bool,
    pub running: bool,
    pub starting: bool,
    pub port: Option<u16>,
    /// 落地目录里实际找到的陶瓦可执行文件名（不含路径）。
    ///
    /// 上游包内的可执行文件名随平台/版本变化（Windows 形如
    /// `terracotta-0.4.2-windows-x86_64.exe`，Unix 形如
    /// `terracotta-0.4.2-linux-x86_64`），因此客户端**不硬编码**文件名，
    /// 而是在落地目录里发现，并把结果记在这里供下次启动直接复用。
    ///
    /// 老状态文件没有这个字段时按 `#[serde(default)]` 视为 `None`，
    /// 此时会重新扫描目录，不影响升级。
    #[serde(default)]
    pub exe_name: Option<String>,
}

/// 内置适配器管理器。
///
/// 字段要么构造后只读，要么自带同步，因此**不需要**再套一把全局互斥锁：
/// 真正会变的只有 `terracotta_port`（有锁）与拉起任务（由 [`LaunchSlot`] 去重）。
pub struct AdapterManager {
    /// 落地目录（构造后不变）。
    pub adapter_dir: PathBuf,
    /// 陶瓦本地端口；只有这个值会变，因此只锁它。
    pub terracotta_port: Arc<Mutex<Option<u16>>>,
    /// "拉起进程 + 端口探测"的任务槽：去重并共享结果。
    launch: Arc<LaunchSlot>,
}

impl AdapterManager {
    pub fn new(data_dir: &Path) -> Self {
        let adapter_dir = data_dir.join("Adapter");

        AdapterManager {
            adapter_dir,
            terracotta_port: Arc::new(Mutex::new(None)),
            launch: Arc::new(LaunchSlot::new()),
        }
    }

    /// 取得管理器的借用句柄（兼容旧调用点）。
    ///
    /// # 为什么这里不再真的加锁
    ///
    /// 旧实现是 `Arc<Mutex<AdapterManager>>`，调用点用 `lock()` 取用管理器。
    /// 那把锁**不保护任何可变状态**（管理器只有一个只读路径，和一个自带锁的
    /// 端口槽），唯一的作用是把"整个管理器借出去"这件事排他化——于是连
    /// "拉起进程 + 12 秒端口探测"也被排他掉了。
    ///
    /// 现在可变的端口值由 `terracotta_port` 自己的锁保护、进程拉起由
    /// [`LaunchSlot`] 去重，因此这里**不需要**再上一把全局锁。
    /// 保留 `lock()` 这个名字与 `Result` 形状，是为了让既有调用点
    /// （`plugin/manager/adapter.rs`）无需改动即可继续编译——它现在返回的
    /// 是一个**不阻塞**的借用句柄。
    pub fn lock(&self) -> Result<AdapterGuard<'_>, String> {
        Ok(AdapterGuard(self))
    }

    fn read_status(&self) -> AdapterStatus {
        read_status_file(&self.adapter_dir)
    }

    fn write_status(&self, status: &AdapterStatus) {
        write_status_file(&self.adapter_dir, status);
    }

    /// 当前适配器状态。
    ///
    /// TCP 连通性探测（最多 2 秒）**不在任何锁内**进行：先把端口值拷出来，
    /// 释放锁，再去连端口。
    pub fn get_status(&self) -> AdapterStatus {
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        let port = match port {
            Some(p) if is_port_alive(p) => Some(p),
            Some(_) => {
                // 端口已经没人听了：清掉，避免状态里留下"假运行"。
                *lock_or_recover(&self.terracotta_port, "terracotta_port") = None;
                None
            }
            None => None,
        };

        let mut status = self.read_status();
        status.running = port.is_some();
        status.port = port;
        self.write_status(&status);
        status
    }

    /// 启动内置适配器（应用启动时调用）。
    ///
    /// 内部会等待端口探测结果（上限 12 秒），调用方应当把它放到阻塞线程上执行。
    pub fn launch_all(&self) {
        if lock_or_recover(&self.terracotta_port, "terracotta_port").is_some() {
            return;
        }
        let status = self.read_status();
        if !status.installed {
            println!("[适配器] 未安装，跳过阻塞下载（将由前端触发下载）");
            return;
        }
        // 拉起 + 探测：轮询在后台线程进行，这里只等结果。
        let _ = self.ensure_running();
    }

    /// 确保陶瓦已安装并运行，返回其本地 HTTP 端口。
    ///
    /// 失败时的错误文案与旧实现保持一致（"陶瓦联机未安装" /
    /// "陶瓦联机自动启动超时"）。
    pub fn ensure_running(&self) -> Result<u16, String> {
        let mut retried = false;
        loop {
            if let Some(p) = *lock_or_recover(&self.terracotta_port, "terracotta_port") {
                return Ok(p);
            }

            let status = self.read_status();
            if !status.installed {
                return Err("陶瓦联机未安装".to_string());
            }

            // 抢占或加入一次"拉起 + 探测"任务：并发调用只会真正拉起一次进程。
            let owner = self.launch.try_begin();
            if owner {
                self.spawn_launch_worker();
            }
            let outcome = self.launch.wait(PORT_PROBE_TIMEOUT + PROBE_WAIT_SLACK);

            match outcome {
                Some(Ok(p)) => {
                    *lock_or_recover(&self.terracotta_port, "terracotta_port") = Some(p);
                    let mut s = self.read_status();
                    s.starting = false;
                    s.running = true;
                    s.port = Some(p);
                    self.write_status(&s);
                    return Ok(p);
                }
                Some(Err(e)) => {
                    if owner || retried {
                        // 自己拉起的失败（或已经重试过一次）：直接报错。
                        let mut s = self.read_status();
                        s.starting = false;
                        self.write_status(&s);
                        return Err(e);
                    }
                    // 别人拉起失败：轮到自己，重新来一次。
                    // 旧实现里后来的调用者就是各自重试一遍（锁释放后再拉起），
                    // 这里保持同样的语义，但只重试一次，避免无限循环。
                    retried = true;
                }
                None => {
                    let mut s = self.read_status();
                    s.starting = false;
                    self.write_status(&s);
                    return Err("陶瓦联机自动启动超时".to_string());
                }
            }
        }
    }

    pub fn mark_installed(&self) {
        let mut status = self.read_status();
        status.installed = true;
        self.write_status(&status);
    }

    /// 下载完成后拉起陶瓦并等待端口就绪。
    pub fn launch_terracotta_after_download(&self) {
        self.mark_installed();
        let owner = self.launch.try_begin();
        if owner {
            self.spawn_launch_worker();
        }
        // 等结果（上限 12 秒）：与旧实现一致，只是轮询跑在后台线程上。
        let _ = self.launch.wait(PORT_PROBE_TIMEOUT + PROBE_WAIT_SLACK);
    }

    /// 关闭适配器：通知陶瓦和平退出并清空状态。
    ///
    /// 不在持有端口锁时发 HTTP 请求；请求超时会**忽略**（退出路径不能被
    /// 一个不响应的适配器拖住）。
    pub fn shutdown_all(&self) {
        let port = *lock_or_recover(&self.terracotta_port, "terracotta_port");
        if let Some(p) = port {
            // 先清端口再做请求：请求期间不应让"端口还活着"的状态继续存在。
            *lock_or_recover(&self.terracotta_port, "terracotta_port") = None;

            let url = format!("http://127.0.0.1:{}/panic?peaceful=true", p);
            if let Ok(client) = reqwest::blocking::Client::builder()
                .connect_timeout(SHUTDOWN_HTTP_TIMEOUT)
                .timeout(SHUTDOWN_HTTP_TIMEOUT)
                .build()
            {
                // 失败即忽略：陶瓦可能已经不在了，退出不该因此失败。
                let _ = client.get(&url).send();
            }
        }

        let mut status = self.read_status();
        status.running = false;
        status.starting = false;
        status.port = None;
        self.write_status(&status);
        println!("[适配器] 所有适配器已关闭");
    }

    /// 在后台线程里执行"拉起陶瓦 + 探测端口"。
    ///
    /// 线程创建失败（极端情况）时也必须把结果发出去，否则等待者会一直等到超时。
    fn spawn_launch_worker(&self) {
        let dir = self.adapter_dir.clone();
        let port = self.terracotta_port.clone();
        let launch = self.launch.clone();

        let spawned = std::thread::Builder::new()
            .name("terracotta-port-probe".to_string())
            .spawn(move || {
                spawn_terracotta_process(&dir);
                let outcome = match poll_port(&dir, PORT_PROBE_TIMEOUT) {
                    Some(p) => Ok(p),
                    None => Err("陶瓦联机自动启动超时".to_string()),
                };
                if let Ok(p) = outcome {
                    *lock_or_recover(&port, "terracotta_port") = Some(p);
                }
                launch.finish(outcome);
            });

        if let Err(e) = spawned {
            self.launch
                .finish(Err(format!("陶瓦联机自动启动失败: {}", e)));
        }
    }
}

/// [`AdapterManager`] 的借用句柄。
///
/// 旧实现里它是 `MutexGuard`；现在管理器内部自带同步，这个句柄**不会阻塞**。
pub struct AdapterGuard<'a>(&'a AdapterManager);

impl Deref for AdapterGuard<'_> {
    type Target = AdapterManager;

    fn deref(&self) -> &AdapterManager {
        self.0
    }
}

/// 状态文件路径。
fn status_path(dir: &Path) -> PathBuf {
    dir.join("Terracotta.json")
}

/// 读取状态文件（文件损坏时由 persist 隔离原文件并返回默认值）。
fn read_status_file(dir: &Path) -> AdapterStatus {
    crate::persist::load_json::<AdapterStatus>(&status_path(dir))
        .map(|loaded| loaded.value)
        .unwrap_or_default()
}

/// 写状态文件。
fn write_status_file(dir: &Path, status: &AdapterStatus) {
    // 旧实现用 `let _ =` 吞掉落盘错误，导致"状态改了但没保存"无从察觉
    if let Err(e) = crate::persist::save_json(&status_path(dir), status) {
        eprintln!("[适配器] 状态落盘失败: {}", e);
    }
}

/// 定位落地目录里的陶瓦可执行文件。
///
/// 先用状态文件里记住的文件名（省一次目录扫描），找不到再按规则发现。
fn resolve_terracotta_exe(dir: &Path, platform: &str) -> Option<PathBuf> {
    let status = read_status_file(dir);
    if let Some(name) = status.exe_name.as_deref() {
        // 只接受单层文件名，避免状态文件被篡改后越出落地目录
        if Path::new(name).components().count() == 1 {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    find_terracotta_exe(dir, platform)
}

/// 拉起陶瓦进程（同步、很快返回）。
fn spawn_terracotta_process(adapter_dir: &Path) {
    let terracotta_dir = adapter_dir.join("Terracotta");
    let platform = crate::assets::adapter::current_platform();

    let Some(exe_path) = resolve_terracotta_exe(&terracotta_dir, &platform) else {
        let mut status = read_status_file(adapter_dir);
        status.installed = false;
        status.exe_name = None;
        write_status_file(adapter_dir, &status);
        eprintln!(
            "[适配器] 在 {} 里没有找到陶瓦可执行文件（该目录下的内容：{}），无法启动",
            terracotta_dir.display(),
            describe_dir(&terracotta_dir)
        );
        return;
    };

    // Unix 上 tar 解压可能丢可执行位：尽力补上，补不上就跳过启动（不 panic）
    #[cfg(unix)]
    if !is_executable_file(&exe_path) {
        if let Err(e) = make_executable(&exe_path) {
            eprintln!(
                "[适配器] 无法为 {} 补上可执行位（{}），已跳过启动",
                exe_path.display(),
                e
            );
            return;
        }
    }

    let port_dir = adapter_dir.join("Terracotta.temp");
    let _ = std::fs::create_dir_all(&port_dir);
    let hmcl_file = port_dir.join("terracotta-port.json");
    let hmcl_str = hmcl_file.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&hmcl_file);

    if let Err(e) = std::process::Command::new(&exe_path)
        .args(["--hmcl", &hmcl_str])
        .current_dir(&terracotta_dir)
        .spawn()
    {
        eprintln!("[适配器] 启动陶瓦进程失败（{}）: {}", exe_path.display(), e);
    }

    let mut new_status = read_status_file(adapter_dir);
    new_status.installed = true;
    new_status.exe_name = exe_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    write_status_file(adapter_dir, &new_status);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每次调用都用唯一目录，避免并行测试互相看到对方的残留文件。
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mc-link-adapter-{}-{}",
            name,
            crate::plugin::crypto::random_hex(4)
        ));
        let _ = std::fs::create_dir_all(dir.join("Adapter"));
        dir
    }

    #[test]
    fn ensure_running_without_install_reports_not_installed() {
        let dir = temp_dir("not-installed");
        let mgr = AdapterManager::new(&dir);
        let err = mgr
            .ensure_running()
            .expect_err("未安装时不应返回端口");
        assert_eq!(err, "陶瓦联机未安装");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_status_drops_a_port_that_is_not_listening() {
        let dir = temp_dir("dead-port");
        let mgr = AdapterManager::new(&dir);
        // 端口 1 必然没有监听：TCP 探测会立刻失败。
        *lock_or_recover(&mgr.terracotta_port, "terracotta_port") = Some(1);
        let status = mgr.get_status();
        assert!(!status.running, "端口不通时不应报告为运行中");
        assert!(status.port.is_none(), "端口不通时应清掉记住的端口");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shutdown_all_clears_the_remembered_port() {
        let dir = temp_dir("shutdown");
        let mgr = AdapterManager::new(&dir);
        *lock_or_recover(&mgr.terracotta_port, "terracotta_port") = Some(1);
        mgr.shutdown_all();
        assert!(
            lock_or_recover(&mgr.terracotta_port, "terracotta_port").is_none(),
            "关闭后不应再记住端口"
        );
        let status = mgr.get_status();
        assert!(!status.running);
        assert!(status.port.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lock_returns_a_non_blocking_borrow() {
        let dir = temp_dir("guard");
        let mgr = AdapterManager::new(&dir);
        let guard = mgr.lock().expect("借用句柄不应失败");
        // 句柄能直接用到底下的方法（不再需要显式解锁）。
        assert_eq!(guard.adapter_dir, dir.join("Adapter"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
