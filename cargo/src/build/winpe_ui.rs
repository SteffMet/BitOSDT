use crate::core::errors::BitOSDTResult;
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const STATUS_FILE_WINPE_PATH: &str = "X:\\BitOSDT\\State\\deploy-status.json";
pub const LOG_FILE_WINPE_PATH: &str = "X:\\BitOSDT\\Logs\\deploy.log";
pub const SHELL_LOG_WINPE_PATH: &str = "X:\\BitOSDT\\Logs\\shell-launch.log";
pub const HTA_WINPE_PATH: &str = "X:\\BitOSDT\\UI\\BitOSDT-Deploy.hta";
pub const HTA_MODE_CONFIG_WINPE_PATH: &str = "X:\\BitOSDT\\Config\\winpe-hta-mode.txt";
pub const LAUNCHER_PS1_WINPE_PATH: &str = "X:\\BitOSDT\\Scripts\\Launch-BitOSDT-WinPE.ps1";
pub const LAUNCHER_CMD_WINPE_PATH: &str = "X:\\BitOSDT\\Scripts\\Launch-BitOSDT-WinPE.cmd";
pub const KIOSK_HELPER_WINPE_PATH: &str = "X:\\BitOSDT\\Scripts\\Apply-Kiosk.ps1";
pub const WINPE_COMPAT_SCRIPT_WINPE_PATH: &str =
    "X:\\BitOSDT\\Scripts\\Set-WinPE-CompatibilitySpoof.ps1";
pub const WINPE_COMPAT_ENABLE_FLAG_WINPE_PATH: &str =
    "X:\\BitOSDT\\Config\\enable-winpe-compat-spoof.flag";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinPEUiMode {
    FullIso,
    Lightweight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WinPEHtaMode {
    Basic,
    Js,
    Kiosk,
    Console,
}

impl WinPEHtaMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Js => "js",
            Self::Kiosk => "kiosk",
            Self::Console => "console",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "basic" => Some(Self::Basic),
            "js" => Some(Self::Js),
            "kiosk" => Some(Self::Kiosk),
            "console" => Some(Self::Console),
            _ => None,
        }
    }
}

impl WinPEUiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullIso => "full_iso",
            Self::Lightweight => "lightweight",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WinPEStatus {
    pub schema_version: u32,
    pub mode: String,
    pub stage_index: u32,
    pub stage_total: u32,
    pub percent_complete: u32,
    pub status_text: String,
    pub detail_text: String,
    pub last_updated_utc: String,
    pub is_error: bool,
    pub error_message: Option<String>,
}

impl WinPEStatus {
    pub fn initial(mode: WinPEUiMode) -> Self {
        Self {
            schema_version: 1,
            mode: mode.as_str().to_string(),
            stage_index: 1,
            stage_total: 4,
            percent_complete: 0,
            status_text: "Preparing deployment environment...".to_string(),
            detail_text: "WinPE shell initialised. Waiting for deployment engine.".to_string(),
            last_updated_utc: Utc::now().to_rfc3339(),
            is_error: false,
            error_message: None,
        }
    }

    pub fn to_json_pretty(&self) -> BitOSDTResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

pub fn generate_winpeshl_ini() -> String {
    "[LaunchApps]\r\nX:\\Windows\\System32\\cmd.exe,/k X:\\Windows\\System32\\startnet.cmd\r\n"
        .to_string()
}

fn js_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    })
}

fn resolve_winpe_hta_mode() -> WinPEHtaMode {
    std::env::var("BITOSDT_WINPE_HTA_MODE")
        .ok()
        .as_deref()
        .and_then(WinPEHtaMode::from_str)
        .unwrap_or(WinPEHtaMode::Kiosk)
}

pub fn generate_kiosk_helper_ps1() -> String {
    r#"# Apply-Kiosk.ps1 — Strip HTA window chrome for true fullscreen in WinPE
# Launched in background by shell wrapper after mshta.exe starts.

$ErrorActionPreference = 'SilentlyContinue'
$LogPath = "X:\BitOSDT\Logs\shell-launch.log"
$HtaPath = "X:\BitOSDT\UI\BitOSDT-Deploy.hta"
$WindowTitle = "BitOSDT Deployment"
$RequiredStablePasses = 3
$MaxAttempts = 120

function Write-KioskLog {
    param([string]$Message)
    try {
        $stamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
        $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
        [System.IO.File]::AppendAllText($LogPath, "$stamp [KIOSK] $Message`r`n", $utf8NoBom)
    } catch {}
}

Write-KioskLog "Kiosk helper started. Waiting for HTA window..."
Write-KioskLog "Tracking HTA path: $HtaPath"

try {
    Add-Type @"
    using System;
    using System.Runtime.InteropServices;
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
    public class KioskHelper {
        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
        [DllImport("user32.dll", SetLastError = true)]
        public static extern int GetWindowLong(IntPtr hWnd, int nIndex);
        [DllImport("user32.dll", SetLastError = true)]
        public static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);
        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
        [DllImport("user32.dll")]
        public static extern int GetSystemMetrics(int nIndex);
        [DllImport("user32.dll")]
        public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool IsWindow(IntPtr hWnd);
    }
"@
} catch {
    Write-KioskLog "Add-Type failed (csc.exe may not be available in this WinPE): $_"
    exit 0
}

$GWL_STYLE       = -16
$WS_CAPTION      = 0x00C00000
$WS_THICKFRAME   = 0x00040000
$SWP_FRAMECHANGED = 0x0020
$SWP_NOZORDER     = 0x0004
$SM_CXSCREEN = 0
$SM_CYSCREEN = 1
$SW_SHOWMAXIMIZED = 3
$script:HtaPathNormalized = $HtaPath.ToLowerInvariant()

function Resolve-HtaWindowHandle {
    param([string]$WindowTitle, [string]$HtaPathNormalized)

    try {
        $matches = Get-CimInstance Win32_Process -Filter "Name = 'mshta.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -and $_.CommandLine.ToLowerInvariant().Contains($HtaPathNormalized) } |
            Sort-Object CreationDate -Descending

        foreach ($match in $matches) {
            $candidate = Get-Process -Id ([int]$match.ProcessId) -ErrorAction SilentlyContinue
            if ($candidate -and $candidate.MainWindowHandle -and $candidate.MainWindowHandle -ne 0) {
                Write-KioskLog ("Resolved HTA window from command line; pid={0}; hwnd={1}" -f $candidate.Id, $candidate.MainWindowHandle)
                return [IntPtr]$candidate.MainWindowHandle
            }
        }
    } catch {
        Write-KioskLog "Command-line HTA detection failed: $($_.Exception.Message)"
    }

    $hwnd = [KioskHelper]::FindWindow($null, $WindowTitle)
    if ($hwnd -ne [IntPtr]::Zero) {
        Write-KioskLog ("Resolved HTA window by title; hwnd={0}" -f $hwnd)
        return $hwnd
    }

    try {
        $candidate = Get-Process -Name mshta -ErrorAction SilentlyContinue |
            Sort-Object StartTime -Descending |
            Select-Object -First 1
        if ($candidate -and $candidate.MainWindowHandle -and $candidate.MainWindowHandle -ne 0) {
            Write-KioskLog ("Resolved HTA window by fallback process scan; pid={0}; hwnd={1}" -f $candidate.Id, $candidate.MainWindowHandle)
            return [IntPtr]$candidate.MainWindowHandle
        }
    } catch {}

    return [IntPtr]::Zero
}

