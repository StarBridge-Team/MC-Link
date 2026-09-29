//! MC Link 资产服务器入口

use std::path::PathBuf;
use mc_link_assets::{AssetsConfig, run_server, prepare_from_npm};

fn main() {
    let _ = env_logger::try_init();

    // 默认配置文件路径：可执行文件同目录 config.yml
    let config_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.yml")))
        .unwrap_or_else(|| PathBuf::from("config.yml"));

    let mut config = AssetsConfig::load(&config_path);

    // 命令行参数：--port <PORT>  --bind <ADDR>  --root <DIR>
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(p) = args.next().and_then(|s| s.parse().ok()) {
                    config.port = p;
                }
            }
            "--bind" => {
                if let Some(a) = args.next() {
                    config.bind = a;
                }
            }
            "--root" => {
                if let Some(d) = args.next() {
                    config.root_dir = PathBuf::from(d);
                }
            }
            _ => {}
        }
    }

    // dev 环境：检测到 node_modules 时自动同步前端资源到 Assets/
    // 必须在命令行参数解析之后，确保 root_dir 已被 --root 覆盖
    prepare_from_npm(&config.root_dir);

    run_server(config);
}
