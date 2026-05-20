use crate::core::errors::{BitOSDTError, BitOSDTResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShellLayoutConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub items: Vec<ShellLayoutItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShellLayoutItem {
    pub id: String,
    pub label: String,
    pub item_type: String,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub shortcut_target_path: Option<String>,
    #[serde(default)]
    pub shortcut_arguments: Option<String>,
    #[serde(default)]
    pub shortcut_working_directory: Option<String>,
    #[serde(default)]
    pub shortcut_icon_path: Option<String>,
    #[serde(default)]
    pub desktop: bool,
    #[serde(default)]
    pub start: bool,
    #[serde(default)]
    pub taskbar: bool,
}

impl ShellLayoutConfig {
    pub fn has_work(&self) -> bool {
        self.enabled
            && self
                .items
                .iter()
                .any(|item| item.desktop || item.start || item.taskbar)
    }
}

pub fn empty_shell_layout_value() -> serde_json::Value {
    serde_json::json!({
        "enabled": false,
        "items": []
    })
}

pub fn generate_shell_layout_script(
    config: &ShellLayoutConfig,
    copy_destination: Option<&str>,
    defer_to_first_logon: bool,
) -> BitOSDTResult<String> {
    if !config.has_work() {
        return Err(BitOSDTError::Validation(
            "Shell layout generation requires at least one placed item.".to_string(),
        ));
    }

    let payload_json = serde_json::to_string_pretty(&serde_json::json!({
        "copyDestination": copy_destination.unwrap_or(r"C:\BitOSDT\Files"),
        "items": config.items,
    }))?;
    let defer_literal = if defer_to_first_logon {
        "$true"
    } else {
        "$false"
    };

    Ok(format!(
        r#"param([switch]$Deferred)

$ErrorActionPreference = "Continue"
$ShouldDeferToFirstLogon = {defer_literal}

function Write-Log {{
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    Write-Host "$timestamp [$Level] $Message"
}}

$payload = @'
{payload_json}
'@
$config = $payload | ConvertFrom-Json
$layoutRoot = "C:\ProgramData\BitOSDT\ShellLayout"
$shortcutRoot = Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\BitOSDT Canvas"
$publicDesktop = Join-Path $env:PUBLIC "Desktop"
New-Item -Path $layoutRoot -ItemType Directory -Force | Out-Null
New-Item -Path $shortcutRoot -ItemType Directory -Force | Out-Null
New-Item -Path $publicDesktop -ItemType Directory -Force | Out-Null

function Normalize-CanvasName {{
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) {{
        return "item"
    }}
    $clean = [regex]::Replace($Value.ToLowerInvariant(), "[^a-z0-9]+", "-").Trim("-")
    if ([string]::IsNullOrWhiteSpace($clean)) {{
        return "item"
    }}
    return $clean
}}

function Is-LocalSystemContext {{
    try {{
        $currentUserSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        return $currentUserSid -eq "S-1-5-18"
    }} catch {{
        return $false
    }}
}}

function Register-DeferredShellLayout {{
    $deferredScriptPath = "C:\Windows\Setup\Scripts\Apply-ShellLayout.ps1"
    $deferredDir = Split-Path -Path $deferredScriptPath -Parent
    if (-not (Test-Path $deferredDir)) {{
        New-Item -Path $deferredDir -ItemType Directory -Force | Out-Null
    }}

    Set-Content -Path $deferredScriptPath -Value $MyInvocation.MyCommand.Definition -Encoding UTF8

    $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$deferredScriptPath`" -Deferred"
    $hooked = $false
    foreach ($hostScriptPath in @(
        "C:\Windows\Setup\Scripts\Install-WingetApps.ps1",
        "C:\Windows\Setup\Scripts\Install-NetworkApps.ps1"
    )) {{
        if (-not (Test-Path $hostScriptPath)) {{
            continue
        }}

        $hostContent = Get-Content -Path $hostScriptPath -Raw -ErrorAction SilentlyContinue
        if ($hostContent -and $hostContent.Contains("Apply-ShellLayout.ps1")) {{
            $hooked = $true
            continue
        }}

        $invocation = @'

Write-Host "Applying BitOSDT desktop customisation..."
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "C:\Windows\Setup\Scripts\Apply-ShellLayout.ps1" -Deferred
'@
        Add-Content -Path $hostScriptPath -Value $invocation
        $hooked = $true
    }}

    if ($hooked) {{
        Write-Log "Attached desktop customisation to the deferred installer handoff for first admin logon." "WARNING"
        return
    }}

    $runOncePath = "HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce"
    if (-not (Test-Path $runOncePath)) {{
        New-Item -Path $runOncePath -Force | Out-Null
    }}

    New-ItemProperty -Path $runOncePath -Name "BitOSDTShellLayout" -PropertyType String -Value $command -Force | Out-Null
    Write-Log "Deferred desktop customisation to first admin logon." "WARNING"
}}

if ($ShouldDeferToFirstLogon -and (-not $Deferred) -and (Is-LocalSystemContext)) {{
    Register-DeferredShellLayout
    return
}}

function Find-ShortcutCandidate {{
    param([string]$Label)
    $roots = @(
        (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu"),
        (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs"),
        (Join-Path $env:PUBLIC "Desktop")
    ) | Select-Object -Unique
    $normalized = Normalize-CanvasName $Label
    $candidates = @()
    foreach ($root in $roots) {{
        if (Test-Path $root) {{
            $candidates += Get-ChildItem -Path $root -Filter *.lnk -Recurse -ErrorAction SilentlyContinue
        }}
    }}

    $exact = $candidates |
        Where-Object {{ (Normalize-CanvasName $_.BaseName) -eq $normalized }} |
        Select-Object -First 1
    if ($exact) {{
        return $exact.FullName
    }}

    $partial = $candidates |
        Where-Object {{
            $candidate = Normalize-CanvasName $_.BaseName
            $candidate.Contains($normalized) -or $normalized.Contains($candidate)
        }} |
        Select-Object -First 1
    if ($partial) {{
        return $partial.FullName
    }}

    return $null
}}

function Resolve-CanvasTarget {{
    param($Entry)
    if ($Entry.itemType -eq "shortcut" -and -not [string]::IsNullOrWhiteSpace($Entry.shortcutTargetPath)) {{
        return $Entry.shortcutTargetPath
    }}

    if ($Entry.itemType -eq "copied" -and -not [string]::IsNullOrWhiteSpace($Entry.sourcePath)) {{
        $leaf = Split-Path -Path $Entry.sourcePath -Leaf
        $copiedPath = Join-Path $config.copyDestination $leaf
        if (Test-Path $copiedPath) {{
            return $copiedPath
        }}
    }}

    return Find-ShortcutCandidate -Label $Entry.label
}}

function Resolve-CanvasTargetWithRetry {{
    param($Entry)

    $retrySeconds = if ($Entry.itemType -eq "winget") {{ 420 }} elseif ($Entry.itemType -eq "shortcut") {{ 30 }} else {{ 120 }}
    $sleepSeconds = 5
    $deadline = (Get-Date).AddSeconds($retrySeconds)
    $attemptedWait = $false

    do {{
        $target = Resolve-CanvasTarget -Entry $Entry
        if (-not [string]::IsNullOrWhiteSpace($target)) {{
            if ($attemptedWait) {{
                Write-Log "Resolved shell layout target for '$($Entry.label)' after waiting for deferred assets."
            }}
            return $target
        }}

        if ((Get-Date) -ge $deadline) {{
            break
        }}

        if (-not $attemptedWait) {{
            Write-Log "Waiting for shell layout target '$($Entry.label)' to become available." "WARNING"
            $attemptedWait = $true
        }}
        Start-Sleep -Seconds $sleepSeconds
    }} while ($true)

    return $null
}}

function Ensure-CanvasShortcut {{
    param($Entry)
    $target = Resolve-CanvasTargetWithRetry -Entry $Entry
    if ([string]::IsNullOrWhiteSpace($target)) {{
        Write-Log "Skipping shell layout item '$($Entry.label)' because no installed shortcut or target path was found." "WARNING"
        return $null
    }}

    $safeName = Normalize-CanvasName $Entry.label
    $shortcutPath = Join-Path $shortcutRoot ($safeName + ".lnk")
    if ($target.EndsWith(".lnk", [System.StringComparison]::OrdinalIgnoreCase)) {{
        Copy-Item -Path $target -Destination $shortcutPath -Force
    }} else {{
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $target

        if (-not [string]::IsNullOrWhiteSpace($Entry.shortcutArguments)) {{
            $shortcut.Arguments = $Entry.shortcutArguments
        }}

        if (-not [string]::IsNullOrWhiteSpace($Entry.shortcutWorkingDirectory)) {{
            $shortcut.WorkingDirectory = $Entry.shortcutWorkingDirectory
        }} else {{
            $shortcut.WorkingDirectory = Split-Path -Path $target -Parent
        }}

        if (-not [string]::IsNullOrWhiteSpace($Entry.shortcutIconPath)) {{
            $shortcut.IconLocation = $Entry.shortcutIconPath
        }}

        $shortcut.Save()
    }}

    return $shortcutPath
}}

$resolvedItems = @()
foreach ($entry in $config.items) {{
    if (-not ($entry.desktop -or $entry.start -or $entry.taskbar)) {{
        continue
    }}
    $shortcutPath = Ensure-CanvasShortcut -Entry $entry
    if (-not [string]::IsNullOrWhiteSpace($shortcutPath)) {{
        $resolvedItems += [pscustomobject]@{{
            label = $entry.label
            shortcutPath = $shortcutPath
            desktop = [bool]$entry.desktop
            start = [bool]$entry.start
            taskbar = [bool]$entry.taskbar
        }}
    }}
}}

foreach ($item in ($resolvedItems | Where-Object {{ $_.desktop }})) {{
    $desktopShortcut = Join-Path $publicDesktop (Split-Path -Path $item.shortcutPath -Leaf)
    Copy-Item -Path $item.shortcutPath -Destination $desktopShortcut -Force
}}

$startItems = @($resolvedItems | Where-Object {{ $_.start }})
$taskbarItems = @($resolvedItems | Where-Object {{ $_.taskbar }})
if ($startItems.Count -gt 0 -or $taskbarItems.Count -gt 0) {{
    $defaultNs = "http://schemas.microsoft.com/Start/2014/LayoutModification"
    $fullNs = "http://schemas.microsoft.com/Start/2014/FullDefaultLayout"
    $startNs = "http://schemas.microsoft.com/Start/2014/StartLayout"
    $taskbarNs = "http://schemas.microsoft.com/Start/2014/TaskbarLayout"
    $xml = New-Object System.Text.StringBuilder
    [void]$xml.AppendLine('<?xml version="1.0" encoding="utf-8"?>')
    [void]$xml.AppendLine("<LayoutModificationTemplate xmlns=`"$defaultNs`" xmlns:defaultlayout=`"$fullNs`" xmlns:start=`"$startNs`" xmlns:taskbar=`"$taskbarNs`" Version=`"1`">")
    [void]$xml.AppendLine("  <LayoutOptions StartTileGroupCellWidth=`"6`" />")

    if ($startItems.Count -gt 0) {{
        [void]$xml.AppendLine("  <DefaultLayoutOverride>")
        [void]$xml.AppendLine("    <StartLayoutCollection>")
        [void]$xml.AppendLine("      <defaultlayout:StartLayout GroupCellWidth=`"6`">")
        [void]$xml.AppendLine("        <start:Group Name=`"BitOSDT`">")
        for ($index = 0; $index -lt $startItems.Count; $index++) {{
            $item = $startItems[$index]
            $column = ($index % 3) * 2
            $row = [int]($index / 3) * 2
            $escapedLink = [System.Security.SecurityElement]::Escape($item.shortcutPath)
            [void]$xml.AppendLine("          <start:DesktopApplicationTile Size=`"2x2`" Column=`"$column`" Row=`"$row`" DesktopApplicationLinkPath=`"$escapedLink`" />")
        }}
        [void]$xml.AppendLine("        </start:Group>")
        [void]$xml.AppendLine("      </defaultlayout:StartLayout>")
        [void]$xml.AppendLine("    </StartLayoutCollection>")
        [void]$xml.AppendLine("  </DefaultLayoutOverride>")
    }}

    if ($taskbarItems.Count -gt 0) {{
        [void]$xml.AppendLine("  <CustomTaskbarLayoutCollection PinListPlacement=`"Replace`">")
        [void]$xml.AppendLine("    <defaultlayout:TaskbarLayout>")
        [void]$xml.AppendLine("      <taskbar:TaskbarPinList>")
        foreach ($item in $taskbarItems) {{
            $escapedLink = [System.Security.SecurityElement]::Escape($item.shortcutPath)
            [void]$xml.AppendLine("        <taskbar:DesktopApp DesktopApplicationLinkPath=`"$escapedLink`" />")
        }}
        [void]$xml.AppendLine("      </taskbar:TaskbarPinList>")
        [void]$xml.AppendLine("    </defaultlayout:TaskbarLayout>")
        [void]$xml.AppendLine("  </CustomTaskbarLayoutCollection>")
    }}

    [void]$xml.AppendLine("</LayoutModificationTemplate>")
    $layoutPath = Join-Path $layoutRoot "LayoutModification.xml"
    Set-Content -Path $layoutPath -Value $xml.ToString() -Encoding UTF8

    $policyPath = "HKLM:\SOFTWARE\Policies\Microsoft\Windows\Explorer"
    New-Item -Path $policyPath -Force | Out-Null
    Set-ItemProperty -Path $policyPath -Name "StartLayoutFile" -Value $layoutPath -Type String
    Write-Log "Generated shell layout XML at $layoutPath and registered it for first sign-in."
}} else {{
        Write-Log "Shell layout only contained desktop shortcuts; no Start or taskbar XML was generated."
}}