function Test-HtaFullscreen {
    param([IntPtr]$Hwnd)

    if ($Hwnd -eq [IntPtr]::Zero -or -not [KioskHelper]::IsWindow($Hwnd)) {
        return $false
    }

    $rect = New-Object RECT
    if (-not [KioskHelper]::GetWindowRect($Hwnd, [ref]$rect)) {
        Write-KioskLog ("GetWindowRect failed for hwnd={0}" -f $Hwnd)
        return $false
    }

    $screenW = [KioskHelper]::GetSystemMetrics($SM_CXSCREEN)
    $screenH = [KioskHelper]::GetSystemMetrics($SM_CYSCREEN)
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $coversScreen = ([Math]::Abs($rect.Left) -le 1) -and ([Math]::Abs($rect.Top) -le 1) -and $width -ge $screenW -and $height -ge $screenH

    if (-not $coversScreen) {
        Write-KioskLog ("Window bounds not fullscreen yet: left={0}; top={1}; width={2}; height={3}; target={4}x{5}" -f $rect.Left, $rect.Top, $width, $height, $screenW, $screenH)
    }

    return $coversScreen
}

$stablePasses = 0
$lastLoggedHandle = ""

for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    Start-Sleep -Milliseconds 500
    $hwnd = Resolve-HtaWindowHandle -WindowTitle $WindowTitle -HtaPathNormalized $script:HtaPathNormalized
    if ($hwnd -eq [IntPtr]::Zero) {
        $stablePasses = 0
        if (($attempt % 10) -eq 0) {
            Write-KioskLog "HTA window handle not found yet (attempt $attempt/$MaxAttempts)."
        }
        continue
    }

    if (-not [KioskHelper]::IsWindow($hwnd)) {
        Write-KioskLog ("Resolved HWND {0} is no longer valid. Retrying." -f $hwnd)
        $stablePasses = 0
        continue
    }

    if ($lastLoggedHandle -ne [string]$hwnd) {
        Write-KioskLog ("Applying kiosk enforcement to hwnd={0} on attempt {1}/{2}." -f $hwnd, $attempt, $MaxAttempts)
        $lastLoggedHandle = [string]$hwnd
    }

    $style = [KioskHelper]::GetWindowLong($hwnd, $GWL_STYLE)
    $newStyle = $style -band (-bnot ($WS_CAPTION -bor $WS_THICKFRAME))
    [KioskHelper]::SetWindowLong($hwnd, $GWL_STYLE, $newStyle) | Out-Null
    $screenW = [KioskHelper]::GetSystemMetrics($SM_CXSCREEN)
    $screenH = [KioskHelper]::GetSystemMetrics($SM_CYSCREEN)
    [KioskHelper]::SetWindowPos($hwnd, [IntPtr]::Zero, 0, 0, $screenW, $screenH, ($SWP_FRAMECHANGED -bor $SWP_NOZORDER)) | Out-Null
    [KioskHelper]::ShowWindow($hwnd, $SW_SHOWMAXIMIZED) | Out-Null
    Write-KioskLog ("Applied kiosk geometry attempt {0}; hwnd={1}; target={2}x{3}" -f $attempt, $hwnd, $screenW, $screenH)

    if (Test-HtaFullscreen -Hwnd $hwnd) {
        $stablePasses = $stablePasses + 1
        Write-KioskLog ("Fullscreen verification pass {0}/{1} for hwnd={2}" -f $stablePasses, $RequiredStablePasses, $hwnd)
        if ($stablePasses -ge $RequiredStablePasses) {
            Write-KioskLog ("HTA window verified at fullscreen after {0} attempts." -f $attempt)
            exit 0
        }
    } else {
        $stablePasses = 0
    }
}

Write-KioskLog ("HTA window did not settle into fullscreen after {0} attempts. Giving up." -f $MaxAttempts)
"#
    .replace('\n', "\r\n")
}

pub fn generate_shell_launcher_cmd(fallback_command: &str) -> String {
    let hta_mode = resolve_winpe_hta_mode().as_str();
    format!(
        r#"@echo off
setlocal EnableDelayedExpansion EnableExtensions

set ORCHESTRATOR={launcher_ps1}
set HTA={hta_path}
set HTA_MODE={hta_mode}
set HTA_MODE_FILE={hta_mode_file}
set KIOSK_HELPER={kiosk_helper_path}
set MSHTA_EXE=X:\Windows\System32\mshta.exe
set POWERSHELL_EXE=X:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe
set LOG_DIR=X:\BitOSDT\Logs
set SHELL_LOG=%LOG_DIR%\shell-launch.log
set HTA_STARTED=0
set FORCE_CONSOLE=0

if not exist "%LOG_DIR%" (
    mkdir "%LOG_DIR%" >nul 2>&1
)

call :log "Shell launcher starting."
call :log "HTA path: %HTA%"
call :log "Orchestrator path: %ORCHESTRATOR%"

if exist "%HTA_MODE_FILE%" (
    set /p HTA_MODE=<"%HTA_MODE_FILE%"
)
call :normalize_hta_mode
call :log "Resolved HTA mode: %HTA_MODE%"

if /I "%HTA_MODE%"=="console" (
    set FORCE_CONSOLE=1
    call :log "HTA mode is console. Skipping HTA launch."
    echo HTA launch disabled by configuration. Continuing in console mode.
) else (
    if exist "%HTA%" (
        call :log "HTA shell detected. Attempting launch."
        if exist "%MSHTA_EXE%" (
            start "" "%MSHTA_EXE%" "%HTA%"
        ) else (
            start "" mshta.exe "%HTA%"
        )
        set HTA_EXIT=!ERRORLEVEL!
        if "!HTA_EXIT!"=="0" (
            set HTA_STARTED=1
            call :log "HTA launch reported success."
            if /I "!HTA_MODE!"=="kiosk" (
                if exist "%KIOSK_HELPER%" (
                    if exist "%POWERSHELL_EXE%" (
                        start "" "%POWERSHELL_EXE%" -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%KIOSK_HELPER%"
                    ) else (
                        start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%KIOSK_HELPER%"
                    )
                    call :log "Kiosk helper launched."
                ) else (
                    call :log "Kiosk helper not found at %KIOSK_HELPER%. Skipping."
                )
            ) else (
                call :log "Skipping kiosk helper for HTA mode !HTA_MODE!."
            )
        ) else (
            set FORCE_CONSOLE=1
            call :log "HTA launch failed with exit code !HTA_EXIT!."
            echo Failed to launch HTA shell. Exit=!HTA_EXIT!. Keeping console fallback visible.
        )
    ) else (
        set FORCE_CONSOLE=1
        call :log "HTA shell missing. Falling back to console mode."
        echo HTA shell not found at "%HTA%". Continuing in console mode.
    )
)

if exist "%ORCHESTRATOR%" (
    call :log "Launching orchestrator script."
    if exist "%POWERSHELL_EXE%" (
        "%POWERSHELL_EXE%" -NoProfile -ExecutionPolicy Bypass -File "%ORCHESTRATOR%"
    ) else (
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%ORCHESTRATOR%"
    )
    set EXITCODE=!ERRORLEVEL!
    call :log "Orchestrator exited with code !EXITCODE!."
    if "!EXITCODE!"=="0" goto :after_launcher
    echo Launcher returned error !EXITCODE!. Executing fallback path...
) else (
    call :log "Orchestrator script missing. Executing fallback path."
    echo Launcher script missing at "%ORCHESTRATOR%". Executing fallback path...
)

call :log "Executing fallback path."
{fallback}
set FALLBACK_EXITCODE=!ERRORLEVEL!
call :log "Fallback path completed with code !FALLBACK_EXITCODE!."

:after_launcher
if "!FORCE_CONSOLE!"=="1" (
    call :log "Forcing interactive console because HTA shell was unavailable."
    echo BitOSDT is running without HTA shell. Console will remain open for diagnostics.
    cmd /k
)

:done
call :log "Shell launcher completed."
goto :eof

:normalize_hta_mode
if /I "%HTA_MODE%"=="basic" goto :eof
if /I "%HTA_MODE%"=="js" goto :eof
if /I "%HTA_MODE%"=="kiosk" goto :eof
if /I "%HTA_MODE%"=="console" goto :eof
call :log "Unknown HTA mode '%HTA_MODE%'. Falling back to kiosk."
set HTA_MODE=kiosk
goto :eof

:log
echo [%DATE% %TIME%] %~1>>"%SHELL_LOG%"
goto :eof
"#,
        launcher_ps1 = LAUNCHER_PS1_WINPE_PATH,
        hta_path = HTA_WINPE_PATH,
        hta_mode = hta_mode,
        hta_mode_file = HTA_MODE_CONFIG_WINPE_PATH,
        kiosk_helper_path = KIOSK_HELPER_WINPE_PATH,
        fallback = fallback_command
    )
    .replace('\n', "\r\n")
}

