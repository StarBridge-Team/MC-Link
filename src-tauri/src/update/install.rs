//! 更新落地：把"已下载的包"变成"新版本正在运行"。
//!
//! # 为什么必须由外部进程来做
//!
//! 覆盖正在运行的 exe、或运行安装器去替换本目录的文件，都必须发生在**本进程退出之后**，
//! 否则文件被占用（Windows 会直接拒绝写入正在运行的映像）。因此这里的做法是：生成一段
//! PowerShell，以 `-EncodedCommand`（UTF-16LE + Base64，绕开脚本编码问题）交给一个隐藏
//! 窗口的子进程，由它等待本进程退出（按 PID 轮询），再执行替换/安装，最后重新拉起应用。
//!
//! | 形态 | 做法 | 原因 |
//! |---|---|---|
//! | 便携版 | 原子替换 exe 同目录的可执行文件（先把旧 exe 备份为 `.bak`） | 目录里没有安装记录，替换 exe 就是全部 |
//! | 安装版 | 运行 NSIS 安装器并等它结束，再由我们拉起应用 | 系统目录 + 注册表由安装器管理，手改会留下不一致的安装记录 |
//!
//! # 失败语义（本次修复的重点）
//!
//! 旧实现把"真正危险的动作"（直接覆盖 exe）放在**退出之后**，失败时既不回滚、也不通知
//! 用户：用户看到的是"应用退出了、更新没发生、也没自动重启"，而唯一的退路 `.bak`
//! 还会在下次启动时被自动删掉。现在改成：
//!
//! 1. **预检在本进程内完成**：先把更新包复制到目标 exe 同目录的暂存文件（`xxx.new.exe`）。
//!    这一步失败就直接返回错误 —— **应用不退出**，前端能拿到可读的失败原因。
//! 2. **退出后只做原子替换**：落地脚本用 `File.Replace`（不可用时退回"删除 + 移动"），
//!    替换失败时目标 exe 保持原样，不会留下半截文件。
//! 3. **失败会回滚并拉起原程序**：目标缺失时用 `.bak` 恢复，并重新拉起旧版 exe。
//! 4. **失败一定会被用户看到**：落地脚本写下失败标记（`install-failed.txt`），下次启动时
//!    读出并通过 `update-install-error` 事件通知前端（见 `lib.rs`）。
//! 5. **`.bak` 只在落地确实成功时才删**：成功时写 `install.ok` 标记，启动时据此决定
//!    是否清理；没有标记就**保留** `.bak`（宁可多留一个备份，也不要删掉唯一的退路）。

use std::path::{Path, PathBuf};

use crate::datadir::{exe_path, install_mode, InstallMode};

use super::model::{UpdateAsset, KIND_INSTALLER, KIND_PORTABLE};

/// 落地脚本模板与占位符填充（单独成文件，见该文件的说明）。
mod scripts;

/// 落地流程的单元测试（单独成文件，见该文件的说明）。
#[cfg(test)]
mod tests;

use self::scripts::{installer_script, portable_script};

/// 落地结果：告诉调用方**能不能退出**。
///
/// 过去 `apply_update` 只返回一句话，调用方（`update::commands`）只能"拿到 Ok 就退出"。
/// 现在把"是否可以退出"显式表达出来：只有落地进程真的接管了后续动作才为 `true`。
#[derive(Debug, Clone)]
pub(crate) struct InstallOutcome {
    /// 面向用户的消息。
    pub message: String,
    /// 本进程是否可以立即退出（落地进程会负责重启应用）。
    ///
    /// `false` 表示**不要退出**：落地尚未接管，此时退出等于"应用凭空消失"。
    pub should_exit: bool,
}

/// 上一次落地失败的信息（由落地脚本写下的标记文件读出）。
#[derive(Debug, Clone)]
pub(crate) struct InstallFailure {
    /// 人可读的中文原因。
    pub reason: String,
    /// 受影响的目标路径。
    pub target: String,
}

/// 自动落地是否受支持。
///
/// - **Flatpak**：**恒不支持**。应用装在只读的 `/app` 里，由 `flatpak` 统一管理；
///   自行覆盖会绕开 OSTree 的版本记录。必须由用户执行 `flatpak update`。
///   这一条排在其它判断之前，因为它是"机制上不可能"，而不是"条件不满足"。
/// - **Windows**：始终支持——安装版交给 NSIS 安装器，便携版直接替换 exe；
/// - **Linux / macOS**：只有当官方更新插件可用时（已配置签名公钥）才支持，且落地由插件
///   完成。自行替换 deb/AppImage/`.app` 会破坏包管理器与 Gatekeeper 的记录，因此没有
///   插件时一律判定为不支持，界面只能引导手动下载。
///
/// 顺带决定"能否给用户一个可安装的资产"：返回 false 时选资产会得到 `None`。
pub(crate) fn is_auto_install_supported() -> bool {
    if crate::runtime::is_flatpak() {
        return false;
    }
    cfg!(windows) || super::plugin_updater::available()
}

/// 安装日志路径：放在更新缓存目录里，用户反馈"更新后打不开"时可直接取证。
fn install_log_path(data_dir: &Path) -> PathBuf {
    super::download::update_cache_dir(data_dir).join("install.log")
}

