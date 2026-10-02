//! HTTP 服务器：监听循环 + 各职责子模块。
//!
//! 拆分前这里是 1000+ 行的单文件，路由、静态文件与 Range、上传与鉴权、路径穿越防护、
//! 查询串解析全挤在一起——问题不在于行数，而在于**两个安全敏感块（路径与鉴权）
//! 被埋在 HTTP 管道代码中间**，review 时要来回翻。
//!
//! 各模块职责：
//! - [`routes`]：看 URL 决定交给谁
//! - [`files`]：静态文件、MIME、`Range` 断点续传、元配置与目录清单
//! - [`upload`]：写入类接口（上传/删除）与令牌鉴权
//! - [`paths`]：路径穿越防护
//! - [`query`]：查询串解析与百分号解码
//! - [`response`]：响应类型与构造辅助（长度表达、CORS、错误响应）

mod files;
mod paths;
mod query;
mod response;
mod routes;
mod upload;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use tiny_http::Server;

use crate::config::AssetsConfig;

use routes::route;
use upload::redact_token;

/// 处理请求的工作线程数。
///
/// 单线程时一个慢客户端就能阻塞全部请求（慢速 DoS），而下载大包时这种阻塞很常见。
const HTTP_WORKERS: usize = 4;

/// 启动资产服务器（阻塞当前线程）
pub fn run_server(config: AssetsConfig) {
    let cfg = Arc::new(config);
    let listen = cfg.listen_addr();
    let server = match Server::http(&listen) {
        Ok(s) => {
            println!();
            println!("===========================================");
            println!("  MC Link 资产服务器 v0.1");
            println!("===========================================");
            println!("  监听: http://{}", listen);
            println!("  根目录: {}", cfg.root_dir.display());
            // 启动时把写入模式说清楚：否则"上传一直 503"会被当成 bug 排查半天
            if cfg.upload_token.is_empty() {
                println!("  写入: 已禁用（未配置 upload_token，/upload 与 /delete 一律拒绝）");
            } else {
                println!("  写入: 已启用（需要 Authorization: Bearer <token>）");
            }
            println!("===========================================");
            println!();
            s
        }
        Err(e) => {
            eprintln!("[启动] 服务器绑定 {} 失败: {}", listen, e);
            eprintln!(
                "       最常见的原因是有另一个实例仍在运行：先结束它，或换个端口（--port <端口>）"
            );
            // 必须以非零码退出：否则 `concurrently` / CI 会把"端口被占"当成正常结束，
            // 前台只剩一句 vite 的端口冲突，真正的原因被淹没（实际排查时踩过）。
            std::process::exit(1);
        }
    };

    // 多工作线程：单线程时一个慢客户端（或一次大文件传输）就能阻塞**全部**请求，
    // 是现成的慢速 DoS。`tiny_http::Server` 是 Sync 的，多线程同时 recv 即可。
    let server = Arc::new(server);
    let mut workers = Vec::new();
    for _ in 0..HTTP_WORKERS {
        let server = server.clone();
        let cfg = cfg.clone();
        workers.push(std::thread::spawn(move || loop {
            let mut request = match server.recv() {
                Ok(r) => r,
                // 连接中断之类不是错误，继续等下一个请求
                Err(_) => continue,
            };

            if cfg.access_log {
                // 脱敏后再打：URL 里的 `?token=` 会被写进 stdout、反代日志与 Referer
                println!(
                    "[{}] {}",
                    request.method(),
                    redact_token(&request.url().to_string())
                );
            }

            let response = route(&mut request, &cfg);

            let _ = request.respond(response);
        }));
    }
    for worker in workers {
        let _ = worker.join();
    }
}
