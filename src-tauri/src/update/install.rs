//! 更新落地：把"已下载的包"变成"新版本正在运行"。
//!
//! # 为什么必须由外部进程来做
//!
//! 覆盖正在运行的 exe、或运行安装器去替换本目录的文件，都必须发生在**本进程退出之后**，
//! 否则文件被占用（Windows 会直接拒绝写入正在运行的映像）。
//!
//! 因此这里的做法是：生成一段 PowerShell，以 `-EncodedCommand` 交给一个隐藏窗口的
//! 子进程，由它等待本进程退出（按 PID 轮询），再执行替换/安装，最后重新拉起应用。
//!
//! 用 `-EncodedCommand`（UTF-16LE + Base64）而不是写临时 `.ps1`/`.cmd` 文件，
//! 是为了彻底绕开脚本编码问题——安装路径里出现中文时，写文件很容易踩代码页的坑。
//!
//! # 两种安装形态的落地方式
//!
//! | 形态 | 做法 | 原因 |
//! |---|---|---|
//! | 便携版 | 覆盖 exe 同目录的可执行文件（先把旧 exe 备份为 `.bak`） | 目录里没有安装记录，替换 exe 就是全部 |
//! | 安装版 | 运行 NSIS 安装器并等它结束，再由我们拉起应用 | 系统目录 + 注册表由安装器管理，手改会留下不一致的安装记录 |
//!
//! 安装器的"完成后运行"可能也会拉起一次应用，`single-instance` 插件会把它折叠成
//! 聚焦已有窗口，不会出现两个实例。

use std::path::{Path, PathBuf};

use crate::datadir::{exe_path, install_mode, InstallMode};

use super::model::{UpdateAsset, KIND_INSTALLER, KIND_PORTABLE};

/// 本进程退出后，落地进程愿意等待的上限（超过则放弃，并把可手动打开的包留在缓存里）。
const WAIT_EXIT_SECS: u64 = 120;

/// 自动落地是否受支持。
///
/// - **Windows**：始终支持——安装版交给 NSIS 安装器，便携版直接替换 exe；
/// - **Linux / macOS**：只有当官方更新插件可用时（已配置签名公钥）才支持，
///   且落地由插件完成。自行替换 deb/AppImage/`.app` 会破坏包管理器与 Gatekeeper
///   的记录，因此没有插件时一律判定为不支持，界面只能引导手动下载。
///
/// 顺带决定"能否给用户一个可安装的资产"：返回 false 时选资产会得到 `None`。
pub(crate) fn is_auto_install_supported() -> bool {
    cfg!(windows) || super::plugin_updater::available()
}

/// 安装日志路径：放在更新缓存目录里，用户反馈"更新后打不开"时可直接取证。
fn install_log_path(data_dir: &Path) -> PathBuf {
    super::download::update_cache_dir(data_dir).join("install.log")
}

/// 旧 exe 的备份路径。
fn bak_path(exe: &Path) -> PathBuf {
    let name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "mc-link.exe".to_string());
    exe.with_file_name(format!("{}.bak", name))
}

/// 启动落地流程。
///
/// 返回后本进程**必须尽快退出**（调用方负责），因为后台进程正在等待它结束。
pub(crate) fn apply_update(
    data_dir: &Path,
    asset: &UpdateAsset,
    payload: &Path,
) -> Result<String, String> {
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

    let script = match (install_mode(), asset.kind.as_str()) {
        (InstallMode::Portable, KIND_PORTABLE) => portable_script(&target, payload, &log),
        (InstallMode::Installed, KIND_INSTALLER) => installer_script(&target, payload, &log),
        (mode, kind) => {
            // 形态与包类型不匹配时宁可不动：把安装版包塞给便携版会直接损坏现有安装
            return Err(format!(
                "更新包类型（{}）与当前安装形态（{}）不匹配，已中止以免破坏现有安装",
                kind,
                mode.as_str()
            ));
        }
    };

    spawn_hidden_powershell(&script)?;
    Ok("更新已就绪，应用即将退出并自动重启".to_string())
}

/// 启动时的清理：上一次落地留下的旧 exe 备份，以及更新缓存里已用完的包。
///
/// 在启动时调用：此时旧进程必然已经退出，这些文件不再被占用。
/// 清理失败不影响使用（下次启动再试）。
pub(crate) fn cleanup_leftovers(data_dir: &Path) {
    if let Some(exe) = exe_path() {
        let _ = std::fs::remove_file(bak_path(&exe));
    }
    super::download::prune_cache(data_dir);
}

// ------------------------------------------------------------------
// 脚本生成
// ------------------------------------------------------------------