pub fn resolve_winpe_compat_spoof_enabled() -> bool {
    match std::env::var("BITOSDT_WINPE_COMPAT_SPOOF") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
        }
        Err(_) => true,
    }
}

pub fn generate_winpe_compat_spoof_script() -> String {
    r#"param(
    [ValidateSet('Apply', 'Revert')]
    [string]$Mode = 'Apply'
)

$ErrorActionPreference = 'Continue'
$targetKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$backupKey = 'HKLM:\SOFTWARE\BitOSDT\CompatBackup\CurrentVersion'

$compatValues = @{
    InstallationType = 'Client'
    ProductName = 'Windows 11 Pro'
    CurrentBuild = '26100'
    CurrentBuildNumber = '26100'
}

function Ensure-BackupState {
    if (-not (Test-Path $backupKey)) {
        New-Item -Path $backupKey -Force | Out-Null
    }
}

function Backup-OriginalValues {
    Ensure-BackupState
    foreach ($name in $compatValues.Keys) {
        $existing = Get-ItemProperty -Path $targetKey -Name $name -ErrorAction SilentlyContinue
        if ($null -ne $existing -and $null -ne $existing.$name) {
            Set-ItemProperty -Path $backupKey -Name $name -Value ([string]$existing.$name) -Force
            Set-ItemProperty -Path $backupKey -Name "${name}__Exists" -Value '1' -Force
        } else {
            Set-ItemProperty -Path $backupKey -Name "${name}__Exists" -Value '0' -Force
        }
    }
}

function Apply-CompatibilitySpoof {
    Backup-OriginalValues
    foreach ($name in $compatValues.Keys) {
        Set-ItemProperty -Path $targetKey -Name $name -Type String -Value $compatValues[$name] -Force
    }
}

function Revert-CompatibilitySpoof {
    if (-not (Test-Path $backupKey)) {
        return
    }

    foreach ($name in $compatValues.Keys) {
        $existsFlag = Get-ItemProperty -Path $backupKey -Name "${name}__Exists" -ErrorAction SilentlyContinue
        $wasPresent = ($existsFlag -and $existsFlag."${name}__Exists" -eq '1')

        if ($wasPresent) {
            $original = Get-ItemProperty -Path $backupKey -Name $name -ErrorAction SilentlyContinue
            if ($original -and $null -ne $original.$name) {
                Set-ItemProperty -Path $targetKey -Name $name -Type String -Value ([string]$original.$name) -Force
            }
        } else {
            Remove-ItemProperty -Path $targetKey -Name $name -ErrorAction SilentlyContinue
        }
    }
}

if ($Mode -eq 'Apply') {
    Apply-CompatibilitySpoof
} else {
    Revert-CompatibilitySpoof
}
"#
    .replace('\n', "\r\n")
}

pub fn write_winpe_compat_spoof_assets(mount_dir: &Path, enabled: bool) -> BitOSDTResult<()> {
    let scripts_dir = mount_dir.join("BitOSDT").join("Scripts");
    let config_dir = mount_dir.join("BitOSDT").join("Config");
    fs::create_dir_all(&scripts_dir)?;
    fs::create_dir_all(&config_dir)?;

    fs::write(
        scripts_dir.join("Set-WinPE-CompatibilitySpoof.ps1"),
        generate_winpe_compat_spoof_script(),
    )?;
    fs::write(
        scripts_dir.join("Revert-WinPE-CompatibilitySpoof.cmd"),
        "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"X:\\BitOSDT\\Scripts\\Set-WinPE-CompatibilitySpoof.ps1\" -Mode Revert\r\n",
    )?;

    let enable_flag_path = config_dir.join("enable-winpe-compat-spoof.flag");
    if enabled {
        fs::write(
            &enable_flag_path,
            "Enable WinPE compatibility spoof on boot.\r\n",
        )?;
    } else if enable_flag_path.exists() {
        fs::remove_file(enable_flag_path)?;
    }

    Ok(())
}

pub fn generate_deploy_hta() -> String {
    let default_hta_mode = js_string_literal(resolve_winpe_hta_mode().as_str());
    let hta_mode_path = js_string_literal(HTA_MODE_CONFIG_WINPE_PATH);
    let status_path = js_string_literal(STATUS_FILE_WINPE_PATH);
    let log_path = js_string_literal(LOG_FILE_WINPE_PATH);
    let shell_log_path = js_string_literal(SHELL_LOG_WINPE_PATH);

    format!(
        r##"<html>
<head>
<meta http-equiv="X-UA-Compatible" content="IE=Edge">
<title>BitOSDT Deployment</title>
<HTA:APPLICATION
  APPLICATIONNAME="BitOSDT"
  BORDER="none"
  CAPTION="no"
  SHOWINTASKBAR="no"
  SINGLEINSTANCE="yes"
  SCROLL="no"
  SYSMENU="no"
  MAXIMIZEBUTTON="no"
  MINIMIZEBUTTON="no"
  WINDOWSTATE="normal"
>
</HTA:APPLICATION>
<style>
html, body {{
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  width: 100%;
  height: 100%;
  margin: 0;
  padding: 0;
  overflow: hidden;
  font-family: Segoe UI, Tahoma, sans-serif;
  color: #eef3ff;
  background: linear-gradient(135deg, #0a1b48 0%, #15337f 40%, #2a5ec7 70%, #1a3f8a 100%);
}}

@keyframes pulseGlow {{
  0%   {{ box-shadow: 0 0 12px rgba(70, 215, 255, 0.25); }}
  50%  {{ box-shadow: 0 0 28px rgba(70, 215, 255, 0.55); }}
  100% {{ box-shadow: 0 0 12px rgba(70, 215, 255, 0.25); }}
}}

@keyframes shimmer {{
  0%   {{ background-position: -200% 0; }}
  100% {{ background-position: 200% 0; }}
}}

@keyframes dotPulse {{
  0%   {{ opacity: 0.3; }}
  50%  {{ opacity: 1; }}
  100% {{ opacity: 0.3; }}
}}

#shell {{
  width: 100%;
  height: 100%;
  min-height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  box-sizing: border-box;
  padding: 1.8vh 0 0.35vh 0;
  overflow: hidden;
}}

#header {{
  font-size: 52px;
  font-weight: 700;
  letter-spacing: 2px;
  margin-bottom: 2px;
  text-shadow: 0 2px 12px rgba(70, 215, 255, 0.25);
}}