try {{
    $runOncePath = "HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce"
    Remove-ItemProperty -Path $runOncePath -Name "BitOSDTShellLayout" -ErrorAction SilentlyContinue
}} catch {{
}}

Write-Log "Desktop customisation processing completed." "SUCCESS"
"#,
        defer_literal = defer_literal
    ))
}

#[cfg(test)]
mod tests {
    use super::{generate_shell_layout_script, ShellLayoutConfig, ShellLayoutItem};

    fn shortcut_item() -> ShellLayoutItem {
        ShellLayoutItem {
            id: "shortcut:1".to_string(),
            label: "Support Tools".to_string(),
            item_type: "shortcut".to_string(),
            source_ref: None,
            source_path: None,
            shortcut_target_path: Some(r"C:\Tools\SupportTools.exe".to_string()),
            shortcut_arguments: Some("--quiet".to_string()),
            shortcut_working_directory: Some(r"C:\Tools".to_string()),
            shortcut_icon_path: Some(r"C:\Tools\SupportTools.ico".to_string()),
            desktop: true,
            start: true,
            taskbar: true,
        }
    }

    #[test]
    fn generates_deferred_shell_layout_registration() {
        let config = ShellLayoutConfig {
            enabled: true,
            items: vec![shortcut_item()],
        };

        let script =
            generate_shell_layout_script(&config, None, true).expect("script should generate");

        assert!(script.contains("$ShouldDeferToFirstLogon = $true"));
        assert!(script.contains("BitOSDTShellLayout"));
        assert!(script.contains("Install-WingetApps.ps1"));
    }

    #[test]
    fn emits_explicit_shortcut_metadata_into_script_payload() {
        let config = ShellLayoutConfig {
            enabled: true,
            items: vec![shortcut_item()],
        };

        let script =
            generate_shell_layout_script(&config, None, false).expect("script should generate");

        assert!(script.contains("shortcutTargetPath"));
        assert!(script.contains("shortcutArguments"));
        assert!(script.contains("shortcutWorkingDirectory"));
        assert!(script.contains("shortcutIconPath"));
        assert!(script.contains("Resolve-CanvasTargetWithRetry"));
    }
}
