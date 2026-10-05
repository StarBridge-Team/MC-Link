//! 最小复现：对**大尺寸真实图片**取种子色是否爆栈。
//!
//! 用法：
//!   cargo run --example seed_repro -- <图片路径>          # 完整取色
//!   cargo run --example seed_repro -- <图片路径> resize   # 只解码+缩放
//!
//! 为什么需要它：单元测试里的手写 PNG（1×1 / 256×256 纯色）都通过，
//! 但真实壁纸（实测 3840×2160 JPEG）会让它爆栈。测试环境的输入覆盖不到真实规模。
//! 它故意跑在**默认栈**的 `main` 线程上——要复现的就是"栈不够"。

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let path: PathBuf = args.next().map(PathBuf::from).expect("用法: <图片路径> [resize] [stack_kb]");
    let only_resize = args.next().as_deref() == Some("resize");
    let stack_kb: usize = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0); // 0 = 用默认栈

    let meta = std::fs::metadata(&path).expect("读不到文件");
    println!(
        "文件 = {}（{} 字节），模式 = {}，栈 = {}",
        path.display(),
        meta.len(),
        if only_resize { "只解码+缩放" } else { "完整取色" },
        if stack_kb == 0 { "默认".to_string() } else { format!("{stack_kb} KiB") }
    );
    use std::io::Write;
    let _ = std::io::stdout().flush();

    // 显式指定栈大小地跑：用来找出"多大栈才够"，而不是靠猜。
    let run = move || {
        if only_resize {
            mc_link_lib::seed_probe::decode_and_resize_only(&path)
        } else {
            mc_link_lib::seed_probe::extract_seed_from_file_inline(&path)
        }
    };

    let result = if stack_kb == 0 {
        run()
    } else {
        std::thread::Builder::new()
            .stack_size(stack_kb * 1024)
            .spawn(run)
            .expect("创建线程失败")
            .join()
            .unwrap_or_else(|_| Err("线程异常结束".to_string()))
    };

    match result {
        Ok(v) => println!("成功: {v}"),
        Err(e) => println!("失败（不是崩溃）: {e}"),
    }
    println!("未爆栈，正常结束");
}