#subheader {{
  font-size: 24px;
  font-weight: 300;
  opacity: 0.85;
  margin-bottom: 12px;
  letter-spacing: 1px;
}}

#ringWrap {{
  position: relative;
  width: 200px;
  height: 200px;
  margin-bottom: 10px;
  border-radius: 50%;
}}

#ringWrap.waiting {{
  animation: pulseGlow 2s ease-in-out infinite;
}}

#progressRing {{
  width: 200px;
  height: 200px;
}}

#percent {{
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  font-size: 52px;
  font-weight: 700;
  text-shadow: 0 1px 8px rgba(0,0,0,0.3);
}}

#statusText {{
  font-size: 26px;
  font-weight: 600;
  margin-bottom: 4px;
  text-align: center;
  padding: 0 20px;
}}

#detailText {{
  font-size: 18px;
  opacity: 0.9;
  margin-bottom: 12px;
  text-align: center;
  padding: 0 20px;
}}

.dotAnim span {{
  animation: dotPulse 1.4s ease-in-out infinite;
}}
.dotAnim span:nth-child(2) {{ animation-delay: 0.2s; }}
.dotAnim span:nth-child(3) {{ animation-delay: 0.4s; }}

#progressOuter {{
  width: 75%;
  max-width: 1100px;
  height: 22px;
  border-radius: 20px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: rgba(10, 24, 70, 0.55);
  overflow: hidden;
  position: relative;
}}

#progressFill {{
  width: 0%;
  height: 100%;
  border-radius: 20px;
  background: linear-gradient(90deg, #29c2ff 0%, #46d7ff 40%, #52f0c9 100%);
  transition: width 0.4s ease;
  position: relative;
}}

#progressFill.active {{
  background: linear-gradient(90deg, #29c2ff 0%, #46d7ff 20%, #52f0c9 40%, #46d7ff 60%, #29c2ff 80%, #46d7ff 100%);
  background-size: 200% 100%;
  animation: shimmer 2.5s linear infinite;
}}

#stepText {{
  margin-top: 8px;
  font-size: 18px;
  opacity: 0.9;
}}

#logPanel {{
  margin-top: 10px;
  width: 80%;
  max-width: 1200px;
  flex: 1;
  min-height: 120px;
  background: rgba(4, 10, 32, 0.8);
  border: 1px solid rgba(120, 178, 255, 0.3);
  border-radius: 8px;
  padding: 10px 14px;
  box-sizing: border-box;
  overflow-y: auto;
  overflow-x: hidden;
}}

#logLabel {{
  font-size: 13px;
  font-weight: 600;
  color: rgba(200, 220, 255, 0.6);
  text-transform: uppercase;
  letter-spacing: 1px;
  margin-bottom: 6px;
}}

#logLines {{
  margin: 0;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 15px;
  line-height: 1.55;
  color: #c9d8ff;
  white-space: pre-wrap;
  word-wrap: break-word;
}}

#footer {{
  width: 80%;
  max-width: 1200px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 14px;
  opacity: 0.8;
  padding: 6px 0 0 0;
  margin-top: auto;
  flex-shrink: 0;
}}

#errorBanner {{
  display: none;
  margin-top: 8px;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid rgba(255, 124, 124, 0.75);
  background: rgba(90, 12, 12, 0.65);
  color: #ffd6d6;
  font-size: 16px;
  width: 80%;
  max-width: 1200px;
  box-sizing: border-box;
}}

#staleBadge {{
  margin-top: 6px;
  font-size: 13px;
  color: #ffe38f;
  display: none;
}}

@media screen and (max-height: 900px) {{
  #shell {{
    padding: 1.4vh 0 0.3vh 0;
  }}

  #header {{
    font-size: 44px;
  }}

  #subheader {{
    font-size: 20px;
    margin-bottom: 10px;
  }}

  #ringWrap {{
    width: 170px;
    height: 170px;
    margin-bottom: 8px;
  }}

  #progressRing {{
    width: 170px;
    height: 170px;
  }}

  #percent {{
    font-size: 44px;
  }}

  #statusText {{
    font-size: 22px;
  }}

  #detailText {{
    font-size: 16px;
    margin-bottom: 10px;
  }}

  #progressOuter {{
    height: 18px;
  }}

  #stepText {{
    font-size: 16px;
    margin-top: 7px;
  }}

  #logPanel {{
    min-height: 100px;
  }}
}}

@media screen and (max-height: 768px) {{
  #shell {{
    padding: 1vh 0 0.2vh 0;
  }}

  #header {{
    font-size: 38px;
  }}

  #subheader {{
    font-size: 18px;
    margin-bottom: 8px;
  }}

  #ringWrap {{
    width: 145px;
    height: 145px;
    margin-bottom: 6px;
  }}

  #progressRing {{
    width: 145px;
    height: 145px;
  }}

  #percent {{
    font-size: 38px;
  }}

  #statusText {{
    font-size: 20px;
    margin-bottom: 3px;
  }}

  #detailText {{
    font-size: 15px;
    margin-bottom: 8px;
  }}

  #progressOuter {{
    height: 16px;
  }}

  #stepText {{
    font-size: 15px;
    margin-top: 6px;
  }}

  #logPanel {{
    margin-top: 8px;
    min-height: 80px;
  }}
}}

@media screen and (max-height: 680px) {{
  #shell {{
    padding: 0.7vh 0 0.1vh 0;
  }}

  #header {{
    font-size: 30px;
  }}

  #subheader {{
    font-size: 15px;
    margin-bottom: 6px;
  }}

  #ringWrap {{
    width: 128px;
    height: 128px;
    margin-bottom: 4px;
  }}

  #progressRing {{
    width: 128px;
    height: 128px;
  }}

  #percent {{
    font-size: 32px;
  }}

  #statusText {{
    font-size: 17px;
    margin-bottom: 2px;
  }}

  #detailText {{
    font-size: 13px;
    margin-bottom: 6px;
  }}

  #progressOuter {{
    height: 14px;
  }}

  #stepText {{
    font-size: 13px;
    margin-top: 5px;
  }}

  #logPanel {{
    margin-top: 6px;
    min-height: 60px;
  }}
}}
</style>
</head>
<body>
<div id="shell">
  <div id="header">BitOSDT</div>
  <div id="subheader">Windows Deployment</div>

  <div id="ringWrap" class="waiting">
    <canvas id="progressRing" width="200" height="200"></canvas>
    <div id="percent">0%</div>
  </div>

  <div id="statusText">Preparing deployment environment...</div>
  <div id="detailText">Connecting to deployment engine<span class="dotAnim"><span>.</span><span>.</span><span>.</span></span></div>

  <div id="progressOuter"><div id="progressFill"></div></div>
  <div id="stepText">Step 1 of 4</div>
  <div id="staleBadge">Status file not updated recently. Deployment may still be running.</div>

  <div id="errorBanner"></div>

  <div id="logPanel">
    <div id="logLabel">Console Output</div>
    <pre id="logLines">Awaiting deployment engine<span class="dotAnim"><span>.</span><span>.</span><span>.</span></span></pre>
  </div>

  <div id="footer">
    <div id="modeText">Mode: initialising</div>
  </div>
</div>

