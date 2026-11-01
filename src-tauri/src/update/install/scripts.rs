//! 落地脚本模板（PowerShell）与占位符填充。
//!
//! 单独成文件是为了让 [`super`] 只留"流程"，脚本细节（尤其是失败分支）
//! 集中在一处，便于逐条核对。
//!
//! # 为什么用 `-EncodedCommand`
//!
//! 脚本以 UTF-16LE + Base64 交给 `powershell -EncodedCommand`，而不是写成临时
//! `.ps1`/`.cmd` 文件：安装路径里出现中文时，写文件很容易踩代码页的坑。

use std::path::Path;

use super::ps_quote;

/// 本进程退出后，落地进程愿意等待的上限（超过则放弃，并把可手动打开的包留在缓存里）。
const WAIT_EXIT_SECS: u64 = 120;

/// 便携版：原子替换 exe，再拉起应用。
///
/// 关键点：
/// - 等本进程退出后才动目标文件（Windows 不允许覆盖正在运行的映像）；
/// - 用 `File.Replace` 做**原子**替换：目标要么是旧的，要么是新的，不会是半截文件；
/// - 失败时用 `.bak` 回滚、写下失败标记，并把**原程序**重新拉起来；
/// - 成功时写成功标记，启动时据此清理 `.bak`。
pub(super) const PORTABLE_SCRIPT: &str = r#"$ErrorActionPreference = 'Continue'
$log = '__LOG__'
$target = '__TARGET__'
$bak = '__BAK__'
$staged = '__STAGED__'
$okMarker = '__OK__'
$failMarker = '__FAIL__'
function Write-Log([string]$m) { try { Add-Content -LiteralPath $log -Value ((Get-Date).ToString('s') + ' ' + $m) -Encoding UTF8 } catch { } }
function Write-Fail([string]$m, [string]$t) {
  Write-Log ('portable update failed: ' + $m)
  try { Set-Content -LiteralPath $failMarker -Value ($m + [Environment]::NewLine + $t) -Encoding UTF8 } catch { }
}
Write-Log 'portable update: waiting for pid __PID__ to exit'
$deadline = (Get-Date).AddSeconds(__WAIT__)
$exited = $false
while ($true) {
  if (-not (Get-Process -Id __PID__ -ErrorAction SilentlyContinue)) { $exited = $true; break }
  if ((Get-Date) -gt $deadline) { Write-Log 'portable update: wait timed out'; break }
  Start-Sleep -Milliseconds 300
}
if (-not $exited) {
  Write-Fail '应用未在超时时间内退出，本次更新已放弃，请稍后重试' $target
  Start-Process -FilePath 'explorer.exe' -ArgumentList ('/select,"__PAYLOAD__"')
  exit 0
}
Start-Sleep -Milliseconds 600
try {
  if (-not (Test-Path -LiteralPath $staged)) { throw ('暂存文件不存在：' + $staged) }
  if (Test-Path -LiteralPath $target) { Copy-Item -LiteralPath $target -Destination $bak -Force -ErrorAction Stop }
  try {
    # 原子替换：目标要么是旧的，要么是新的，不会出现"半个 exe"
    [System.IO.File]::Replace($staged, $target, $null)
  } catch {
    Write-Log ('portable update: atomic replace unavailable, fallback: ' + $_.Exception.Message)
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Force -ErrorAction Stop }
    Move-Item -LiteralPath $staged -Destination $target -Force -ErrorAction Stop
  }
  Write-Log 'portable update: exe replaced'
  try { Set-Content -LiteralPath $okMarker -Value ((Get-Date).ToString('s')) -Encoding UTF8 } catch { Write-Log 'portable update: success marker not written' }
  Start-Process -FilePath $target -WorkingDirectory (Split-Path -Parent $target)
  Write-Log 'portable update: relaunched'
} catch {
  $msg = $_.Exception.Message
  Write-Log ('portable update failed: ' + $msg)
  if (-not (Test-Path -LiteralPath $target)) {
    if (Test-Path -LiteralPath $bak) {
      try { Copy-Item -LiteralPath $bak -Destination $target -Force -ErrorAction Stop; Write-Log 'portable update: restored from backup' } catch { Write-Log ('portable update: restore failed: ' + $_.Exception.Message) }
    }
  }
  Write-Fail ('替换程序文件失败：' + $msg) $target
  if (Test-Path -LiteralPath $target) {
    Start-Process -FilePath $target -WorkingDirectory (Split-Path -Parent $target)
    Write-Log 'portable update: relaunched original'
  } else {
    Start-Process -FilePath 'explorer.exe' -ArgumentList ('/select,"__PAYLOAD__"')
  }
}
"#;

