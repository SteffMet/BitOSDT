param(
    [string]$PreviewRoot = "C:\BitOSDT\WinpePreview",
    [string]$DriveLetter = "X",
    [switch]$NoLaunch,
    [switch]$DisableKioskHelper
)

$ErrorActionPreference = "Stop"

function Get-SubstMapping {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Drive
    )

    $targetDrive = ($Drive.TrimEnd(":") + ":").ToUpperInvariant()
    $output = cmd /c subst
    foreach ($line in $output) {
        if ($line -match "^\s*([A-Za-z]:)\\:\s*=>\s*(.+)\s*$") {
            $mappedDrive = $matches[1].ToUpperInvariant()
            $mappedPath = $matches[2].Trim()
            if ($mappedDrive -eq $targetDrive) {
                return $mappedPath
            }
        }
    }
    return $null
}

function Assert-DriveLetter {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Drive
    )

    $letter = $Drive.Trim().TrimEnd(":")
    if ($letter.Length -ne 1 -or $letter -notmatch "^[A-Za-z]$") {
        throw "DriveLetter must be a single letter (for example X)."
    }
    return $letter.ToUpperInvariant()
}

function Resolve-HtaTemplate {
    param(
        [Parameter(Mandatory = $true)]
        [string]$WinpeUiRsPath
    )

    if (-not (Test-Path -LiteralPath $WinpeUiRsPath)) {
        throw "Could not find WinPE UI source file at: $WinpeUiRsPath"
    }

    $source = Get-Content -LiteralPath $WinpeUiRsPath -Raw
    $rawStart = $source.IndexOf('r##"<html>')
    if ($rawStart -lt 0) {
        throw "Unable to locate HTA template block in: $WinpeUiRsPath"
    }

    $htmlStart = $source.IndexOf("<html>", $rawStart)
    $htmlEnd = $source.IndexOf("</html>", $htmlStart)
    if ($htmlStart -lt 0 -or $htmlEnd -lt 0) {
        throw "Unable to extract HTA HTML template in: $WinpeUiRsPath"
    }

    $htmlCloseTagLength = "</html>".Length
    return $source.Substring($htmlStart, ($htmlEnd + $htmlCloseTagLength) - $htmlStart)
}

function To-JsStringLiteral {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    return '"' + ($Value -replace '\\', '\\\\') + '"'
}

function Start-LocalKioskHelper {
    param(
        [Parameter(Mandatory = $true)]
        [string]$WindowTitle,
        [int]$TargetPid = 0,
        [string]$LogPath = ""
    )

    $escapedLogPath = ""
    if ($LogPath) {
        $escapedLogPath = $LogPath.Replace("'", "''")
    }

    $helperScript = @"
`$ErrorActionPreference = 'SilentlyContinue'
`$logPath = '$escapedLogPath'
function Write-PreviewKioskLog {
    param([string]`$Message)
    if (-not `$logPath) { return }
    try {
        `$stamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
        "`$stamp [KIOSK-PREVIEW] `$Message" | Out-File -FilePath `$logPath -Append -Encoding utf8
    } catch {}
}

Write-PreviewKioskLog "Preview kiosk helper started (targetPid=$TargetPid, title='$WindowTitle')."

try {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class PreviewKioskHelper {
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
}
'@
} catch {
    Write-PreviewKioskLog "Add-Type failed: `$_"
    exit 0
}

`$GWL_STYLE = -16
`$WS_CAPTION = 0x00C00000
`$WS_THICKFRAME = 0x00040000
`$SWP_FRAMECHANGED = 0x0020
`$SWP_NOZORDER = 0x0004
`$SW_SHOWMAXIMIZED = 3
`$SM_CXSCREEN = 0
`$SM_CYSCREEN = 1

for (`$attempt = 1; `$attempt -le 20; `$attempt++) {
    Start-Sleep -Milliseconds 500
    `$hwnd = [IntPtr]::Zero
    if ($TargetPid -gt 0) {
        try {
            `$proc = Get-Process -Id $TargetPid -ErrorAction SilentlyContinue
            if (`$proc -and `$proc.MainWindowHandle -and `$proc.MainWindowHandle -ne 0) {
                `$hwnd = [IntPtr]`$proc.MainWindowHandle
            }
        } catch {}
    }

    if (`$hwnd -eq [IntPtr]::Zero) {
        `$hwnd = [PreviewKioskHelper]::FindWindow(`$null, '$WindowTitle')
    }

    if (`$hwnd -eq [IntPtr]::Zero) {
        try {
            `$fallback = Get-Process -Name mshta -ErrorAction SilentlyContinue |
                Sort-Object StartTime -Descending |
                Select-Object -First 1
            if (`$fallback -and `$fallback.MainWindowHandle -and `$fallback.MainWindowHandle -ne 0) {
                `$hwnd = [IntPtr]`$fallback.MainWindowHandle
            }
        } catch {}
    }

    if (`$hwnd -ne [IntPtr]::Zero) {
        Write-PreviewKioskLog "Found HTA HWND=`$hwnd on attempt `$attempt."
        `$style = [PreviewKioskHelper]::GetWindowLong(`$hwnd, `$GWL_STYLE)
        `$newStyle = `$style -band (-bnot (`$WS_CAPTION -bor `$WS_THICKFRAME))
        [PreviewKioskHelper]::SetWindowLong(`$hwnd, `$GWL_STYLE, `$newStyle) | Out-Null
        `$w = [PreviewKioskHelper]::GetSystemMetrics(`$SM_CXSCREEN)
        `$h = [PreviewKioskHelper]::GetSystemMetrics(`$SM_CYSCREEN)
        [PreviewKioskHelper]::SetWindowPos(`$hwnd, [IntPtr]::Zero, 0, 0, `$w, `$h, (`$SWP_FRAMECHANGED -bor `$SWP_NOZORDER)) | Out-Null
        [PreviewKioskHelper]::ShowWindow(`$hwnd, `$SW_SHOWMAXIMIZED) | Out-Null
        Write-PreviewKioskLog "Applied kiosk fullscreen `${w}x`${h}."
        exit 0
    }
}
Write-PreviewKioskLog "Unable to find HTA window handle after 20 attempts."
exit 1
"@

    $encodedHelper = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($helperScript))
    & powershell.exe @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        $encodedHelper
    ) *> $null
    return $LASTEXITCODE
}