/// 便携版：覆盖 exe，再拉起应用。
const PORTABLE_SCRIPT: &str = r#"$ErrorActionPreference = 'Continue'
$log = '__LOG__'
function Write-Log([string]$m) { try { Add-Content -LiteralPath $log -Value ((Get-Date).ToString('s') + ' ' + $m) -Encoding UTF8 } catch { } }
Write-Log 'portable update: waiting for pid __PID__ to exit'
$deadline = (Get-Date).AddSeconds(__WAIT__)
while ($true) {
  if (-not (Get-Process -Id __PID__ -ErrorAction SilentlyContinue)) { break }
  if ((Get-Date) -gt $deadline) { Write-Log 'portable update: wait timed out'; break }
  Start-Sleep -Milliseconds 300
}
Start-Sleep -Milliseconds 600
$target = '__TARGET__'
try {
  if (Test-Path -LiteralPath $target) { Copy-Item -LiteralPath $target -Destination '__BAK__' -Force -ErrorAction SilentlyContinue }
  Copy-Item -LiteralPath '__PAYLOAD__' -Destination $target -Force
  Write-Log 'portable update: exe replaced'
  Start-Process -FilePath $target -WorkingDirectory (Split-Path -Parent $target)
  Write-Log 'portable update: relaunched'
} catch {
  Write-Log ('portable update failed: ' + $_.Exception.Message)
  Start-Process -FilePath 'explorer.exe' -ArgumentList ('/select,"__PAYLOAD__"')
}
"#;

/// 安装版：运行安装器并等它结束，再拉起应用。
const INSTALLER_SCRIPT: &str = r#"$ErrorActionPreference = 'Continue'
$log = '__LOG__'
function Write-Log([string]$m) { try { Add-Content -LiteralPath $log -Value ((Get-Date).ToString('s') + ' ' + $m) -Encoding UTF8 } catch { } }
Write-Log 'installer update: waiting for pid __PID__ to exit'
$deadline = (Get-Date).AddSeconds(__WAIT__)
while ($true) {
  if (-not (Get-Process -Id __PID__ -ErrorAction SilentlyContinue)) { break }
  if ((Get-Date) -gt $deadline) { Write-Log 'installer update: wait timed out'; break }
  Start-Sleep -Milliseconds 300
}
Start-Sleep -Milliseconds 600
try {
  $p = Start-Process -FilePath '__PAYLOAD__' -PassThru
  Write-Log ('installer update: installer started, pid ' + $p.Id)
  Wait-Process -Id $p.Id -ErrorAction SilentlyContinue
  Write-Log 'installer update: installer exited'
} catch {
  Write-Log ('installer update failed: ' + $_.Exception.Message)
}
Start-Sleep -Milliseconds 800
try {
  Start-Process -FilePath '__TARGET__' -WorkingDirectory (Split-Path -Parent '__TARGET__')
  Write-Log 'installer update: relaunched'
} catch {
  Write-Log ('installer update: relaunch failed: ' + $_.Exception.Message)
}
"#;

fn portable_script(target: &Path, payload: &Path, log: &Path) -> String {
    render(
        PORTABLE_SCRIPT,
        &[
            ("__LOG__", &ps_quote(log)),
            ("__TARGET__", &ps_quote(target)),
            ("__BAK__", &ps_quote(&bak_path(target))),
            ("__PAYLOAD__", &ps_quote(payload)),
        ],
    )
}

fn installer_script(target: &Path, payload: &Path, log: &Path) -> String {
    render(
        INSTALLER_SCRIPT,
        &[
            ("__LOG__", &ps_quote(log)),
            ("__TARGET__", &ps_quote(target)),
            ("__PAYLOAD__", &ps_quote(payload)),
        ],
    )
}

/// 填充模板里的占位符（含进程号与等待时限）。
fn render(template: &str, pairs: &[(&str, &String)]) -> String {
    let mut out = template
        .replace("__PID__", &std::process::id().to_string())
        .replace("__WAIT__", &WAIT_EXIT_SECS.to_string());
    for (key, value) in pairs {
        out = out.replace(key, value);
    }
    out
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bak_path_keeps_extension_and_sits_next_to_exe() {
        let exe = Path::new("C:\\portable\\MC Link.exe");
        assert_eq!(bak_path(exe), Path::new("C:\\portable\\MC Link.exe.bak"));
    }

    #[test]
    fn scripts_keep_placeholders_filled_and_no_leftover_marker() {
        let target = Path::new("C:\\portable\\MC Link.exe");
        let payload = Path::new("C:\\cache\\new.exe");
        let log = Path::new("C:\\cache\\install.log");

        for script in [
            portable_script(target, payload, log),
            installer_script(target, payload, log),
        ] {
            assert!(!script.contains("__"), "占位符未被全部替换: {}", script);
            assert!(script.contains(&std::process::id().to_string()));
            assert!(
                script.contains("Copy-Item") || script.contains("Start-Process"),
                "落地脚本应包含替换或安装动作"
            );
        }

        // 便携版必须备份旧 exe：替换失败时用户还能手动回退
        let portable = portable_script(target, payload, log);
        assert!(portable.contains("MC Link.exe.bak"));
    }

    #[test]
    fn ps_quote_escapes_single_quotes() {
        assert_eq!(
            ps_quote(Path::new("C:\\it's here\\a.exe")),
            "C:\\it''s here\\a.exe"
        );
    }
}