/// 标记文件路径（更新缓存目录内）。
///
/// - `install.ok`：**存在即说明上一次落地确实成功**，启动时据此决定要不要删 `.bak`；
/// - `install-failed.txt`：落地失败的原因（第一行）与受影响的目标（第二行）。
fn marker_path(data_dir: &Path, name: &str) -> PathBuf {
    super::download::update_cache_dir(data_dir).join(name)
}

/// 旧 exe 的备份路径。
fn bak_path(exe: &Path) -> PathBuf {
    let name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "mc-link.exe".to_string());
    exe.with_file_name(format!("{}.bak", name))
}

/// 暂存文件路径：与目标 exe 同目录、同卷，`File.Replace` 才能原子替换。
fn staged_path(exe: &Path) -> PathBuf {
    let name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "mc-link.exe".to_string());
    let stem = name.strip_suffix(".exe").unwrap_or(name.as_str());
    exe.with_file_name(format!("{}.new.exe", stem))
}

/// 启动落地流程（兼容入口：只要面向用户的那句话）。
///
/// 失败一律走 `Err`：此时**没有启动任何落地进程**，调用方**不要退出应用**。
pub(crate) fn apply_update(
    data_dir: &Path,
    asset: &UpdateAsset,
    payload: &Path,
) -> Result<String, String> {
    let outcome = apply_update_outcome(data_dir, asset, payload)?;
    // 旧调用方只要一句话。但"落地没接管"时**绝不能**让它以为可以退出 ——
    // 那会让应用凭空消失（旧实现正是栽在这里）。
    if !outcome.should_exit {
        return Err("更新尚未接管完成，请稍后重试".to_string());
    }
    Ok(outcome.message)
}

/// 启动落地流程，并明确回答"现在能不能退出"。
pub(crate) fn apply_update_outcome(
    data_dir: &Path,
    asset: &UpdateAsset,
    payload: &Path,
) -> Result<InstallOutcome, String> {
    if !is_auto_install_supported() {
        return Err("当前平台暂不支持自动安装更新，请手动下载安装包".to_string());
    }

    // 渠道闸门：命令层已据此收敛，这里是第二道防线——
    // 即使有人绕过界面直接调用命令，也不能让它替换掉开发构建或别人自构建的程序文件
    let channel = crate::build_channel::channel();
    if !crate::build_channel::update_allowed() {
        return Err(format!(
            "当前是{}，不会自动替换程序文件；请手动下载安装包",
            channel.display_name()
        ));
    }
    if !payload.is_file() {
        return Err(format!("更新包不存在: {}", payload.display()));
    }

    let target = exe_path().ok_or_else(|| "无法定位当前可执行文件，更新已中止".to_string())?;
    let log = install_log_path(data_dir);
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建更新缓存目录失败: {}", e))?;
    }

    match (install_mode(), asset.kind.as_str()) {
        (InstallMode::Portable, KIND_PORTABLE) => portable_install(data_dir, &target, payload, &log),
        (InstallMode::Installed, KIND_INSTALLER) => {
            installer_install(data_dir, &target, payload, &log)
        }
        (mode, kind) => {
            // 形态与包类型不匹配时宁可不动：把安装版包塞给便携版会直接损坏现有安装
            Err(format!(
                "更新包类型（{}）与当前安装形态（{}）不匹配，已中止以免破坏现有安装",
                kind,
                mode.as_str()
            ))
        }
    }
}

/// 便携版落地：替换 exe 同目录的可执行文件。
///
/// # 顺序很重要：先预检，再退出
///
/// 磁盘满、目录只读、权限不足、杀软锁定……这些失败都发生在"能不能把文件放到位"上。
/// 预检（把更新包复制成暂存文件）在本进程内、**应用还活着**的时候做完，失败就直接
/// 返回错误：用户看到的是"更新失败 + 原因"，而不是"应用退出后什么都没发生"。
///
/// 真正替换 exe 必须等本进程退出（文件被占用），剩余风险由落地脚本兜底：
/// 原子替换、失败回滚、失败标记、重新拉起旧版程序。
fn portable_install(
    data_dir: &Path,
    target: &Path,
    payload: &Path,
    log: &Path,
) -> Result<InstallOutcome, String> {
    let staged = stage_payload(target, payload).map_err(|e| {
        append_log(log, &format!("portable update failed (preflight): {}", e));
        e
    })?;

    let script = portable_script(
        target,
        payload,
        &staged,
        &bak_path(target),
        log,
        &marker_path(data_dir, "install.ok"),
        &marker_path(data_dir, "install-failed.txt"),
    );
    if let Err(e) = spawn_hidden_powershell(&script) {
        // 落地进程没起来：暂存文件没有用处，清掉免得留垃圾
        let _ = std::fs::remove_file(&staged);
        append_log(log, &format!("portable update failed (spawn): {}", e));
        return Err(e);
    }
    Ok(InstallOutcome {
        message: "更新已就绪，应用即将退出并自动重启".to_string(),
        should_exit: true,
    })
}