/// 安装版：运行安装器并等它结束，再拉起应用。
///
/// 失败时写失败标记（下次启动通知用户）；成功时写成功标记（启动时据此清理 `.bak`）。
const INSTALLER_SCRIPT: &str = r#"$ErrorActionPreference = 'Continue'
$log = '__LOG__'
$okMarker = '__OK__'
$failMarker = '__FAIL__'
function Write-Log([string]$m) { try { Add-Content -LiteralPath $log -Value ((Get-Date).ToString('s') + ' ' + $m) -Encoding UTF8 } catch { } }
Write-Log 'installer update: waiting for pid __PID__ to exit'
$deadline = (Get-Date).AddSeconds(__WAIT__)
while ($true) {
  if (-not (Get-Process -Id __PID__ -ErrorAction SilentlyContinue)) { break }
  if ((Get-Date) -gt $deadline) { Write-Log 'installer update: wait timed out'; break }
  Start-Sleep -Milliseconds 300
}
Start-Sleep -Milliseconds 600
$ok = $false
try {
  $p = Start-Process -FilePath '__PAYLOAD__' -PassThru
  Write-Log ('installer update: installer started, pid ' + $p.Id)
  Wait-Process -Id $p.Id -ErrorAction SilentlyContinue
  Write-Log 'installer update: installer exited'
  $ok = $true
} catch {
  Write-Log ('installer update failed: ' + $_.Exception.Message)
  try { Set-Content -LiteralPath $failMarker -Value ('安装器运行失败：' + $_.Exception.Message + [Environment]::NewLine + '__TARGET__') -Encoding UTF8 } catch { }
}
if ($ok) { try { Set-Content -LiteralPath $okMarker -Value ((Get-Date).ToString('s')) -Encoding UTF8 } catch { } }
Start-Sleep -Milliseconds 800
try {
  Start-Process -FilePath '__TARGET__' -WorkingDirectory (Split-Path -Parent '__TARGET__')
  Write-Log 'installer update: relaunched'
} catch {
  Write-Log ('installer update: relaunch failed: ' + $_.Exception.Message)
}
"#;

/// 便携版落地脚本：等本进程退出 → 原子替换 exe → 拉起；失败则回滚并拉起原程序。
pub(super) fn portable_script(
    target: &Path,
    payload: &Path,
    staged: &Path,
    bak: &Path,
    log: &Path,
    ok_marker: &Path,
    fail_marker: &Path,
) -> String {
    render(
        PORTABLE_SCRIPT,
        &[
            ("__LOG__", &ps_quote(log)),
            ("__TARGET__", &ps_quote(target)),
            ("__BAK__", &ps_quote(bak)),
            ("__STAGED__", &ps_quote(staged)),
            ("__PAYLOAD__", &ps_quote(payload)),
            ("__OK__", &ps_quote(ok_marker)),
            ("__FAIL__", &ps_quote(fail_marker)),
        ],
    )
}

/// 安装版落地脚本：等本进程退出 → 跑安装器 → 拉起。
pub(super) fn installer_script(
    target: &Path,
    payload: &Path,
    log: &Path,
    ok_marker: &Path,
    fail_marker: &Path,
) -> String {
    render(
        INSTALLER_SCRIPT,
        &[
            ("__LOG__", &ps_quote(log)),
            ("__TARGET__", &ps_quote(target)),
            ("__PAYLOAD__", &ps_quote(payload)),
            ("__OK__", &ps_quote(ok_marker)),
            ("__FAIL__", &ps_quote(fail_marker)),
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