<script type="text/javascript">
(function() {{
  var defaultHtaMode = {default_hta_mode};
  var htaModePath = {hta_mode_path};
  var statusPath = {status_path};
  var logPath = {log_path};
  var shellLogPath = {shell_log_path};
  var htaMode = defaultHtaMode;
  var statusPollMs = 600;
  var logPollMs = 1200;
  var kioskEnforceMs = 2000;
  var staleAfterMs = 15000;
  var maxRenderedLogChars = 160000;
  var maxKioskApplyAttempts = 0;
  var lastRenderedLog = "";
  var lastStatusPayload = "";
  var lastStatusUpdatedAtMs = NaN;
  var hasReceivedStatus = false;
  var hasReceivedLog = false;
  var learnedChromeOffsetX = 0;
  var learnedChromeOffsetY = 0;
  var baselineOverscanX = 0;
  var baselineOverscanY = 0;
  var maxLearnableChromeOffsetX = 120;
  var maxLearnableChromeOffsetY = 120;
  var kioskApplyAttempts = 0;
  var kioskStablePasses = 0;
  var kioskTimer = null;

  function toPositiveInt(value) {{
    var n = parseInt(value, 10);
    if (isNaN(n) || n <= 0) {{
      return 0;
    }}
    return n;
  }}

  function toSignedInt(value) {{
    var n = parseInt(value, 10);
    if (isNaN(n)) {{
      return 0;
    }}
    return n;
  }}

  function getViewportSize() {{
    var docEl = document.documentElement;
    var body = document.body;
    var width = 0;
    var height = 0;

    if (docEl) {{
      width = toPositiveInt(docEl.clientWidth);
      height = toPositiveInt(docEl.clientHeight);
    }}

    if ((!width || !height) && body) {{
      if (!width) {{
        width = toPositiveInt(body.clientWidth);
      }}
      if (!height) {{
        height = toPositiveInt(body.clientHeight);
      }}
    }}

    return {{
      width: width,
      height: height
    }};
  }}

  function getScreenTargetSize() {{
    var width = 0;
    var height = 0;

    try {{
      width = Math.max(toPositiveInt(screen.width), toPositiveInt(screen.availWidth));
      height = Math.max(toPositiveInt(screen.height), toPositiveInt(screen.availHeight));
    }} catch (e) {{}}

    if (!width || !height) {{
      var viewport = getViewportSize();
      if (!width) {{
        width = viewport.width;
      }}
      if (!height) {{
        height = viewport.height;
      }}
    }}

    return {{
      width: width,
      height: height
    }};
  }}

  function getScreenOrigin() {{
    var x = 0;
    var y = 0;

    try {{
      if (typeof screen.availLeft === "number" && !isNaN(screen.availLeft)) {{
        x = toSignedInt(screen.availLeft);
      }}
      if (typeof screen.availTop === "number" && !isNaN(screen.availTop)) {{
        y = toSignedInt(screen.availTop);
      }}
    }} catch (e) {{}}

    return {{
      x: x,
      y: y
    }};
  }}

  function getWindowFrame() {{
    var x = 0;
    var y = 0;
    var hasPosition = false;
    var width = 0;
    var height = 0;

    if (typeof window.screenLeft === "number" && !isNaN(window.screenLeft)) {{
      x = parseInt(window.screenLeft, 10);
      hasPosition = true;
    }} else if (typeof window.screenX === "number" && !isNaN(window.screenX)) {{
      x = parseInt(window.screenX, 10);
      hasPosition = true;
    }}

    if (typeof window.screenTop === "number" && !isNaN(window.screenTop)) {{
      y = parseInt(window.screenTop, 10);
      hasPosition = true;
    }} else if (typeof window.screenY === "number" && !isNaN(window.screenY)) {{
      y = parseInt(window.screenY, 10);
      hasPosition = true;
    }}

    width = toPositiveInt(window.outerWidth);
    height = toPositiveInt(window.outerHeight);

    return {{
      x: x,
      y: y,
      width: width,
      height: height,
      hasPosition: hasPosition,
      hasSize: width > 0 && height > 0
    }};
  }}

  function applyKioskGeometry() {{
    try {{
      // The kiosk helper strips chrome via Win32 API.
      // This fallback adapts to different HTA shell/client area behaviors.
      var target = getScreenTargetSize();
      if (!target.width || !target.height) {{
        return false;
      }}

      var viewport = getViewportSize();
      var missingW = target.width - viewport.width;
      var missingH = target.height - viewport.height;
      var viewportFilled = viewport.width >= target.width && viewport.height >= target.height;
      var origin = getScreenOrigin();
      var frame = getWindowFrame();
      var atNaturalPosition = !frame.hasPosition ||
        (Math.abs(frame.x - origin.x) <= 1 && Math.abs(frame.y - origin.y) <= 1);
      var atNaturalSize = !frame.hasSize ||
        (Math.abs(frame.width - target.width) <= 1 && Math.abs(frame.height - target.height) <= 1);

      // If the helper already has us at true fullscreen, discard stale learned offsets
      // from earlier passes so we do not keep forcing negative coordinates.
      if (viewportFilled && atNaturalPosition && atNaturalSize) {{
        learnedChromeOffsetX = 0;
        learnedChromeOffsetY = 0;
      }}

      if (missingW > 0 && missingW <= maxLearnableChromeOffsetX) {{
        learnedChromeOffsetX = Math.max(learnedChromeOffsetX, missingW);
      }}
      if (missingH > 0 && missingH <= maxLearnableChromeOffsetY) {{
        learnedChromeOffsetY = Math.max(learnedChromeOffsetY, missingH);
      }}

      var effectiveOverscanX = baselineOverscanX + learnedChromeOffsetX;
      var effectiveOverscanY = baselineOverscanY + learnedChromeOffsetY;
      var desiredX = origin.x - Math.floor(effectiveOverscanX / 2);
      var desiredY = origin.y - effectiveOverscanY;
      var desiredW = target.width + effectiveOverscanX;
      var desiredH = target.height + effectiveOverscanY;

      var positionMatch = !frame.hasPosition ||
        (Math.abs(frame.x - desiredX) <= 1 && Math.abs(frame.y - desiredY) <= 1);
      var sizeMatch = !frame.hasSize ||
        (Math.abs(frame.width - desiredW) <= 1 && Math.abs(frame.height - desiredH) <= 1);
      var needsApply = !viewportFilled || !positionMatch || !sizeMatch;

      if (needsApply) {{
        kioskStablePasses = 0;
        kioskApplyAttempts = kioskApplyAttempts + 1;
        window.moveTo(desiredX, desiredY);
        window.resizeTo(desiredW, desiredH);
        return false;
      }}
      kioskStablePasses = kioskStablePasses + 1;
      return true;
    }} catch (e) {{
      return false;
    }}
  }}

  function enforceKioskGeometry() {{
    var stable = applyKioskGeometry();

    if (stable) {{
      if (kioskStablePasses >= 2 && kioskTimer !== null) {{
        window.clearInterval(kioskTimer);
        kioskTimer = null;
      }}
      return;
    }}

    if (maxKioskApplyAttempts > 0 && kioskApplyAttempts >= maxKioskApplyAttempts && kioskTimer !== null) {{
      window.clearInterval(kioskTimer);
      kioskTimer = null;
    }}
  }}

  function readFileText(path) {{
    try {{
      var fso = new ActiveXObject("Scripting.FileSystemObject");
      if (!fso.FileExists(path)) {{
        return null;
      }}
      var stream = fso.OpenTextFile(path, 1, false);
      var text = stream.ReadAll();
      stream.Close();
      return text;
    }} catch (e) {{
      return null;
    }}
  }}

  function sanitizeLogText(text) {{
    if (text === null || typeof text === "undefined") {{
      return "";
    }}
    var clean = String(text);
    clean = clean.replace(/^\u00EF\u00BB\u00BF/, "");
    clean = clean.replace(/\u00EF\u00BB\u00BF/g, "");
    clean = clean.replace(/^\uFEFF/, "");
    clean = clean.replace(/\u0000/g, "");
    clean = clean.replace(/\r\n/g, "\n");
    clean = clean.replace(/\r/g, "\n");
    return clean;
  }}

  function normalizeHtaMode(value) {{
    var normalized = sanitizeLogText(value).replace(/^\s+|\s+$/g, "").toLowerCase();
    if (normalized === "basic" || normalized === "js" || normalized === "kiosk" || normalized === "console") {{
      return normalized;
    }}
    return "kiosk";
  }}

  function readHtaMode() {{
    var fileValue = readFileText(htaModePath);
    if (fileValue !== null) {{
      return normalizeHtaMode(fileValue);
    }}
    return normalizeHtaMode(defaultHtaMode);
  }}

  function parseJson(text) {{
    if (!text) {{ return null; }}
    if (typeof text === "string") {{
      text = text.replace(/^\u00EF\u00BB\u00BF/, "");
      text = text.replace(/\u00EF\u00BB\u00BF/g, "");
      text = text.replace(/^\uFEFF/, "");
      text = text.replace(/\u0000/g, "");
      text = text.replace(/^\s+|\s+$/g, "");
      if (!text) {{ return null; }}
    }}
    try {{
      return JSON.parse(text);
    }} catch (e) {{
      try {{
        return eval('(' + text + ')');
      }} catch (ignore) {{
        return null;
      }}
    }}
  }}

  function drawRing(percent) {{
    var canvas = document.getElementById("progressRing");
    var ctx = canvas.getContext("2d");
    var w = canvas.width;
    var h = canvas.height;
    var cx = w / 2;
    var cy = h / 2;
    var radius = 80;

    ctx.clearRect(0, 0, w, h);

    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, Math.PI * 2, false);
    ctx.lineWidth = 14;
    ctx.strokeStyle = "rgba(255,255,255,0.12)";
    ctx.stroke();

    if (percent > 0) {{
      var start = -Math.PI / 2;
      var end = start + (Math.PI * 2 * (percent / 100));
      ctx.beginPath();
      ctx.arc(cx, cy, radius, start, end, false);
      ctx.lineWidth = 14;
      ctx.strokeStyle = "#46d7ff";
      ctx.lineCap = "round";
      ctx.stroke();
    }}
  }}

  function updateStatusView(status) {{
    if (!status) {{
      return;
    }}

    hasReceivedStatus = true;
    var ringWrap = document.getElementById("ringWrap");
    ringWrap.className = "";

    var pct = Math.max(0, Math.min(100, parseInt(status.percent_complete || 0, 10)));
    var stageIndex = parseInt(status.stage_index || 1, 10);
    var stageTotal = parseInt(status.stage_total || 4, 10);
    var statusText = status.status_text || "Running deployment...";
    var detailText = status.detail_text || "";

    document.getElementById("percent").innerText = pct + "%";
    document.getElementById("statusText").innerText = statusText;
    document.getElementById("detailText").innerHTML = detailText;
    document.getElementById("stepText").innerText = "Step " + stageIndex + " of " + stageTotal;
    document.getElementById("modeText").innerText = "Mode: " + (status.mode || "unknown");

    var fill = document.getElementById("progressFill");
    fill.style.width = pct + "%";
    fill.className = (pct > 0 && pct < 100) ? "active" : "";

    drawRing(pct);

    if (pct === 0) {{
      ringWrap.className = "waiting";
    }}

    try {{
      lastStatusUpdatedAtMs = Date.parse(status.last_updated_utc);
    }} catch (e) {{
      lastStatusUpdatedAtMs = NaN;
    }}
    refreshStaleBadge();

    var errorBanner = document.getElementById("errorBanner");
    if (status.is_error) {{
      var msg = status.error_message || "Deployment reported an error. Automatic fallback may be in progress.";
      errorBanner.style.display = "block";
      errorBanner.innerText = "Error: " + msg;
    }} else {{
      errorBanner.style.display = "none";
      errorBanner.innerText = "";
    }}
  }}

  function refreshStaleBadge() {{
    if (!hasReceivedStatus) {{
      return;
    }}

    var stale = true;
    if (!isNaN(lastStatusUpdatedAtMs)) {{
      stale = ((new Date()).getTime() - lastStatusUpdatedAtMs > staleAfterMs);
    }}
    document.getElementById("staleBadge").style.display = stale ? "block" : "none";
  }}

  function trimLogForDisplay(text) {{
    if (!text || text.length <= maxRenderedLogChars) {{
      return text;
    }}
    return "[output truncated to latest logs]\n" + text.substring(text.length - maxRenderedLogChars);
  }}

  function updateLogView() {{
    var text = sanitizeLogText(readFileText(logPath));

    if (!text || !text.trim()) {{
      text = sanitizeLogText(readFileText(shellLogPath));
    }}

    if (text && text.length > 0) {{
      hasReceivedLog = true;
      text = trimLogForDisplay(text);
      if (text !== lastRenderedLog) {{
        var el = document.getElementById("logLines");
        el.innerText = text;
        lastRenderedLog = text;
        var panel = document.getElementById("logPanel");
        if (panel) {{
          panel.scrollTop = panel.scrollHeight;
        }}
      }}
    }}
  }}

  function tickStatus() {{
    var statusText = sanitizeLogText(readFileText(statusPath));
    if (!statusText || !statusText.trim()) {{
      refreshStaleBadge();
      return;
    }}

    if (statusText === lastStatusPayload) {{
      refreshStaleBadge();
      return;
    }}

    lastStatusPayload = statusText;
    var status = parseJson(statusText);
    updateStatusView(status);
  }}

  function tickLog() {{
    updateLogView();
  }}

  htaMode = readHtaMode();
  if (htaMode !== "basic") {{
    applyKioskGeometry();
    window.onresize = function() {{
      kioskStablePasses = 0;
      applyKioskGeometry();
    }};
    kioskTimer = window.setInterval(enforceKioskGeometry, kioskEnforceMs);
  }} else {{
    window.onresize = null;
  }}
  drawRing(0);
  tickStatus();
  tickLog();
  window.setInterval(tickStatus, statusPollMs);
  window.setInterval(tickLog, logPollMs);
}})();
</script>
</body>
</html>
"##,
        default_hta_mode = default_hta_mode,
        hta_mode_path = hta_mode_path,
        status_path = status_path,
        log_path = log_path,
        shell_log_path = shell_log_path,
    )
    .replace('\n', "\r\n")
}