/// 安装版落地：运行 NSIS 安装器，由安装器替换系统目录里的文件。
///
/// 安装器必须在应用退出后运行（它要替换的正是被占用的文件），因此这里没有可预检的
/// "落盘"步骤；失败由落地脚本写失败标记，下次启动时通知用户。
fn installer_install(
    data_dir: &Path,
    target: &Path,
    payload: &Path,
    log: &Path,
) -> Result<InstallOutcome, String> {
    let script = installer_script(
        target,
        payload,
        log,
        &marker_path(data_dir, "install.ok"),
        &marker_path(data_dir, "install-failed.txt"),
    );
    spawn_hidden_powershell(&script)?;
    Ok(InstallOutcome {
        message: "更新已就绪，应用即将退出并自动重启".to_string(),
        should_exit: true,
    })
}

/// 把更新包复制到目标 exe 同目录的暂存文件。
///
/// 落地脚本稍后把它**原子地**换到目标位置：复制失败时目标 exe 原封不动。
fn stage_payload(target: &Path, payload: &Path) -> Result<PathBuf, String> {
    let staged = staged_path(target);
    let _ = std::fs::remove_file(&staged); // 上一次没走完的暂存文件先清掉

    let expected = std::fs::metadata(payload).map(|m| m.len()).unwrap_or(0);
    std::fs::copy(payload, &staged).map_err(|e| {
        format!(
            "无法把更新文件放到程序目录（{}）：{}。请检查磁盘空间与目录权限后重试",
            staged.display(),
            e
        )
    })?;

    let actual = std::fs::metadata(&staged).map(|m| m.len()).unwrap_or(0);
    if expected != 0 && actual != expected {
        let _ = std::fs::remove_file(&staged);
        return Err(format!(
            "更新文件复制不完整（{} 字节，应为 {} 字节）",
            actual, expected
        ));
    }
    Ok(staged)
}

/// 启动时的清理：上一次落地留下的旧 exe 备份、失败标记与下载残留。
///
/// 返回"上一次落地失败"的信息（若有）：调用方应把它通知给用户，否则用户只看到
/// "应用退出后没更新，也没重启"。
pub(crate) fn cleanup_leftovers(data_dir: &Path) -> Option<InstallFailure> {
    let failure = read_failure_marker(data_dir);
    if failure.is_some() {
        // 只提示一次：完整记录仍留在 install.log 里
        let _ = std::fs::remove_file(marker_path(data_dir, "install-failed.txt"));
    }

    // `.bak` 只在"上一次落地确实成功"时才删：没有成功标记说明上次落地可能失败或
    // 被中断 —— 宁可多留一个备份，也不要删掉唯一的退路。
    if should_clean_bak(marker_path(data_dir, "install.ok").is_file()) {
        if let Some(exe) = exe_path() {
            let _ = std::fs::remove_file(bak_path(&exe));
        }
        // 标记用完即弃：否则下一次失败落地会被"上一次成功"误判而删掉 `.bak`
        let _ = std::fs::remove_file(marker_path(data_dir, "install.ok"));
    }

    super::download::prune_cache(data_dir);
    failure
}

/// 是否可以在启动时删除旧 exe 的备份（`.bak`）。
///
/// 只有"上一次落地确实成功"（成功标记存在）时才删。纯函数：便于单测锁住这条规则。
pub(crate) fn should_clean_bak(success_marker_present: bool) -> bool {
    success_marker_present
}

/// 读出落地脚本写下的失败标记（没有则 `None`）。
fn read_failure_marker(data_dir: &Path) -> Option<InstallFailure> {
    let text = std::fs::read_to_string(marker_path(data_dir, "install-failed.txt")).ok()?;
    let mut lines = text.lines();
    // PowerShell 的 `-Encoding UTF8` 会写 BOM，读出来要先去掉
    let reason = lines
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches('\u{feff}')
        .to_string();
    if reason.is_empty() {
        return None;
    }
    let target = lines.next().unwrap_or_default().trim().to_string();
    Some(InstallFailure { reason, target })
}

/// 往安装日志里追加一行（Rust 侧失败时用；脚本侧自己带时间戳）。
fn append_log(log: &Path, message: &str) {
    use std::io::Write;
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        let _ = writeln!(file, "{}", message);
    }
}

// ------------------------------------------------------------------
// 脚本生成
// ------------------------------------------------------------------

/// 转义为 PowerShell 单引号字符串字面量（内部单引号需翻倍）。
fn ps_quote(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

/// 以隐藏窗口启动 PowerShell。
#[cfg(windows)]
fn spawn_hidden_powershell(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    /// 不弹出控制台窗口。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let encoded = encode_powershell(script);
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动更新进程失败，请手动安装更新包: {}", e))?;
    Ok(())
}

#[cfg(not(windows))]
fn spawn_hidden_powershell(_script: &str) -> Result<(), String> {
    Err("当前平台暂不支持自动安装更新，请手动下载安装包".to_string())
}

/// PowerShell `-EncodedCommand` 要求 UTF-16LE 字节流的 Base64。
#[cfg(windows)]
fn encode_powershell(script: &str) -> String {
    use base64::Engine as _;

    let bytes: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