$drive = Assert-DriveLetter -Drive $DriveLetter
$repoRoot = Split-Path -Path $PSScriptRoot -Parent
$winpeUiRs = Join-Path $repoRoot "src\build\winpe_ui.rs"

$previewDriveRoot = Join-Path $PreviewRoot $drive
$bitOsdtRoot = Join-Path $previewDriveRoot "BitOSDT"
$uiDir = Join-Path $bitOsdtRoot "UI"
$stateDir = Join-Path $bitOsdtRoot "State"
$logsDir = Join-Path $bitOsdtRoot "Logs"

New-Item -ItemType Directory -Force -Path $uiDir, $stateDir, $logsDir | Out-Null

$statusPath = "$drive`:\BitOSDT\State\deploy-status.json"
$logPath = "$drive`:\BitOSDT\Logs\deploy.log"
$shellLogPath = "$drive`:\BitOSDT\Logs\shell-launch.log"

$htaTemplate = Resolve-HtaTemplate -WinpeUiRsPath $winpeUiRs
$htaContent = $htaTemplate
$htaContent = $htaContent.Replace("{status_path}", (To-JsStringLiteral -Value $statusPath))
$htaContent = $htaContent.Replace("{log_path}", (To-JsStringLiteral -Value $logPath))
$htaContent = $htaContent.Replace("{shell_log_path}", (To-JsStringLiteral -Value $shellLogPath))
$htaContent = $htaContent.Replace("{{", "{").Replace("}}", "}")
$htaContent = $htaContent -replace "`n", "`r`n"

$htaFile = Join-Path $uiDir "BitOSDT-Deploy.hta"
$statusFile = Join-Path $stateDir "deploy-status.json"
$logFile = Join-Path $logsDir "deploy.log"
$shellLogFile = Join-Path $logsDir "shell-launch.log"

Set-Content -LiteralPath $htaFile -Value $htaContent -Encoding UTF8

$statusPayload = [ordered]@{
    schema_version = 1
    mode = "full_iso"
    stage_index = 1
    stage_total = 4
    percent_complete = 1
    status_text = "Preparing deployment..."
    detail_text = "Local preview mode started."
    last_updated_utc = (Get-Date).ToUniversalTime().ToString("o")
    is_error = $false
    error_message = $null
} | ConvertTo-Json -Depth 4

Set-Content -LiteralPath $statusFile -Value $statusPayload -Encoding UTF8
Set-Content -LiteralPath $logFile -Value "$(Get-Date -Format "yyyy-MM-dd HH:mm:ss") [INFO] Local preview started." -Encoding UTF8
Set-Content -LiteralPath $shellLogFile -Value "$(Get-Date -Format "yyyy-MM-dd HH:mm:ss") [INFO] Shell log preview fallback." -Encoding UTF8

$mappedTarget = Get-SubstMapping -Drive $drive
$expectedTarget = [System.IO.Path]::GetFullPath($previewDriveRoot)

if ($mappedTarget) {
    $mappedFull = [System.IO.Path]::GetFullPath($mappedTarget)
    if ($mappedFull -ne $expectedTarget) {
        throw "$drive`: is already mapped to '$mappedFull'. Use -DriveLetter with a different letter or remove the existing mapping."
    }
}
else {
    cmd /c "subst $drive`: `"$expectedTarget`""
}

Write-Host "Preview content root: $expectedTarget"
Write-Host "Mapped drive: $drive`: -> $expectedTarget"
Write-Host "HTA file: $htaFile"

if (-not $NoLaunch) {
    $htaProcess = Start-Process -FilePath "mshta.exe" -ArgumentList "`"$htaFile`"" -PassThru
    if (-not $DisableKioskHelper) {
        $kioskExit = Start-LocalKioskHelper -WindowTitle "BitOSDT Deployment" -TargetPid $htaProcess.Id -LogPath $shellLogFile
        if ($kioskExit -eq 0) {
            Write-Host "Local kiosk helper applied."
        }
        else {
            Write-Warning "Local kiosk helper could not lock fullscreen. See $shellLogFile"
        }
    }
    Write-Host "Launched WinPE UI preview with mshta.exe"
}
else {
    Write-Host "NoLaunch set, HTA generated but not started."
}