pub fn write_winpeshl_ini(mount_dir: &Path) -> BitOSDTResult<PathBuf> {
    let path = mount_dir
        .join("Windows")
        .join("System32")
        .join("winpeshl.ini");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, generate_winpeshl_ini())?;
    Ok(path)
}

pub fn write_hta_shell(mount_dir: &Path) -> BitOSDTResult<PathBuf> {
    let path = mount_dir
        .join("BitOSDT")
        .join("UI")
        .join("BitOSDT-Deploy.hta");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, generate_deploy_hta())?;
    Ok(path)
}

pub fn write_hta_mode_config(mount_dir: &Path) -> BitOSDTResult<PathBuf> {
    let path = mount_dir
        .join("BitOSDT")
        .join("Config")
        .join("winpe-hta-mode.txt");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, format!("{}\r\n", resolve_winpe_hta_mode().as_str()))?;
    Ok(path)
}

pub fn write_shell_launcher_cmd(
    mount_dir: &Path,
    fallback_command: &str,
) -> BitOSDTResult<PathBuf> {
    let path = mount_dir
        .join("BitOSDT")
        .join("Scripts")
        .join("Launch-BitOSDT-WinPE.cmd");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, generate_shell_launcher_cmd(fallback_command))?;
    Ok(path)
}

