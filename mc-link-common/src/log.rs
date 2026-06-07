//! 日志输出模块

use std::time::{SystemTime, UNIX_EPOCH};

pub enum LogLevel {
    Info,
    Warn,
    Error,
}

pub fn log(level: LogLevel, msg: &str) {
    let now = SystemTime::now();
    let secs = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;

    let (level_str, color) = match level {
        LogLevel::Info => ("INFO", "\x1b[32m"),
        LogLevel::Warn => ("WARN", "\x1b[33m"),
        LogLevel::Error => ("ERROR", "\x1b[31m"),
    };

    println!(
        "[{:02}:{:02}:{:02} {}{}\x1b[0m] {}",
        hours, minutes, seconds, color, level_str, msg
    );
}