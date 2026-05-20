use crate::core::errors::BitOSDTResult;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Application installer generator for post-deployment
pub struct AppInstaller;

/// Application installation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInstallConfig {
    /// Winget packages to install
    pub winget_packages: Vec<WingetPackage>,
    /// Chocolatey packages to install
    pub chocolatey_packages: Vec<ChocolateyPackage>,
    /// Custom installers (EXE/MSI)
    pub custom_installers: Vec<CustomInstaller>,
    /// Files/folders to copy onto the installed machine
    #[serde(default)]
    pub copied_items: Vec<LocalPayloadItem>,
    /// Destination root for copied items
    #[serde(default)]
    pub copy_destination: Option<String>,
    /// Install Chocolatey if needed
    pub auto_install_chocolatey: bool,
    /// Continue on individual app failure
    pub continue_on_error: bool,
    /// Log file path
    pub log_path: String,
    /// Optional progress json path for interactive provisioning UI
    #[serde(default)]
    pub progress_json_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LocalPayloadKind {
    #[default]
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LocalPayloadItem {
    pub source_path: String,
    #[serde(default)]
    pub source_kind: LocalPayloadKind,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WingetPackage {
    /// Package ID (e.g., "Microsoft.VisualStudioCode")
    pub package_id: String,
    /// Specific version (optional)
    pub version: Option<String>,
    /// Custom arguments
    pub custom_args: Option<String>,
    /// Enabled for installation
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChocolateyPackage {
    /// Package name (e.g., "googlechrome")
    pub package_name: String,
    /// Specific version (optional)
    pub version: Option<String>,
    /// Custom source/repository
    pub source: Option<String>,
    /// Custom arguments
    pub custom_args: Option<String>,
    /// Enabled for installation
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInstaller {
    /// Display name
    pub name: String,
    /// Path or URL to installer
    pub path: String,
    /// Source model for resolving installer input
    #[serde(default)]
    pub source_type: InstallerSourceType,
    /// Installer file name when source type is NetworkDirectory
    #[serde(default)]
    pub source_file_name: Option<String>,
    /// Supporting files/folders to copy before running the installer
    #[serde(default)]
    pub dependencies: Vec<LocalPayloadItem>,
    /// Destination root for supporting files/folders
    #[serde(default)]
    pub dependency_destination: Option<String>,
    /// Silent install arguments
    pub silent_args: String,
    /// Installer type
    pub installer_type: InstallerType,
    /// Expected return codes (default: 0, 3010)
    pub success_codes: Vec<i32>,
    /// Enabled for installation
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InstallerType {
    Exe,
    Msi,
    Msix,
    Msp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum InstallerSourceType {
    #[default]
    DirectPathOrUrl,
    EmbeddedFile,
    NetworkDirectory,
}

impl Default for AppInstallConfig {
    fn default() -> Self {
        Self {
            winget_packages: Vec::new(),
            chocolatey_packages: Vec::new(),
            custom_installers: Vec::new(),
            copied_items: Vec::new(),
            copy_destination: None,
            auto_install_chocolatey: true,
            continue_on_error: true,
            log_path: "C:\\BitOSDT\\Logs\\app-install.log".to_string(),
            progress_json_path: None,
        }
    }
}

impl AppInstaller {
    /// Generate complete PowerShell script for application installation
    pub fn generate_install_script(config: &AppInstallConfig) -> BitOSDTResult<String> {
        info!("Generating application installation script");

        let mut script = String::new();

        // Script header
        script.push_str(&Self::script_header(
            &config.log_path,
            config.continue_on_error,
            config.progress_json_path.as_deref(),
            Self::enabled_progress_count(config),
        ));

        // Winget installations
        if !config.copied_items.is_empty() {
            script.push_str(&Self::generate_payload_copy_section(
                &config.copied_items,
                config.copy_destination.as_deref(),
                config.continue_on_error,
            ));
        }

        // Winget installations
        if config.winget_packages.iter().any(|p| p.enabled) {
            script.push_str(&Self::generate_winget_section(&config.winget_packages));
        }

        // Chocolatey installations
        if config.chocolatey_packages.iter().any(|p| p.enabled) {
            script.push_str(&Self::generate_chocolatey_section(
                &config.chocolatey_packages,
                config.auto_install_chocolatey,
            ));
        }

        // Custom installers
        if config.custom_installers.iter().any(|i| i.enabled) {
            script.push_str(&Self::generate_custom_section(
                &config.custom_installers,
                config.continue_on_error,
            ));
        }

        // Script footer
        script.push_str(&Self::script_footer());

        Ok(script)
    }

    fn enabled_progress_count(config: &AppInstallConfig) -> usize {
        config.winget_packages.iter().filter(|p| p.enabled).count()
            + config
                .chocolatey_packages
                .iter()
                .filter(|p| p.enabled)
                .count()
            + config
                .custom_installers
                .iter()
                .filter(|installer| installer.enabled)
                .count()
            + config.copied_items.len()
    }

    fn generate_payload_copy_section(
        payloads: &[LocalPayloadItem],
        destination: Option<&str>,
        continue_on_error: bool,
    ) -> String {
        if payloads.is_empty() {
            return String::new();
        }

        let mut script = String::from(
            r#"
# ================================================
# FILES AND FOLDERS
# ================================================

Write-Log "Copying configured files and folders..."

"#,
        );

        for payload in payloads {
            let label = Self::payload_display_name(payload);
            script.push_str(&Self::generate_payload_copy_block(
                payload,
                destination,
                continue_on_error,
                &label,
                &format!("payload:{}", label),
                "",
            ));
        }

        script
    }

    fn script_header(
        log_path: &str,
        continue_on_error: bool,
        progress_json_path: Option<&str>,
        progress_total: usize,
    ) -> String {
        let error_action = if continue_on_error {
            "Continue"
        } else {
            "Stop"
        };
        let progress_json_path = progress_json_path
            .map(Self::escape_ps_double_quoted)
            .unwrap_or_default();
        let progress_path_line = if progress_json_path.is_empty() {
            "$ProvisioningProgressPath = \"\"".to_string()
        } else {
            format!("$ProvisioningProgressPath = \"{}\"", progress_json_path)
        };

        format!(
            r#"# BitOSDT Application Installation Script
# Generated by BitOSDT 2.0
# ================================================

$ErrorActionPreference = "{error_action}"
$LogPath = "{log_path}"
$InstallErrors = @()
$ProvisioningProgressTotal = {progress_total}
$AppProgressCompleted = 0
{progress_path_line}

# Create log directory
$logDir = Split-Path $LogPath -Parent
if (-not (Test-Path $logDir)) {{
    New-Item -Path $logDir -ItemType Directory -Force | Out-Null
}}

function Write-Log {{
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logLine = "$timestamp [$Level] $Message"
    $logLine | Out-File -Append $LogPath
    
    switch ($Level) {{
        "ERROR" {{ Write-Host $logLine -ForegroundColor Red }}
        "WARNING" {{ Write-Host $logLine -ForegroundColor Yellow }}
        "SUCCESS" {{ Write-Host $logLine -ForegroundColor Green }}
        default {{ Write-Host $logLine }}
    }}
}}

function Write-AppProgress {{
    param(
        [string]$CurrentItem = "",
        [string]$State = "active",
        [string]$Message = "",
        [switch]$IncrementCompleted
    )

    if ([string]::IsNullOrWhiteSpace($ProvisioningProgressPath)) {{
        return
    }}

    if ($IncrementCompleted) {{
        $script:AppProgressCompleted++
    }}

    try {{
        $progressDir = Split-Path $ProvisioningProgressPath -Parent
        if (-not (Test-Path $progressDir)) {{
            New-Item -Path $progressDir -ItemType Directory -Force | Out-Null
        }}

        $payload = [ordered]@{{
            schemaVersion = 1
            currentItem = $CurrentItem
            state = $State
            completedCount = $script:AppProgressCompleted
            totalCount = $ProvisioningProgressTotal
            message = $Message
            updatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
        }}

        $payload | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ProvisioningProgressPath -Encoding UTF8
    }} catch {{
    }}
}}

function Test-ReturnCode {{
    param([int]$Code, [int[]]$SuccessCodes = @(0, 3010))
    return $SuccessCodes -contains $Code
}}

Write-Log "Starting application installation..."
Write-Log "Log file: $LogPath"
Write-AppProgress -CurrentItem "Queued" -State "idle" -Message "Preparing application tasks"

"#,
            progress_total = progress_total,
            progress_path_line = progress_path_line
        )
    }

    fn generate_winget_section(packages: &[WingetPackage]) -> String {
        let enabled_packages: Vec<_> = packages.iter().filter(|p| p.enabled).collect();
        if enabled_packages.is_empty() {
            return String::new();
        }

        let deferred_script = Self::generate_deferred_winget_script(&enabled_packages);
        let mut script = String::new();
        script.push_str(
            r#"
# ================================================
# WINGET PACKAGES
# ================================================

Write-Log "Installing winget packages..."

$currentUserSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$isLocalSystem = $currentUserSid -eq "S-1-5-18"

if ($isLocalSystem) {
    Write-Log "Detected LocalSystem context (S-1-5-18). Deferring winget package installation to first admin logon." "WARNING"
    try {
        $wingetScriptPath = "C:\Windows\Setup\Scripts\Install-WingetApps.ps1"
        $wingetScriptContent = @'
"#,
        );
        script.push_str(&deferred_script);
        script.push_str(
            r#"'@
        $wingetScriptDir = Split-Path $wingetScriptPath -Parent
        if (-not (Test-Path $wingetScriptDir)) {
            New-Item -Path $wingetScriptDir -ItemType Directory -Force | Out-Null
        }
        Set-Content -Path $wingetScriptPath -Value $wingetScriptContent -Encoding UTF8

        $runOncePath = "HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce"
        if (-not (Test-Path $runOncePath)) {
            New-Item -Path $runOncePath -Force | Out-Null
        }
        $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$wingetScriptPath`""
        New-ItemProperty -Path $runOncePath -Name "BitOSDTWingetInstallers" -PropertyType String -Value $command -Force | Out-Null
        Write-Log "Deferred winget installer script registered for first admin logon." "WARNING"
    } catch {
        Write-Log "Failed to register deferred winget installer script: $_" "ERROR"
        $InstallErrors += "winget:deferred-registration"
    }
} else {
    # Check if winget is available
    $wingetPath = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $wingetPath) {
        Write-Log "Winget not found. Attempting to install App Installer..." "WARNING"
        
        # Try to register the App Installer
        try {
            Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe -ErrorAction Stop
            Start-Sleep -Seconds 5
            $wingetPath = Get-Command winget -ErrorAction SilentlyContinue
        } catch {
            Write-Log "Failed to install winget: $_" "ERROR"
        }
    }

    if ($wingetPath) {
        Write-Log "Winget found at: $($wingetPath.Source)"
    
"#,
        );

        // Generate install commands for each package
        for pkg in enabled_packages {
            let version_arg = pkg
                .version
                .as_ref()
                .map(|v| format!(" --version \"{}\"", v))
                .unwrap_or_default();

            let custom_args = pkg.custom_args.as_deref().unwrap_or("");

            script.push_str(&format!(
                r#"        # Install: {id}
        Write-Log "Installing {id}..."
        Write-AppProgress -CurrentItem "{id}" -State "active" -Message "Installing {id}"
        try {{
            winget install --id "{id}" --silent --accept-source-agreements --accept-package-agreements{version}{custom}
            if ($LASTEXITCODE -eq 0 -or $LASTEXITCODE -eq $null) {{
                Write-Log "{id} installed successfully" "SUCCESS"
                Write-AppProgress -CurrentItem "{id}" -State "complete" -Message "Installed {id}" -IncrementCompleted
            }} else {{
                Write-Log "{id} installation returned code: $LASTEXITCODE" "WARNING"
                $InstallErrors += "{id}"
                Write-AppProgress -CurrentItem "{id}" -State "error" -Message "Failed to install {id}" -IncrementCompleted
            }}
        }} catch {{
            Write-Log "Error installing {id}: $_" "ERROR"
            $InstallErrors += "{id}"
            Write-AppProgress -CurrentItem "{id}" -State "error" -Message "Failed to install {id}" -IncrementCompleted
        }}

"#,
                id = pkg.package_id,
                version = version_arg,
                custom = if custom_args.is_empty() {
                    "".to_string()
                } else {
                    format!(" {}", custom_args)
                }
            ));
        }

        script.push_str(
            r#"    } else {
        Write-Log "Winget is not available - skipping winget packages" "ERROR"
        $InstallErrors += "winget-not-available"
    }
}

"#,
        );

        script
    }

    fn generate_deferred_winget_script(packages: &[&WingetPackage]) -> String {
        let mut script = String::new();
        script.push_str(
            r#"# BitOSDT Deferred Winget Installer Script
$ErrorActionPreference = "Continue"
$LogPath = "C:\BitOSDT\Logs\app-install-winget-deferred.log"
$InstallErrors = @()

if (-not (Test-Path (Split-Path $LogPath -Parent))) {
    New-Item -Path (Split-Path $LogPath -Parent) -ItemType Directory -Force | Out-Null
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logLine = "$timestamp [$Level] $Message"
    $logLine | Out-File -Append $LogPath
    Write-Host $logLine
}

Write-Log "Starting deferred winget installation..."

$wingetPath = Get-Command winget -ErrorAction SilentlyContinue
if (-not $wingetPath) {
    Write-Log "Winget not found. Attempting to install App Installer..." "WARNING"

    try {
        Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe -ErrorAction Stop
        Start-Sleep -Seconds 5
        $wingetPath = Get-Command winget -ErrorAction SilentlyContinue
    } catch {
        Write-Log "Failed to install winget: $_" "ERROR"
    }
}

if (-not $wingetPath) {
    Write-Log "Winget is not available - skipping winget packages" "ERROR"
    $InstallErrors += "winget-not-available"
} else {
    Write-Log "Winget found at: $($wingetPath.Source)"

"#,
        );

        for pkg in packages {
            let version_arg = pkg
                .version
                .as_ref()
                .map(|v| format!(" --version \"{}\"", v))
                .unwrap_or_default();

            let custom_args = pkg.custom_args.as_deref().unwrap_or("");

            script.push_str(&format!(
                r#"    # Install: {id}
    Write-Log "Installing {id}..."
    try {{
        winget install --id "{id}" --silent --accept-source-agreements --accept-package-agreements{version}{custom}
        if ($LASTEXITCODE -eq 0 -or $LASTEXITCODE -eq $null) {{
            Write-Log "{id} installed successfully" "SUCCESS"
        }} else {{
            Write-Log "{id} installation returned code: $LASTEXITCODE" "WARNING"
            $InstallErrors += "{id}"
        }}
    }} catch {{
        Write-Log "Error installing {id}: $_" "ERROR"
        $InstallErrors += "{id}"
    }}

"#,
                id = pkg.package_id,
                version = version_arg,
                custom = if custom_args.is_empty() {
                    "".to_string()
                } else {
                    format!(" {}", custom_args)
                }
            ));
        }

        script.push_str(
            r#"}

try {
    $runOncePath = "HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce"
    Remove-ItemProperty -Path $runOncePath -Name "BitOSDTWingetInstallers" -ErrorAction SilentlyContinue
} catch {
    Write-Log "Failed to cleanup RunOnce entry BitOSDTWingetInstallers: $_" "WARNING"
}

if ($InstallErrors.Count -gt 0) {
    Write-Log "Deferred winget installation completed with $($InstallErrors.Count) error(s)." "ERROR"
    foreach ($error in $InstallErrors) {
        Write-Log "  - $error" "ERROR"
    }
    exit 1
}

Write-Log "Deferred winget installation completed successfully." "SUCCESS"
exit 0

"#,
        );

        script
    }

    fn generate_chocolatey_section(packages: &[ChocolateyPackage], auto_install: bool) -> String {
        let enabled_packages: Vec<_> = packages.iter().filter(|p| p.enabled).collect();
        if enabled_packages.is_empty() {
            return String::new();
        }

        let mut script = String::new();
        script.push_str(
            r#"
# ================================================
# CHOCOLATEY PACKAGES
# ================================================

Write-Log "Installing Chocolatey packages..."

# Check if Chocolatey is installed
$chocoPath = Get-Command choco -ErrorAction SilentlyContinue

"#,
        );

        if auto_install {
            script.push_str(
                r#"if (-not $chocoPath) {
    Write-Log "Chocolatey not found. Installing Chocolatey..." "WARNING"
    try {
        Set-ExecutionPolicy Bypass -Scope Process -Force
        [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
        $downloadedScript = (New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1')
        & ([ScriptBlock]::Create($downloadedScript))

        $chocoPath = Get-Command choco -ErrorAction SilentlyContinue
        if ($chocoPath) {
            $env:ChocolateyInstall = Convert-Path "$($chocoPath.Source)\..\.."
            $chocoProfileModule = "$env:ChocolateyInstall\helpers\chocolateyProfile.psm1"
            if (Test-Path $chocoProfileModule) {
                Import-Module $chocoProfileModule -ErrorAction SilentlyContinue
            }
            if (Get-Command refreshenv -ErrorAction SilentlyContinue) {
                refreshenv
            }
            Write-Log "Chocolatey installed successfully" "SUCCESS"
        } else {
            Write-Log "Chocolatey installer completed but choco is still unavailable." "ERROR"
        }
    } catch {
        Write-Log "Failed to install Chocolatey: $_" "ERROR"
    }
}

"#,
            );
        }

        script.push_str(
            r#"if ($chocoPath) {
    Write-Log "Chocolatey found at: $($chocoPath.Source)"
    
"#,
        );

        // Generate install commands for each package
        for pkg in enabled_packages {
            let version_arg = pkg
                .version
                .as_ref()
                .map(|v| format!(" --version=\"{}\"", v))
                .unwrap_or_default();

            let source_arg = pkg
                .source
                .as_ref()
                .map(|s| format!(" --source=\"{}\"", s))
                .unwrap_or_default();

            let custom_args = pkg.custom_args.as_deref().unwrap_or("");

            script.push_str(&format!(
                r#"    # Install: {name}
    Write-Log "Installing {name} via Chocolatey..."
    Write-AppProgress -CurrentItem "{name}" -State "active" -Message "Installing {name}"
    try {{
        choco install {name} -y --no-progress{version}{source}{custom}
        if ($LASTEXITCODE -eq 0) {{
            Write-Log "{name} installed successfully" "SUCCESS"
            Write-AppProgress -CurrentItem "{name}" -State "complete" -Message "Installed {name}" -IncrementCompleted
        }} else {{
            Write-Log "{name} installation returned code: $LASTEXITCODE" "WARNING"
            $InstallErrors += "choco:{name}"
            Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
        }}
    }} catch {{
        Write-Log "Error installing {name}: $_" "ERROR"
        $InstallErrors += "choco:{name}"
        Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
    }}

"#,
                name = pkg.package_name,
                version = version_arg,
                source = source_arg,
                custom = if custom_args.is_empty() {
                    "".to_string()
                } else {
                    format!(" {}", custom_args)
                }
            ));
        }

        script.push_str(
            r#"} else {
    Write-Log "Chocolatey is not available - skipping Chocolatey packages" "ERROR"
    $InstallErrors += "chocolatey-not-available"
}

"#,
        );

        script
    }

    fn generate_custom_section(installers: &[CustomInstaller], continue_on_error: bool) -> String {
        let enabled_installers: Vec<_> = installers.iter().filter(|i| i.enabled).collect();
        if enabled_installers.is_empty() {
            return String::new();
        }

        let mut immediate_installers = Vec::new();
        let mut deferred_network_installers = Vec::new();
        for installer in enabled_installers {
            if installer.source_type == InstallerSourceType::NetworkDirectory {
                deferred_network_installers.push(installer);
            } else {
                immediate_installers.push(installer);
            }
        }

        let mut script = String::new();
        script.push_str(
            r#"
# ================================================
# CUSTOM INSTALLERS
# ================================================

Write-Log "Installing custom applications..."

"#,
        );

        if immediate_installers.is_empty() {
            script.push_str(
                r#"Write-Log "No immediate custom installers configured."

"#,
            );
        } else {
            for installer in immediate_installers {
                for dependency in &installer.dependencies {
                    let label = format!(
                        "{} dependency {}",
                        installer.name,
                        Self::payload_display_name(dependency)
                    );
                    script.push_str(&Self::generate_payload_copy_block(
                        dependency,
                        installer.dependency_destination.as_deref(),
                        continue_on_error,
                        &label,
                        &format!("custom:{}:dependency", installer.name),
                        "",
                    ));
                }
                script.push_str(&Self::generate_immediate_custom_installer(installer));
            }
        }

        if !deferred_network_installers.is_empty() {
            let deferred_script = Self::generate_deferred_network_script(
                &deferred_network_installers,
                continue_on_error,
            );
            script.push_str(&format!(
                r#"# Defer network-path installers to first admin logon
try {{
    $networkScriptPath = "C:\Windows\Setup\Scripts\Install-NetworkApps.ps1"
    $networkScriptContent = @'
{deferred_script}
'@
    $networkScriptDir = Split-Path $networkScriptPath -Parent
    if (-not (Test-Path $networkScriptDir)) {{
        New-Item -Path $networkScriptDir -ItemType Directory -Force | Out-Null
    }}
    Set-Content -Path $networkScriptPath -Value $networkScriptContent -Encoding UTF8

    $runOncePath = "HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce"
    if (-not (Test-Path $runOncePath)) {{
        New-Item -Path $runOncePath -Force | Out-Null
    }}
    $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$networkScriptPath`""
    New-ItemProperty -Path $runOncePath -Name "BitOSDTNetworkInstallers" -PropertyType String -Value $command -Force | Out-Null
    Write-Log "Deferred network installer script registered for first admin logon." "WARNING"
}} catch {{
    Write-Log "Failed to register deferred network installer script: $_" "ERROR"
    $InstallErrors += "custom:network-deferred-registration"
}}

"#
            ));
        }

        script
    }

    fn generate_immediate_custom_installer(installer: &CustomInstaller) -> String {
        let name = Self::escape_ps_double_quoted(&installer.name);
        let path = Self::escape_ps_double_quoted(&installer.path);
        let args = Self::escape_ps_double_quoted(&installer.silent_args);
        let name_safe = Self::sanitize_name_for_temp(&installer.name);
        let success_codes = Self::render_success_codes(&installer.success_codes);
        let exe_start_process =
            Self::render_exe_start_process("$installerPath", installer.silent_args.as_str());

        match installer.installer_type {
            InstallerType::Msi => format!(
                r#"# Install: {name}
Write-Log "Installing {name} (MSI)..."
Write-AppProgress -CurrentItem "{name}" -State "active" -Message "Installing {name}"
try {{
    $installerPath = "{path}"
    $extraArgs = "{args}".Trim()

    if ($installerPath -match "^https?://") {{
        Write-Log "Downloading installer from $installerPath..."
        $localPath = "$env:TEMP\{name_safe}.msi"
        Invoke-WebRequest -Uri $installerPath -OutFile $localPath -UseBasicParsing
        $installerPath = $localPath
    }}

    $arguments = "/i `"$installerPath`""
    if ($extraArgs.Length -gt 0) {{
        $arguments = "$arguments $extraArgs"
    }}
    $process = Start-Process "msiexec.exe" -ArgumentList $arguments -Wait -PassThru -NoNewWindow
    $successCodes = @({success_codes})

    if (Test-ReturnCode -Code $process.ExitCode -SuccessCodes $successCodes) {{
        Write-Log "{name} installed successfully (Exit code: $($process.ExitCode))" "SUCCESS"
        Write-AppProgress -CurrentItem "{name}" -State "complete" -Message "Installed {name}" -IncrementCompleted
    }} else {{
        Write-Log "{name} installation failed with exit code: $($process.ExitCode)" "ERROR"
        $InstallErrors += "custom:{name}"
        Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
    }}
}} catch {{
    Write-Log "Error installing {name}: $_" "ERROR"
    $InstallErrors += "custom:{name}"
    Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
}}

"#
            ),
            InstallerType::Msix => format!(
                r#"# Install: {name}
Write-Log "Installing {name} (MSIX)..."
Write-AppProgress -CurrentItem "{name}" -State "active" -Message "Installing {name}"
try {{
    $installerPath = "{path}"
    $msixArgs = "{args}".Trim()

    if ($installerPath -match "^https?://") {{
        Write-Log "Downloading installer from $installerPath..."
        $localPath = "$env:TEMP\{name_safe}.msix"
        Invoke-WebRequest -Uri $installerPath -OutFile $localPath -UseBasicParsing
        $installerPath = $localPath
    }}

    if ($msixArgs.Length -gt 0) {{
        $command = "Add-AppxPackage -Path `"$installerPath`" $msixArgs"
        Invoke-Expression $command
    }} else {{
        Add-AppxPackage -Path $installerPath
    }}
    $exitCode = if ($?) {{ 0 }} else {{ 1 }}
    $successCodes = @({success_codes})

    if (Test-ReturnCode -Code $exitCode -SuccessCodes $successCodes) {{
        Write-Log "{name} installed successfully (Exit code: $exitCode)" "SUCCESS"
        Write-AppProgress -CurrentItem "{name}" -State "complete" -Message "Installed {name}" -IncrementCompleted
    }} else {{
        Write-Log "{name} installation failed with exit code: $exitCode" "ERROR"
        $InstallErrors += "custom:{name}"
        Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
    }}
}} catch {{
    Write-Log "Error installing {name}: $_" "ERROR"
    $InstallErrors += "custom:{name}"
    Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
}}

"#
            ),
            InstallerType::Msp => format!(
                r#"# Install: {name}
Write-Log "Installing {name} (MSP)..."
Write-AppProgress -CurrentItem "{name}" -State "active" -Message "Installing {name}"
try {{
    $installerPath = "{path}"
    $extraArgs = "{args}".Trim()

    if ($installerPath -match "^https?://") {{
        Write-Log "Downloading installer from $installerPath..."
        $localPath = "$env:TEMP\{name_safe}.msp"
        Invoke-WebRequest -Uri $installerPath -OutFile $localPath -UseBasicParsing
        $installerPath = $localPath
    }}

    $arguments = "/p `"$installerPath`""
    if ($extraArgs.Length -gt 0) {{
        $arguments = "$arguments $extraArgs"
    }}
    $process = Start-Process "msiexec.exe" -ArgumentList $arguments -Wait -PassThru -NoNewWindow
    $successCodes = @({success_codes})

    if (Test-ReturnCode -Code $process.ExitCode -SuccessCodes $successCodes) {{
        Write-Log "{name} installed successfully (Exit code: $($process.ExitCode))" "SUCCESS"
        Write-AppProgress -CurrentItem "{name}" -State "complete" -Message "Installed {name}" -IncrementCompleted
    }} else {{
        Write-Log "{name} installation failed with exit code: $($process.ExitCode)" "ERROR"
        $InstallErrors += "custom:{name}"
        Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
    }}
}} catch {{
    Write-Log "Error installing {name}: $_" "ERROR"
    $InstallErrors += "custom:{name}"
    Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
}}

"#
            ),
            InstallerType::Exe => format!(
                r#"# Install: {name}
Write-Log "Installing {name} (EXE)..."
Write-AppProgress -CurrentItem "{name}" -State "active" -Message "Installing {name}"
try {{
    $installerPath = "{path}"

    if ($installerPath -match "^https?://") {{
        Write-Log "Downloading installer from $installerPath..."
        $localPath = "$env:TEMP\{name_safe}.exe"
        Invoke-WebRequest -Uri $installerPath -OutFile $localPath -UseBasicParsing
        $installerPath = $localPath
    }}

    $process = {exe_start_process}
    $successCodes = @({success_codes})

    if (Test-ReturnCode -Code $process.ExitCode -SuccessCodes $successCodes) {{
        Write-Log "{name} installed successfully (Exit code: $($process.ExitCode))" "SUCCESS"
        Write-AppProgress -CurrentItem "{name}" -State "complete" -Message "Installed {name}" -IncrementCompleted
    }} else {{
        Write-Log "{name} installation failed with exit code: $($process.ExitCode)" "ERROR"
        $InstallErrors += "custom:{name}"
        Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
    }}
}} catch {{
    Write-Log "Error installing {name}: $_" "ERROR"
    $InstallErrors += "custom:{name}"
    Write-AppProgress -CurrentItem "{name}" -State "error" -Message "Failed to install {name}" -IncrementCompleted
}}

"#,
                exe_start_process = exe_start_process
            ),
        }
    }

    fn generate_deferred_network_script(
        installers: &[&CustomInstaller],
        continue_on_error: bool,
    ) -> String {
        let mut script = format!(
            r#"# BitOSDT Deferred Network Installer Script
$ErrorActionPreference = "Continue"
$LogPath = "C:\BitOSDT\Logs\app-install-network.log"
$InstallErrors = @()
$MountedShares = @{{}}
$ContinueOnError = ${continue_on_error}

if (-not (Test-Path (Split-Path $LogPath -Parent))) {{
    New-Item -Path (Split-Path $LogPath -Parent) -ItemType Directory -Force | Out-Null
}}

function Write-Log {{
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logLine = "$timestamp [$Level] $Message"
    $logLine | Out-File -Append $LogPath
    Write-Host $logLine
}}

function Test-ReturnCode {{
    param([int]$Code, [int[]]$SuccessCodes = @(0, 3010))
    return $SuccessCodes -contains $Code
}}

function Assert-AdminSession {{
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {{
        throw "BitOSDT deferred network installers require an administrator session."
    }}
}}

function Get-ShareRoot {{
    param([string]$UncPath)
    if ($UncPath -notmatch '^\\\\[^\\]+\\[^\\]+') {{
        throw "Invalid UNC path: $UncPath"
    }}
    return $matches[0]
}}

function Get-AvailableDriveLetter {{
    foreach ($candidate in @("Z","Y","X","W","V","U","T","S","R","Q","P")) {{
        if (-not (Get-PSDrive -Name $candidate -ErrorAction SilentlyContinue)) {{
            return $candidate
        }}
    }}
    throw "No free drive letters available for temporary network mapping."
}}

function Ensure-MappedShare {{
    param([string]$ShareRoot)
    if ($MountedShares.ContainsKey($ShareRoot)) {{
        return
    }}

    $credential = Get-Credential -Message "Enter credentials for $ShareRoot"
    if ($null -eq $credential) {{
        throw "No credential supplied for $ShareRoot"
    }}

    $driveLetter = Get-AvailableDriveLetter
    New-PSDrive -Name $driveLetter -PSProvider FileSystem -Root $ShareRoot -Credential $credential -Scope Script -ErrorAction Stop | Out-Null
    $MountedShares[$ShareRoot] = "${{driveLetter}}:"
    Write-Log "Mapped $ShareRoot to drive $driveLetter."
}}

function Resolve-NetworkInstallerPath {{
    param(
        [string]$DirectoryPath,
        [string]$FileName
    )

    if ([string]::IsNullOrWhiteSpace($FileName)) {{
        throw "Installer filename is required for network directory installs."
    }}

    $shareRoot = Get-ShareRoot -UncPath $DirectoryPath
    Ensure-MappedShare -ShareRoot $shareRoot
    $drivePath = $MountedShares[$shareRoot]
    $relativePath = $DirectoryPath.Substring($shareRoot.Length).TrimStart('\')
    $mappedDirectory = if ([string]::IsNullOrWhiteSpace($relativePath)) {{ "$drivePath\" }} else {{ Join-Path "$drivePath\" $relativePath }}
    $installerPath = Join-Path $mappedDirectory $FileName

    if (-not (Test-Path $installerPath)) {{
        throw "Installer not found at $installerPath"
    }}

    return $installerPath
}}

function Cleanup-MappedShares {{
    foreach ($entry in $MountedShares.GetEnumerator()) {{
        $drive = $entry.Value.Replace(":", "")
        try {{
            Remove-PSDrive -Name $drive -Scope Script -Force -ErrorAction SilentlyContinue
            Write-Log "Removed temporary mapping for $($entry.Key)."
        }} catch {{
            Write-Log "Failed to remove mapped drive ${{drive}}: $_" "WARN"
        }}
    }}
}}

Write-Log "Starting deferred network installer execution..."

try {{
    Assert-AdminSession
"#,
            continue_on_error = if continue_on_error { "true" } else { "false" }
        );

        for installer in installers {
            for dependency in &installer.dependencies {
                let label = format!(
                    "{} dependency {}",
                    installer.name,
                    Self::payload_display_name(dependency)
                );
                script.push_str(&Self::generate_payload_copy_block(
                    dependency,
                    installer.dependency_destination.as_deref(),
                    continue_on_error,
                    &label,
                    &format!("custom:{}:dependency", installer.name),
                    "    ",
                ));
            }
            script.push_str(&Self::generate_deferred_network_install_block(installer));
        }

        script.push_str(
            r#"
    if ($InstallErrors.Count -gt 0) {
        Write-Log "Deferred network installer execution finished with $($InstallErrors.Count) failure(s)." "ERROR"
        foreach ($entry in $InstallErrors) {
            Write-Log "  - $entry" "ERROR"
        }
        exit 1
    }

    $runOncePath = "HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce"
    Remove-ItemProperty -Path $runOncePath -Name "BitOSDTNetworkInstallers" -ErrorAction SilentlyContinue
    Write-Log "Deferred network installers completed successfully." "SUCCESS"
    exit 0
} catch {
    Write-Log "Deferred network installer execution failed: $_" "ERROR"
    exit 1
} finally {
    Cleanup-MappedShares
}
"#,
        );

        script
    }

    fn generate_deferred_network_install_block(installer: &CustomInstaller) -> String {
        let name = Self::escape_ps_double_quoted(&installer.name);
        let path = Self::escape_ps_double_quoted(&installer.path);
        let args = Self::escape_ps_double_quoted(&installer.silent_args);
        let file_name = Self::escape_ps_double_quoted(
            installer
                .source_file_name
                .as_deref()
                .unwrap_or_default()
                .trim(),
        );
        let success_codes = Self::render_success_codes(&installer.success_codes);
        let exe_start_process =
            Self::render_exe_start_process("$installerPath", installer.silent_args.as_str());

        match installer.installer_type {
            InstallerType::Msi => format!(
                r#"
    # Deferred network install: {name}
    try {{
        $installerPath = Resolve-NetworkInstallerPath -DirectoryPath "{path}" -FileName "{file_name}"
        $extraArgs = "{args}".Trim()
        $arguments = "/i `"$installerPath`""
        if ($extraArgs.Length -gt 0) {{
            $arguments = "$arguments $extraArgs"
        }}
        $process = Start-Process "msiexec.exe" -ArgumentList $arguments -Wait -PassThru -NoNewWindow
        $successCodes = @({success_codes})
        if (Test-ReturnCode -Code $process.ExitCode -SuccessCodes $successCodes) {{
            Write-Log "{name} installed successfully (Exit code: $($process.ExitCode))." "SUCCESS"
        }} else {{
            Write-Log "{name} failed with exit code $($process.ExitCode)." "ERROR"
            $InstallErrors += "custom:{name}"
            if (-not $ContinueOnError) {{ throw "{name} failed." }}
        }}
    }} catch {{
        Write-Log "Error installing {name}: $_" "ERROR"
        $InstallErrors += "custom:{name}"
        if (-not $ContinueOnError) {{ throw }}
    }}
"#
            ),
            InstallerType::Msix => format!(
                r#"
    # Deferred network install: {name}
    try {{
        $installerPath = Resolve-NetworkInstallerPath -DirectoryPath "{path}" -FileName "{file_name}"
        $msixArgs = "{args}".Trim()
        if ($msixArgs.Length -gt 0) {{
            $command = "Add-AppxPackage -Path `"$installerPath`" $msixArgs"
            Invoke-Expression $command
        }} else {{
            Add-AppxPackage -Path $installerPath
        }}
        $exitCode = if ($?) {{ 0 }} else {{ 1 }}
        $successCodes = @({success_codes})
        if (Test-ReturnCode -Code $exitCode -SuccessCodes $successCodes) {{
            Write-Log "{name} installed successfully (Exit code: $exitCode)." "SUCCESS"
        }} else {{
            Write-Log "{name} failed with exit code $exitCode." "ERROR"
            $InstallErrors += "custom:{name}"
            if (-not $ContinueOnError) {{ throw "{name} failed." }}
        }}
    }} catch {{
        Write-Log "Error installing {name}: $_" "ERROR"
        $InstallErrors += "custom:{name}"
        if (-not $ContinueOnError) {{ throw }}
    }}
"#
            ),
            InstallerType::Msp => format!(
                r#"
    # Deferred network install: {name}
    try {{
        $installerPath = Resolve-NetworkInstallerPath -DirectoryPath "{path}" -FileName "{file_name}"
        $extraArgs = "{args}".Trim()
        $arguments = "/p `"$installerPath`""
        if ($extraArgs.Length -gt 0) {{
            $arguments = "$arguments $extraArgs"
        }}
        $process = Start-Process "msiexec.exe" -ArgumentList $arguments -Wait -PassThru -NoNewWindow
        $successCodes = @({success_codes})
        if (Test-ReturnCode -Code $process.ExitCode -SuccessCodes $successCodes) {{
            Write-Log "{name} installed successfully (Exit code: $($process.ExitCode))." "SUCCESS"
        }} else {{
            Write-Log "{name} failed with exit code $($process.ExitCode)." "ERROR"
            $InstallErrors += "custom:{name}"
            if (-not $ContinueOnError) {{ throw "{name} failed." }}
        }}
    }} catch {{
        Write-Log "Error installing {name}: $_" "ERROR"
        $InstallErrors += "custom:{name}"
        if (-not $ContinueOnError) {{ throw }}
    }}
"#
            ),
            InstallerType::Exe => format!(
                r#"
    # Deferred network install: {name}
    try {{
        $installerPath = Resolve-NetworkInstallerPath -DirectoryPath "{path}" -FileName "{file_name}"
        $process = {exe_start_process}
        $successCodes = @({success_codes})
        if (Test-ReturnCode -Code $process.ExitCode -SuccessCodes $successCodes) {{
            Write-Log "{name} installed successfully (Exit code: $($process.ExitCode))." "SUCCESS"
        }} else {{
            Write-Log "{name} failed with exit code $($process.ExitCode)." "ERROR"
            $InstallErrors += "custom:{name}"
            if (-not $ContinueOnError) {{ throw "{name} failed." }}
        }}
    }} catch {{
        Write-Log "Error installing {name}: $_" "ERROR"
        $InstallErrors += "custom:{name}"
        if (-not $ContinueOnError) {{ throw }}
    }}
"#,
                exe_start_process = exe_start_process
            ),
        }
    }

    fn render_success_codes(success_codes: &[i32]) -> String {
        let rendered = success_codes
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");

        if rendered.is_empty() {
            "0, 3010".to_string()
        } else {
            rendered
        }
    }

    fn render_exe_start_process(installer_path_expr: &str, args: &str) -> String {
        if args.trim().is_empty() {
            format!(
                "Start-Process -FilePath {} -Wait -PassThru -NoNewWindow",
                installer_path_expr
            )
        } else {
            let escaped_args = Self::escape_ps_double_quoted(args);
            format!(
                "Start-Process -FilePath {} -ArgumentList \"{}\" -Wait -PassThru -NoNewWindow",
                installer_path_expr, escaped_args
            )
        }
    }

    fn sanitize_name_for_temp(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        if sanitized.is_empty() {
            "installer".to_string()
        } else {
            sanitized
        }
    }

    fn escape_ps_double_quoted(value: &str) -> String {
        value.replace('`', "``").replace('"', "`\"")
    }

    fn payload_display_name(payload: &LocalPayloadItem) -> String {
        if let Some(display_name) = payload.display_name.as_deref() {
            let trimmed = display_name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        Self::payload_leaf_name(&payload.source_path)
    }

    fn payload_leaf_name(path: &str) -> String {
        let normalized = path.replace('\\', "/");
        Path::new(&normalized)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "payload".to_string())
    }

    fn normalized_destination_root(destination: Option<&str>) -> String {
        let normalized = destination
            .unwrap_or("")
            .trim()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_string();

        if normalized.is_empty() {
            r"C:\BitOSDT\Files".to_string()
        } else {
            normalized
        }
    }

    fn generate_payload_copy_block(
        payload: &LocalPayloadItem,
        destination: Option<&str>,
        continue_on_error: bool,
        label: &str,
        error_tag: &str,
        indent: &str,
    ) -> String {
        let source_path = Self::escape_ps_double_quoted(&payload.source_path);
        let destination_root =
            Self::escape_ps_double_quoted(&Self::normalized_destination_root(destination));
        let label = Self::escape_ps_double_quoted(label);
        let error_tag = Self::escape_ps_double_quoted(error_tag);

        match payload.source_kind {
            LocalPayloadKind::Directory => format!(
                r#"{indent}# Copy payload: {label}
{indent}try {{
{indent}    $sourcePath = "{source_path}"
{indent}    $destinationRoot = "{destination_root}"
{indent}    $targetPath = Join-Path $destinationRoot "{leaf_name}"
{indent}    if (-not (Test-Path -LiteralPath $sourcePath)) {{
{indent}        throw "Payload source not found: $sourcePath"
{indent}    }}
{indent}    if (-not (Test-Path -LiteralPath $destinationRoot)) {{
{indent}        New-Item -Path $destinationRoot -ItemType Directory -Force | Out-Null
{indent}    }}
{indent}    if ($sourcePath.TrimEnd('\') -ieq $targetPath.TrimEnd('\')) {{
{indent}        Write-Log "Payload {label} already staged at $targetPath; skipping copy." "INFO"
{indent}    }} else {{
{indent}        Copy-Item -LiteralPath $sourcePath -Destination $destinationRoot -Recurse -Force
{indent}        Write-Log "Copied payload {label} to $targetPath" "SUCCESS"
{indent}    }}
{indent}}} catch {{
{indent}    Write-Log "Failed to copy payload {label}: $_" "ERROR"
{indent}    $InstallErrors += "{error_tag}"
{indent}    {failure_handling}
{indent}}}

"#,
                indent = indent,
                source_path = source_path,
                destination_root = destination_root,
                label = label,
                error_tag = error_tag,
                leaf_name =
                    Self::escape_ps_double_quoted(&Self::payload_leaf_name(&payload.source_path,)),
                failure_handling = if continue_on_error {
                    "# Continue on error"
                } else {
                    "throw"
                }
            ),
            LocalPayloadKind::File => {
                let file_name =
                    Self::escape_ps_double_quoted(&Self::payload_leaf_name(&payload.source_path));
                format!(
                    r#"{indent}# Copy payload: {label}
{indent}try {{
{indent}    $sourcePath = "{source_path}"
{indent}    $destinationRoot = "{destination_root}"
{indent}    $targetPath = Join-Path $destinationRoot "{file_name}"
{indent}    if (-not (Test-Path -LiteralPath $sourcePath)) {{
{indent}        throw "Payload source not found: $sourcePath"
{indent}    }}
{indent}    if (-not (Test-Path -LiteralPath $destinationRoot)) {{
{indent}        New-Item -Path $destinationRoot -ItemType Directory -Force | Out-Null
{indent}    }}
{indent}    if ($sourcePath.TrimEnd('\') -ieq $targetPath.TrimEnd('\')) {{
{indent}        Write-Log "Payload {label} already staged at $targetPath; skipping copy." "INFO"
{indent}    }} else {{
{indent}        Copy-Item -LiteralPath $sourcePath -Destination $targetPath -Force
{indent}        Write-Log "Copied payload {label} to $targetPath" "SUCCESS"
{indent}    }}
{indent}}} catch {{
{indent}    Write-Log "Failed to copy payload {label}: $_" "ERROR"
{indent}    $InstallErrors += "{error_tag}"
{indent}    {failure_handling}
{indent}}}

"#,
                    indent = indent,
                    source_path = source_path,
                    destination_root = destination_root,
                    file_name = file_name,
                    label = label,
                    error_tag = error_tag,
                    failure_handling = if continue_on_error {
                        "# Continue on error"
                    } else {
                        "throw"
                    }
                )
            }
        }
    }

    fn script_footer() -> String {
        r#"
# ================================================
# INSTALLATION SUMMARY
# ================================================

Write-Log "Application installation completed."
Write-AppProgress -CurrentItem "Completed" -State "complete" -Message "Application installation finished"

if ($InstallErrors.Count -gt 0) {
    Write-Log "The following installations had errors:" "WARNING"
    foreach ($error in $InstallErrors) {
        Write-Log "  - $error" "WARNING"
    }
    Write-Log "Total errors: $($InstallErrors.Count)" "WARNING"
} else {
    Write-Log "All applications installed successfully!" "SUCCESS"
}

# Return exit code based on errors
if ($InstallErrors.Count -gt 0) {
    exit 1
} else {
    exit 0
}
"#
        .to_string()
    }

    /// Generate a simple winget-only installation script
    pub fn generate_winget_only_script(packages: &[WingetPackage]) -> String {
        let mut script = r#"# Winget Installation Script
$ErrorActionPreference = "Continue"

"#
        .to_string();

        for pkg in packages.iter().filter(|p| p.enabled) {
            let version = pkg
                .version
                .as_ref()
                .map(|v| format!(" --version \"{}\"", v))
                .unwrap_or_default();

            script.push_str(&format!(
                "winget install --id \"{}\" --silent --accept-source-agreements --accept-package-agreements{}\n",
                pkg.package_id, version
            ));
        }

        script
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_install_script() {
        let config = AppInstallConfig {
            winget_packages: vec![WingetPackage {
                package_id: "Microsoft.VisualStudioCode".to_string(),
                version: None,
                custom_args: None,
                enabled: true,
            }],
            chocolatey_packages: vec![ChocolateyPackage {
                package_name: "googlechrome".to_string(),
                version: None,
                source: None,
                custom_args: None,
                enabled: true,
            }],
            custom_installers: vec![CustomInstaller {
                name: "MyApp".to_string(),
                path: "C:\\Installers\\myapp.msi".to_string(),
                source_type: InstallerSourceType::DirectPathOrUrl,
                source_file_name: None,
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "/qn /norestart".to_string(),
                installer_type: InstallerType::Msi,
                success_codes: vec![0, 3010],
                enabled: true,
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(script.contains("Microsoft.VisualStudioCode"));
        assert!(script.contains("googlechrome"));
        assert!(script.contains("MyApp"));
        assert!(script.contains("msiexec"));
    }

    #[test]
    fn test_winget_system_deferral_present() {
        let config = AppInstallConfig {
            winget_packages: vec![WingetPackage {
                package_id: "Microsoft.VisualStudioCode".to_string(),
                version: None,
                custom_args: None,
                enabled: true,
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(script.contains("S-1-5-18"));
        assert!(script.contains("Install-WingetApps.ps1"));
        assert!(script.contains("BitOSDTWingetInstallers"));
    }

    #[test]
    fn test_chocolatey_bootstrap_uses_scriptblock_create() {
        let config = AppInstallConfig {
            chocolatey_packages: vec![ChocolateyPackage {
                package_name: "googlechrome".to_string(),
                version: None,
                source: None,
                custom_args: None,
                enabled: true,
            }],
            auto_install_chocolatey: true,
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(script.contains("[ScriptBlock]::Create($downloadedScript)"));
        assert!(!script.contains(
            "Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))"
        ));
    }

    #[test]
    fn test_winget_install_does_not_assign_result_variable() {
        let config = AppInstallConfig {
            winget_packages: vec![WingetPackage {
                package_id: "Microsoft.VisualStudioCode".to_string(),
                version: None,
                custom_args: None,
                enabled: true,
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(!script.contains("$result = winget install"));
        assert!(script.contains(
            "winget install --id \"Microsoft.VisualStudioCode\" --silent --accept-source-agreements --accept-package-agreements"
        ));
    }

    #[test]
    fn test_network_installers_are_deferred_with_runonce() {
        let config = AppInstallConfig {
            custom_installers: vec![CustomInstaller {
                name: "Network App".to_string(),
                path: r"\\fileserver\apps\office".to_string(),
                source_type: InstallerSourceType::NetworkDirectory,
                source_file_name: Some("setup.exe".to_string()),
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "/quiet".to_string(),
                installer_type: InstallerType::Exe,
                success_codes: vec![0],
                enabled: true,
            }],
            continue_on_error: false,
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(script.contains("Install-NetworkApps.ps1"));
        assert!(script.contains("BitOSDTNetworkInstallers"));
        assert!(script.contains("Get-Credential -Message"));
        assert!(script.contains("Resolve-NetworkInstallerPath"));
        assert!(script.contains("$ContinueOnError = $false"));
    }

    #[test]
    fn test_exe_installer_without_silent_args_omits_argument_list() {
        let config = AppInstallConfig {
            custom_installers: vec![CustomInstaller {
                name: "Plain EXE".to_string(),
                path: r"C:\Installers\plain.exe".to_string(),
                source_type: InstallerSourceType::DirectPathOrUrl,
                source_file_name: None,
                dependencies: vec![],
                dependency_destination: None,
                silent_args: String::new(),
                installer_type: InstallerType::Exe,
                success_codes: vec![0],
                enabled: true,
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(script
            .contains(r#"Start-Process -FilePath $installerPath -Wait -PassThru -NoNewWindow"#));
        assert!(!script.contains(
            r#"Start-Process -FilePath $installerPath -ArgumentList "" -Wait -PassThru -NoNewWindow"#
        ));
    }

    #[test]
    fn test_deferred_exe_installer_without_silent_args_omits_argument_list() {
        let config = AppInstallConfig {
            custom_installers: vec![CustomInstaller {
                name: "Deferred Plain EXE".to_string(),
                path: r"\\fileserver\apps\plain".to_string(),
                source_type: InstallerSourceType::NetworkDirectory,
                source_file_name: Some("setup.exe".to_string()),
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "   ".to_string(),
                installer_type: InstallerType::Exe,
                success_codes: vec![0],
                enabled: true,
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(script
            .contains(r#"Start-Process -FilePath $installerPath -Wait -PassThru -NoNewWindow"#));
        assert!(!script.contains(
            r#"Start-Process -FilePath $installerPath -ArgumentList "" -Wait -PassThru -NoNewWindow"#
        ));
    }

    #[test]
    fn test_msix_uses_add_appxpackage() {
        let config = AppInstallConfig {
            custom_installers: vec![CustomInstaller {
                name: "Modern App".to_string(),
                path: "C:\\Installers\\modern.msix".to_string(),
                source_type: InstallerSourceType::DirectPathOrUrl,
                source_file_name: None,
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "-ForceApplicationShutdown".to_string(),
                installer_type: InstallerType::Msix,
                success_codes: vec![0],
                enabled: true,
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();
        assert!(script.contains("Add-AppxPackage -Path"));
    }

    #[test]
    fn test_generate_winget_only() {
        let packages = vec![
            WingetPackage {
                package_id: "Microsoft.VisualStudioCode".to_string(),
                version: Some("1.85.0".to_string()),
                custom_args: None,
                enabled: true,
            },
            WingetPackage {
                package_id: "Google.Chrome".to_string(),
                version: None,
                custom_args: None,
                enabled: true,
            },
        ];

        let script = AppInstaller::generate_winget_only_script(&packages);

        assert!(script.contains("Microsoft.VisualStudioCode"));
        assert!(script.contains("--version \"1.85.0\""));
        assert!(script.contains("Google.Chrome"));
    }

    #[test]
    fn test_disabled_packages_excluded() {
        let config = AppInstallConfig {
            winget_packages: vec![WingetPackage {
                package_id: "DisabledApp".to_string(),
                version: None,
                custom_args: None,
                enabled: false, // Disabled
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        // Should not contain the disabled app (winget section shouldn't be generated at all)
        assert!(!script.contains("DisabledApp"));
    }

    #[test]
    fn test_payload_copy_section_defaults_to_bitosdt_files() {
        let config = AppInstallConfig {
            copied_items: vec![
                LocalPayloadItem {
                    source_path: r"C:\Staging\config.json".to_string(),
                    source_kind: LocalPayloadKind::File,
                    display_name: None,
                },
                LocalPayloadItem {
                    source_path: r"C:\Staging\Support".to_string(),
                    source_kind: LocalPayloadKind::Directory,
                    display_name: None,
                },
            ],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        assert!(script.contains(r#"$destinationRoot = "C:\BitOSDT\Files""#));
        assert!(script.contains(r#"Join-Path $destinationRoot "config.json""#));
        assert!(script.contains(r#"Join-Path $destinationRoot "Support""#));
    }

    #[test]
    fn test_custom_installer_dependencies_are_copied_before_install() {
        let config = AppInstallConfig {
            custom_installers: vec![CustomInstaller {
                name: "My App".to_string(),
                path: r"C:\Installers\myapp.exe".to_string(),
                source_type: InstallerSourceType::DirectPathOrUrl,
                source_file_name: None,
                dependencies: vec![LocalPayloadItem {
                    source_path: r"C:\Payloads\dependency.dll".to_string(),
                    source_kind: LocalPayloadKind::File,
                    display_name: Some("dependency.dll".to_string()),
                }],
                dependency_destination: Some(r"C:\Program Files\Vendor".to_string()),
                silent_args: "/quiet".to_string(),
                installer_type: InstallerType::Exe,
                success_codes: vec![0],
                enabled: true,
            }],
            ..Default::default()
        };

        let script = AppInstaller::generate_install_script(&config).unwrap();

        let copy_pos = script
            .find("Copy payload: My App dependency dependency.dll")
            .unwrap();
        let install_pos = script.find("Installing My App (EXE)").unwrap();
        assert!(copy_pos < install_pos);
        assert!(script.contains(r#"$destinationRoot = "C:\Program Files\Vendor""#));
    }
}