pub fn write_initial_status(mount_dir: &Path, mode: WinPEUiMode) -> BitOSDTResult<PathBuf> {
    let path = mount_dir
        .join("BitOSDT")
        .join("State")
        .join("deploy-status.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = WinPEStatus::initial(mode).to_json_pretty()?;
    fs::write(&path, payload)?;
    Ok(path)
}

pub fn write_kiosk_helper(mount_dir: &Path) -> BitOSDTResult<PathBuf> {
    let path = mount_dir
        .join("BitOSDT")
        .join("Scripts")
        .join("Apply-Kiosk.ps1");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, generate_kiosk_helper_ps1())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn winpeshl_ini_launches_wrapper() {
        let ini = generate_winpeshl_ini();
        assert!(ini.contains("[LaunchApps]"));
        assert!(ini.contains("X:\\Windows\\System32\\cmd.exe"));
        assert!(ini.contains("startnet.cmd"));
        assert!(!ini.contains("Launch-BitOSDT-WinPE.cmd"));
        assert!(ini.contains("/k"));
    }

    #[test]
    fn hta_template_has_progress_and_polling_hooks() {
        let html = generate_deploy_hta();
        let expected_mode = format!(
            "var defaultHtaMode = \"{}\";",
            resolve_winpe_hta_mode().as_str()
        );
        assert!(html.contains("id=\"progressRing\""));
        assert!(html.contains("id=\"statusText\""));
        assert!(html.contains("window.setInterval(tickStatus, statusPollMs);"));
        assert!(html.contains("window.setInterval(tickLog, logPollMs);"));
        assert!(html.contains(&expected_mode));
        assert!(
            html.contains("var htaModePath = \"X:\\\\BitOSDT\\\\Config\\\\winpe-hta-mode.txt\";")
        );
        assert!(html.contains("text.replace(/^\\uFEFF/, \"\")"));
        assert!(html.contains("text.replace(/^\\u00EF\\u00BB\\u00BF/, \"\")"));
        assert!(html.contains("text.replace(/\\u0000/g, \"\")"));
        assert!(html.contains("var statusPath = \"X:\\\\BitOSDT\\\\State\\\\deploy-status.json\";"));
        assert!(html.contains("var logPath = \"X:\\\\BitOSDT\\\\Logs\\\\deploy.log\";"));
    }

    #[test]
    fn hta_template_escapes_windows_paths_for_javascript_literals() {
        let html = generate_deploy_hta();
        assert!(html.contains("var statusPath = \"X:\\\\BitOSDT\\\\State\\\\deploy-status.json\";"));
        assert!(html.contains("var logPath = \"X:\\\\BitOSDT\\\\Logs\\\\deploy.log\";"));
        assert!(html.contains("var shellLogPath = \"X:\\\\BitOSDT\\\\Logs\\\\shell-launch.log\";"));
        assert!(!html.contains("var statusPath = \"X:\\BitOSDT\\State\\deploy-status.json\";"));
    }

    #[test]
    fn hta_template_streams_full_session_log_content() {
        let html = generate_deploy_hta();
        assert!(html.contains("function sanitizeLogText(text)"));
        assert!(html.contains("text = sanitizeLogText(readFileText(logPath));"));
        assert!(html.contains("text = sanitizeLogText(readFileText(shellLogPath));"));
        assert!(html.contains("el.innerText = text;"));
        assert!(!html.contains("maxLogLines"));
        assert!(!html.contains("function tailLines(text)"));
    }

    #[test]
    fn initial_status_schema_is_valid() {
        let status = WinPEStatus::initial(WinPEUiMode::FullIso);
        let json = status.to_json_pretty().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["mode"], "full_iso");
        assert_eq!(parsed["stage_total"], 4);
        assert_eq!(parsed["percent_complete"], 0);
    }

    #[test]
    fn shell_wrapper_contains_fallback_command() {
        let cmd = generate_shell_launcher_cmd("powershell.exe -File X:\\foo.ps1");
        let expected_mode = format!("set HTA_MODE={}", resolve_winpe_hta_mode().as_str());
        assert!(cmd.contains("Launch-BitOSDT-WinPE.ps1"));
        assert!(cmd.contains("X:\\Windows\\System32\\mshta.exe"));
        assert!(cmd.contains("X:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"));
        assert!(cmd.contains(&expected_mode));
        assert!(cmd.contains("set HTA_MODE_FILE=X:\\BitOSDT\\Config\\winpe-hta-mode.txt"));
        assert!(cmd.contains("set SHELL_LOG=%LOG_DIR%\\shell-launch.log"));
        assert!(cmd.contains("set FORCE_CONSOLE=0"));
        assert!(cmd.contains("Forcing interactive console because HTA shell was unavailable."));
        assert!(cmd.contains("cmd /k"));
        assert!(cmd.contains("mshta.exe"));
        assert!(!cmd.contains("/min mshta.exe"));
        assert!(cmd.contains("powershell.exe -File X:\\foo.ps1"));
        assert!(cmd.contains("Shell launcher starting."));
        assert!(cmd.contains("Shell launcher completed."));
        // Kiosk helper integration
        assert!(cmd.contains("set KIOSK_HELPER="));
        assert!(cmd.contains("Apply-Kiosk.ps1"));
        assert!(cmd.contains("Kiosk helper launched."));
        assert!(cmd.contains("Skipping kiosk helper for HTA mode !HTA_MODE!."));
    }

    #[test]
    fn shell_wrapper_uses_delayed_expansion() {
        let cmd = generate_shell_launcher_cmd("powershell.exe -File X:\\foo.ps1");
        assert!(cmd.contains("setlocal EnableDelayedExpansion"));
        assert!(cmd.contains("!EXITCODE!"));
        assert!(!cmd.contains("%EXITCODE%"));
    }

    #[test]
    fn shell_wrapper_avoids_unexpected_at_this_time_parse_pattern() {
        let cmd = generate_shell_launcher_cmd("powershell.exe -File X:\\foo.ps1");
        assert!(!cmd.contains(
            "echo Failed to launch HTA shell (exit=%HTA_EXIT%). Keeping console fallback visible."
        ));
        assert!(cmd.contains(
            "echo Failed to launch HTA shell. Exit=!HTA_EXIT!. Keeping console fallback visible."
        ));
    }

    #[test]
    fn compat_spoof_script_contains_requested_registry_values() {
        let script = generate_winpe_compat_spoof_script();
        assert!(script.contains("InstallationType = 'Client'"));
        assert!(script.contains("ProductName = 'Windows 11 Pro'"));
        assert!(script.contains("CurrentBuild = '26100'"));
        assert!(script.contains("CurrentBuildNumber = '26100'"));
        assert!(script.contains("ValidateSet('Apply', 'Revert')"));
    }

    #[test]
    fn write_compat_spoof_assets_writes_script_and_enable_flag() {
        let temp = tempdir().expect("temp dir");
        write_winpe_compat_spoof_assets(temp.path(), true).expect("write compat assets");

        assert!(temp
            .path()
            .join("BitOSDT")
            .join("Scripts")
            .join("Set-WinPE-CompatibilitySpoof.ps1")
            .exists());
        assert!(temp
            .path()
            .join("BitOSDT")
            .join("Scripts")
            .join("Revert-WinPE-CompatibilitySpoof.cmd")
            .exists());
        assert!(temp
            .path()
            .join("BitOSDT")
            .join("Config")
            .join("enable-winpe-compat-spoof.flag")
            .exists());
    }

    #[test]
    fn hta_has_locked_kiosk_mode_and_responsive_fixed_log_panel() {
        let html = generate_deploy_hta();
        assert!(html.contains("CAPTION=\"no\""));
        assert!(html.contains("SYSMENU=\"no\""));
        assert!(html.contains("BORDER=\"none\""));
        assert!(html.contains("SHOWINTASKBAR=\"no\""));
        assert!(html.contains("MAXIMIZEBUTTON=\"no\""));
        assert!(html.contains("MINIMIZEBUTTON=\"no\""));
        // WINDOWSTATE changed to normal so moveTo/resizeTo aren't blocked
        assert!(html.contains("WINDOWSTATE=\"normal\""));

        assert!(!html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<HTA:APPLICATION"));
        assert!(html.contains("</HTA:APPLICATION>"));
        assert!(!html.contains("WINDOWSTATE=\"maximize\"\r\n/>"));

        assert!(html.contains("applyKioskGeometry"));
        assert!(html.contains("window.onresize = function() {"));
        assert!(
            html.contains("kioskTimer = window.setInterval(enforceKioskGeometry, kioskEnforceMs);")
        );
        assert!(html.contains("var kioskEnforceMs = 2000;"));
        assert!(html.contains("var maxKioskApplyAttempts = 0;"));
        assert!(html.contains("if (maxKioskApplyAttempts > 0 && kioskApplyAttempts >= maxKioskApplyAttempts && kioskTimer !== null) {"));
        assert!(html
            .contains("Math.max(toPositiveInt(screen.width), toPositiveInt(screen.availWidth));"));
        assert!(html.contains(
            "Math.max(toPositiveInt(screen.height), toPositiveInt(screen.availHeight));"
        ));
        assert!(html.contains("function getScreenOrigin()"));
        assert!(html.contains("function normalizeHtaMode(value)"));
        assert!(html.contains("function readHtaMode()"));
        assert!(html.contains("htaMode = readHtaMode();"));
        assert!(html.contains("if (htaMode !== \"basic\") {"));
        assert!(html.contains("var learnedChromeOffsetY = 0;"));
        assert!(html.contains("var baselineOverscanX = 0;"));
        assert!(html.contains("var baselineOverscanY = 0;"));
        assert!(html.contains("var maxLearnableChromeOffsetX = 120;"));
        assert!(html.contains("var maxLearnableChromeOffsetY = 120;"));
        assert!(html.contains("if (missingW > 0 && missingW <= maxLearnableChromeOffsetX) {"));
        assert!(html.contains("if (missingH > 0 && missingH <= maxLearnableChromeOffsetY) {"));
        assert!(html.contains("var viewportFilled = viewport.width >= target.width && viewport.height >= target.height;"));
        assert!(html.contains("if (viewportFilled && atNaturalPosition && atNaturalSize) {"));
        assert!(html.contains("learnedChromeOffsetY = 0;"));
        assert!(html.contains("learnedChromeOffsetY = Math.max(learnedChromeOffsetY, missingH);"));
        assert!(html.contains("var missingH = target.height - viewport.height;"));
        assert!(!html.contains("function getScreenReservedOffsets()"));
        assert!(html.contains("var origin = getScreenOrigin();"));
        assert!(html.contains("var desiredX = origin.x - Math.floor(effectiveOverscanX / 2);"));
        assert!(html.contains("var desiredY = origin.y - effectiveOverscanY;"));
        assert!(html.contains("var needsApply = !viewportFilled || !positionMatch || !sizeMatch;"));
        assert!(html.contains("window.moveTo(desiredX, desiredY);"));
        assert!(html.contains("window.resizeTo(desiredW, desiredH);"));
        assert!(!html.contains("var baseWindowX = null;"));
        assert!(!html.contains("window.moveTo(0, 0);"));
        assert!(!html.contains("window.moveTo(0, -40);"));
        assert!(!html.contains("window.resizeTo(screen.width + 8, screen.height + 48);"));
        assert!(!html.contains("toggleWindowMode"));
        assert!(!html.contains("applyEscapeGeometry"));
        assert!(!html.contains("Ctrl+Shift+P"));
        assert!(!html.contains("key === \"p\""));

        assert!(html.contains("Console Output"));
        // Log panel uses flex instead of fixed heights
        assert!(html.contains("flex: 1;"));
        assert!(html.contains("min-height: 120px;"));
        assert!(!html.contains("max-height: 260px;"));
        // Larger log font for readability
        assert!(html.contains("font-size: 15px;"));
        assert!(html.contains("line-height: 1.55;"));
        // Responsive breakpoints still present
        assert!(html.contains("@media screen and (max-height: 900px)"));
        assert!(html.contains("@media screen and (max-height: 768px)"));
        assert!(html.contains("@media screen and (max-height: 680px)"));

        // Issue 3: Updated:-- removed from footer
        assert!(!html.contains("Updated: --"));
        // Issue 4: UK spelling
        assert!(html.contains("initialising"));
        assert!(!html.contains("initializing"));
        // Issue 2: shell-launch.log fallback
        assert!(html.contains("shell-launch.log"));
    }

    #[test]
    fn hta_kiosk_geometry_uses_bounded_fallback_overscan() {
        let html = generate_deploy_hta();
        assert!(html.contains("if (missingH > 0 && missingH <= maxLearnableChromeOffsetY) {"));
        assert!(html.contains("if (missingW > 0 && missingW <= maxLearnableChromeOffsetX) {"));
        assert!(html.contains("var baselineOverscanX = 0;"));
        assert!(html.contains("var baselineOverscanY = 0;"));
        assert!(html.contains("if (viewportFilled && atNaturalPosition && atNaturalSize) {"));
        assert!(!html.contains("function getScreenReservedOffsets()"));
    }

    #[test]
    fn kiosk_helper_has_win32_api_calls() {
        let ps1 = generate_kiosk_helper_ps1();
        assert!(ps1.contains("Add-Type"));
        assert!(ps1.contains("FindWindow"));
        assert!(ps1.contains("GetWindowRect"));
        assert!(ps1.contains("IsWindow"));
        assert!(ps1.contains("SetWindowLong"));
        assert!(ps1.contains("SetWindowPos"));
        assert!(ps1.contains("GetSystemMetrics"));
        assert!(ps1.contains("WS_CAPTION"));
        assert!(ps1.contains("BitOSDT Deployment"));
        assert!(ps1.contains("SWP_FRAMECHANGED"));
        assert!(ps1.contains("Write-KioskLog"));
        assert!(ps1.contains("Get-CimInstance Win32_Process -Filter \"Name = 'mshta.exe'\""));
        assert!(ps1.contains("Resolved HTA window from command line"));
        assert!(ps1.contains("Fullscreen verification pass"));
        assert!(ps1.contains("$RequiredStablePasses = 3"));
        assert!(ps1.contains("$MaxAttempts = 120"));
    }

    #[test]
    fn write_hta_mode_config_writes_default_mode_file() {
        let temp = tempdir().expect("temp dir");
        let path = write_hta_mode_config(temp.path()).expect("write hta mode config");
        let payload = fs::read_to_string(path).expect("read hta mode config");
        assert_eq!(payload.trim(), resolve_winpe_hta_mode().as_str());
    }
}
