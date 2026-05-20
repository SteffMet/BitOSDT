use crate::build::{
    full_iso_builder::preprocess_task_sequence_for_full_iso,
    runtime_drivers::{stage_runtime_driver_assets, RuntimeDriverAssetConfig},
    winpe_ui::{
        resolve_winpe_compat_spoof_enabled, write_hta_mode_config, write_hta_shell,
        write_initial_status, write_kiosk_helper, write_shell_launcher_cmd,
        write_winpe_compat_spoof_assets, write_winpeshl_ini, WinPEUiMode,
    },
    FileInjection, IsoCreator, RuntimeDomainJoinConfig, WinPEBuilder,
};
use crate::config::{AutopilotProfile, UnattendConfig};
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::{DriverPack, RuntimeDriverConfig, RuntimeDriverContext, RuntimeDriverPolicy};
use crate::tasks::TaskSequence;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

/// Lightweight ISO builder for network-boot deployments
pub struct LightweightBuilder {
    work_dir: PathBuf,
    winpe_builder: WinPEBuilder,
}

/// Configuration for lightweight ISO
#[derive(Debug, Clone)]
pub struct LightweightConfig {
    /// Network server URL for downloading deployment resources
    pub server_url: String,
    /// Include BitOSDT GUI executable
    pub include_gui: bool,
    /// BitOSDT executable path (if include_gui is true)
    pub gui_executable: Option<PathBuf>,
    /// Additional scripts to include
    pub scripts: Vec<PathBuf>,
    /// Target Windows OS version for runtime driver matching
    pub os_version: String,
    /// Driver paths to include in WinPE
    pub driver_paths: Vec<PathBuf>,
    /// Optional shared native BitOSDT executable path
    pub native_executable: Option<PathBuf>,
    /// Optional Linux WinPE asset bundle root
    pub winpe_assets_dir: Option<PathBuf>,
    /// Optional curated common boot-driver bundle
    pub common_boot_driver_dir: Option<PathBuf>,
    /// Optional offline driver cache directory to stage into media/publish output
    pub driver_cache_dir: Option<PathBuf>,
    /// Runtime driver policy embedded into WinPE
    pub runtime_driver_policy: RuntimeDriverPolicy,
    /// Runtime driver catalog snapshot embedded into WinPE
    pub runtime_driver_catalog: Vec<DriverPack>,
    /// Optional deployment image URL reachable from WinPE
    pub http_image_url: Option<String>,
    /// Optional UNC path reachable from WinPE
    pub unc_image_path: Option<String>,
    /// WIM index to apply from the prepared single-image WIM
    pub wim_index: u32,
    /// Selected source image index from the original WIM/ESD when known
    pub source_image_index: Option<u32>,
    /// Requested Windows edition used to resolve the source image index
    pub windows_edition: String,
    /// Unattend configuration to inject at runtime
    pub unattend: UnattendConfig,
    /// Autopilot profile to inject at runtime
    pub autopilot: Option<AutopilotProfile>,
    /// Task sequence to inject at runtime
    pub task_sequence: Option<TaskSequence>,
    /// Runtime domain join defaults for WinPE prompting
    pub runtime_domain_join: Option<RuntimeDomainJoinConfig>,
    /// Output ISO path
    pub output_path: PathBuf,
    /// Volume label
    pub volume_label: String,
    /// Enable PowerShell in WinPE
    pub enable_powershell: bool,
    /// Network timeout in seconds
    pub network_timeout: u32,
    /// Optional directory containing WinPE packages copied to X:\BitOSDT\Packages
    pub winpe_packages_dir: Option<PathBuf>,
    /// Optional directory containing the compiled React UI copied to X:\BitOSDT\UI
    pub ui_dir: Option<PathBuf>,
}

impl Default for LightweightConfig {
    fn default() -> Self {
        Self {
            server_url: "http://deploy.local:8080".to_string(),
            include_gui: false,
            gui_executable: None,
            scripts: Vec::new(),
            os_version: "24H2".to_string(),
            driver_paths: Vec::new(),
            native_executable: None,
            winpe_assets_dir: None,
            common_boot_driver_dir: None,
            driver_cache_dir: None,
            runtime_driver_policy: RuntimeDriverPolicy::default(),
            runtime_driver_catalog: Vec::new(),
            http_image_url: None,
            unc_image_path: None,
            wim_index: 1,
            source_image_index: None,
            windows_edition: "Pro".to_string(),
            unattend: UnattendConfig::default(),
            autopilot: None,
            task_sequence: None,
            runtime_domain_join: None,
            output_path: PathBuf::from("BitOSDT-Lightweight.iso"),
            volume_label: "BITOSDT".to_string(),
            enable_powershell: true,
            network_timeout: 60,
            winpe_packages_dir: None,
            ui_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LightweightDeployConfig {
    mode: String,
    os_version: String,
    server_url: String,
    wim_index: u32,
    source_image_index: Option<u32>,
    windows_edition: String,
    http_image_url: Option<String>,
    unc_image_path: Option<String>,
    runtime_driver_policy: RuntimeDriverPolicy,
    runtime_driver_context: RuntimeDriverContext,
    unattend: UnattendConfig,
    autopilot: Option<AutopilotProfile>,
    task_sequence: Option<TaskSequence>,
    runtime_domain_join: Option<RuntimeDomainJoinConfig>,
    inject_files: Vec<FileInjection>,
}

fn build_lightweight_deploy_config(
    config: &LightweightConfig,
    runtime_driver_context: RuntimeDriverContext,
    task_sequence: Option<TaskSequence>,
    inject_files: Vec<FileInjection>,
) -> LightweightDeployConfig {
    LightweightDeployConfig {
        mode: "lightweight".to_string(),
        os_version: config.os_version.clone(),
        server_url: config.server_url.clone(),
        wim_index: config.wim_index.max(1),
        source_image_index: config.source_image_index,
        windows_edition: config.windows_edition.clone(),
        http_image_url: config.http_image_url.clone(),
        unc_image_path: config.unc_image_path.clone(),
        runtime_driver_policy: config.runtime_driver_policy.clone(),
        runtime_driver_context,
        unattend: config.unattend.clone(),
        autopilot: config.autopilot.clone(),
        task_sequence,
        runtime_domain_join: config.runtime_domain_join.clone(),
        inject_files,
    }
}

fn copy_directory_recursive(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> BitOSDTResult<()> {
    if !source.exists() {
        return Ok(());
    }

    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory_recursive(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

fn stage_runtime_injections_in_winpe(
    mount_dir: &std::path::Path,
    inject_files: Vec<FileInjection>,
) -> BitOSDTResult<Vec<FileInjection>> {
    let payload_root = mount_dir.join("BitOSDT").join("Payloads");
    fs::create_dir_all(&payload_root)?;

    let mut staged = Vec::with_capacity(inject_files.len());
    for (index, injection) in inject_files.into_iter().enumerate() {
        if !injection.source.is_file() {
            return Err(BitOSDTError::InvalidInput(format!(
                "Runtime payload source not found: {}",
                injection.source.display()
            )));
        }

        let file_name = injection
            .source
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("payload-{}", index + 1));
        let staged_name = format!("{:03}-{}", index + 1, file_name);
        let staged_path = payload_root.join(staged_name);
        fs::copy(&injection.source, &staged_path)?;

        staged.push(FileInjection {
            source: PathBuf::from(format!(
                r"X:\BitOSDT\Payloads\{}",
                staged_path.file_name().unwrap().to_string_lossy()
            )),
            destination: injection.destination,
        });
    }

    Ok(staged)
}

impl LightweightBuilder {
    pub fn new(work_dir: PathBuf) -> BitOSDTResult<Self> {
        fs::create_dir_all(&work_dir)?;
        let winpe_builder = WinPEBuilder::new(work_dir.clone(), "amd64".to_string());

        Ok(Self {
            work_dir,
            winpe_builder,
        })
    }

    /// Build a lightweight network-boot ISO
    pub fn build<F>(
        &mut self,
        config: &LightweightConfig,
        mut progress_callback: F,
    ) -> BitOSDTResult<PathBuf>
    where
        F: FnMut(u8, String),
    {
        info!("Building lightweight ISO: {:?}", config.output_path);

        // Initialize WinPE builder
        progress_callback(5, "Initializing WinPE builder...".to_string());
        self.winpe_builder
            .initialize_with_assets(None, config.winpe_assets_dir.as_deref())?;

        // Create WinPE base
        progress_callback(10, "Creating WinPE base...".to_string());
        let winpe_dir = self.winpe_builder.create_winpe()?;
        if let Some(driver_cache_dir) = config.driver_cache_dir.as_ref() {
            let media_runtime_cache_dir =
                winpe_dir.join("media").join("BitOSDT").join("DriverCache");
            copy_directory_recursive(driver_cache_dir, &media_runtime_cache_dir)?;
        }

        // Mount boot.wim for customization
        let boot_wim = winpe_dir.join("media").join("sources").join("boot.wim");
        let mount_dir = self.work_dir.join("mount");

        progress_callback(20, "Mounting WinPE for customization...".to_string());
        self.winpe_builder.mount_wim(&boot_wim, &mount_dir)?;

        // Enable PowerShell if requested
        if config.enable_powershell {
            progress_callback(30, "Enabling PowerShell in WinPE...".to_string());
            // Note: This requires ADK path - the builder should have it
            #[cfg(target_os = "windows")]
            {
                if self.winpe_builder.adk_paths().is_some() {
                    self.winpe_builder
                        .enable_powershell_for_language(&mount_dir, "en-us")?;
                }
            }
        }
        self.winpe_builder
            .enable_extended_components_for_language(&mount_dir, "en-us")?;
        if !self
            .winpe_builder
            .enable_hta_for_language(&mount_dir, "en-us")?
            && self.winpe_builder.adk_paths().is_some()
        {
            return Err(BitOSDTError::WinPE(
                "WinPE-HTA optional component is missing for language 'en-us'. Install the Windows ADK WinPE-HTA package and rebuild."
                    .to_string(),
            ));
        }

        // Add drivers
        if config.runtime_driver_policy.bundle_common_boot_drivers {
            if let Some(common_boot_driver_dir) = config.common_boot_driver_dir.as_ref() {
                if common_boot_driver_dir.exists() {
                    progress_callback(38, "Adding common boot drivers...".to_string());
                    self.winpe_builder
                        .add_drivers(&mount_dir, common_boot_driver_dir)?;
                }
            }
        }
        if !config.driver_paths.is_empty() {
            progress_callback(40, "Adding drivers...".to_string());
            for driver_path in &config.driver_paths {
                self.winpe_builder.add_drivers(&mount_dir, driver_path)?;
            }
        }

        // Create BitOSDT directory structure in WinPE
        progress_callback(50, "Creating BitOSDT directory structure...".to_string());
        let bitosdt_dir = mount_dir.join("BitOSDT");
        fs::create_dir_all(&bitosdt_dir)?;
        fs::create_dir_all(bitosdt_dir.join("Logs"))?;
        fs::create_dir_all(bitosdt_dir.join("Scripts"))?;
        fs::create_dir_all(bitosdt_dir.join("Config"))?;
        fs::create_dir_all(bitosdt_dir.join("State"))?;
        fs::create_dir_all(bitosdt_dir.join("UI"))?;
        write_initial_status(&mount_dir, WinPEUiMode::Lightweight)?;
        write_winpe_compat_spoof_assets(&mount_dir, resolve_winpe_compat_spoof_enabled())?;
        let (processed_task_sequence, runtime_inject_files) =
            preprocess_task_sequence_for_full_iso(config.task_sequence.as_ref())?;
        let staged_runtime_inject_files =
            stage_runtime_injections_in_winpe(&mount_dir, runtime_inject_files)?;
        let mut runtime_driver_context = RuntimeDriverContext::winpe_default();
        runtime_driver_context.cache_download_base_url = Some(format!(
            "{}/BitOSDT/DriverCache",
            config.server_url.trim_end_matches('/')
        ));
        if config.common_boot_driver_dir.is_some() {
            runtime_driver_context.common_boot_driver_directory =
                Some(PathBuf::from(r"X:\BitOSDT\DriverCache\common-boot"));
        }
        stage_runtime_driver_assets(
            &mount_dir,
            &self.winpe_builder,
            &RuntimeDriverAssetConfig {
                policy: config.runtime_driver_policy.clone(),
                context: runtime_driver_context.clone(),
                catalog: config.runtime_driver_catalog.clone(),
                cache_source: config.driver_cache_dir.clone(),
            },
        )?;

        if let Some(packages_dir) = config.winpe_packages_dir.as_ref() {
            progress_callback(55, "Copying WinPE packages...".to_string());
            let dest_packages = mount_dir.join("BitOSDT").join("Packages");
            fs::create_dir_all(&dest_packages)?;

            let has_sciter = packages_dir.join("sciter").is_dir();

            if let Ok(entries) = fs::read_dir(packages_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy().to_lowercase();

                    if path.is_dir() {
                        // Skip legacy browser packages if sciter is present
                        if has_sciter && (name_str == "chrome" || name_str == "supermium") {
                            continue;
                        }
                        let dest_subdir = format!(r"BitOSDT\Packages\{}", name.to_string_lossy());
                        self.winpe_builder
                            .add_files(&mount_dir, &path, &dest_subdir)?;
                    } else if path.is_file() {
                        let _ = fs::copy(&path, dest_packages.join(&name));
                    }
                }
            }

            self.winpe_builder
                .inject_vc_runtime_dlls_from_dir(&mount_dir, packages_dir)?;

            let custom_fonts_dir = packages_dir.join("fonts");
            if custom_fonts_dir.exists() {
                let _ = self
                    .winpe_builder
                    .inject_custom_fonts_from_dir(&mount_dir, &custom_fonts_dir)?;
            }
        }

        if let Some(ui_dir) = config.ui_dir.as_ref() {
            progress_callback(57, "Copying BitOSDT Web UI...".to_string());
            self.winpe_builder
                .add_files(&mount_dir, ui_dir, r"BitOSDT\UI")?;
        }

        // Copy GUI executable if provided
        if config.include_gui {
            if let Some(ref gui_path) = config.gui_executable {
                if gui_path.exists() {
                    progress_callback(60, "Copying BitOSDT executable...".to_string());
                    fs::copy(gui_path, bitosdt_dir.join("bitosdt.exe"))?;
                } else {
                    warn!("GUI executable not found: {:?}", gui_path);
                }
            }
        }
        if let Some(native_executable) = config.native_executable.as_ref() {
            if native_executable.exists() && !bitosdt_dir.join("bitosdt.exe").exists() {
                fs::copy(native_executable, bitosdt_dir.join("bitosdt.exe"))?;
            }
        }
        let prefer_native_runtime = !cfg!(target_os = "windows")
            || config
                .runtime_domain_join
                .as_ref()
                .is_some_and(|value| value.prompt_for_credentials_at_runtime);
        if prefer_native_runtime && !bitosdt_dir.join("bitosdt.exe").exists() {
            return Err(BitOSDTError::Validation(
                "Linux-built Lightweight ISO media requires a WinPE-native bitosdt.exe in the asset bundle."
                    .to_string(),
            ));
        }

        // Create network bootstrap script
        progress_callback(65, "Creating runtime launch files...".to_string());
        if !prefer_native_runtime {
            let bootstrap_script = Self::generate_bootstrap_script(
                &config.server_url,
                config.network_timeout,
                config.include_gui && config.gui_executable.is_some(),
            );
            fs::write(
                bitosdt_dir.join("Scripts").join("Start-BitOSDT.ps1"),
                bootstrap_script,
            )?;
            fs::write(
                bitosdt_dir.join("Scripts").join("Launch-BitOSDT-WinPE.ps1"),
                Self::generate_shell_launcher_script(),
            )?;

            let fallback_cmd = concat!(
                "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"X:\\BitOSDT\\Scripts\\Start-BitOSDT.ps1\"\r\n",
                "if %ERRORLEVEL% NEQ 0 cmd /k"
            );
            write_hta_mode_config(&mount_dir)?;
            write_shell_launcher_cmd(&mount_dir, fallback_cmd)?;
        }

        // Generate and inject startnet.cmd
        progress_callback(70, "Generating startnet.cmd...".to_string());
        let startnet_content = Self::generate_startnet(prefer_native_runtime);
        let startnet_path = mount_dir
            .join("Windows")
            .join("System32")
            .join("startnet.cmd");
        if let Some(parent) = startnet_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(startnet_path, startnet_content)?;
        if !prefer_native_runtime {
            write_hta_shell(&mount_dir)?;
            write_kiosk_helper(&mount_dir)?;
        }
        write_winpeshl_ini(&mount_dir)?;

        // Copy additional scripts
        for script in &config.scripts {
            if script.exists() {
                if let Some(filename) = script.file_name() {
                    fs::copy(script, bitosdt_dir.join("Scripts").join(filename))?;
                }
            }
        }

        // Create server configuration file
        let server_config = format!(
            r#"{{
    "server_url": "{}",
    "network_timeout": {},
    "auto_start": true
}}"#,
            config.server_url, config.network_timeout
        );
        fs::write(
            bitosdt_dir.join("Config").join("server.json"),
            server_config,
        )?;
        fs::write(
            bitosdt_dir.join("Config").join("runtime-drivers.json"),
            serde_json::to_string_pretty(&RuntimeDriverConfig {
                os_version: config.os_version.clone(),
                runtime_driver_policy: config.runtime_driver_policy.clone(),
                runtime_driver_context: runtime_driver_context.clone(),
            })?,
        )?;
        let deploy_config = build_lightweight_deploy_config(
            config,
            runtime_driver_context,
            processed_task_sequence,
            staged_runtime_inject_files,
        );
        fs::write(
            bitosdt_dir.join("Config").join("deploy.json"),
            serde_json::to_string_pretty(&deploy_config)?,
        )?;

        // Unmount WinPE
        progress_callback(75, "Committing WinPE changes...".to_string());
        self.winpe_builder.unmount_wim(&mount_dir, true)?;

        // Create ISO
        progress_callback(85, "Creating ISO file...".to_string());
        let media_dir = winpe_dir.join("media");

        // Ensure output directory exists
        if let Some(parent) = config.output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        IsoCreator::create_iso(&media_dir, &config.output_path, &config.volume_label)?;

        progress_callback(100, "Lightweight ISO created successfully!".to_string());
        info!("Lightweight ISO created: {:?}", config.output_path);

        Ok(config.output_path.clone())
    }

    /// Generate the network bootstrap PowerShell script
    fn generate_bootstrap_script(server_url: &str, timeout: u32, prefer_local_exe: bool) -> String {
        format!(
            r#"# BitOSDT Network Bootstrap Script
# Downloads and executes BitOSDT from network server

param(
    [string]$ServerUrl = "{server_url}",
    [int]$NetworkTimeout = {timeout}
)

$ProgressPreference = 'SilentlyContinue'
$ErrorActionPreference = 'Stop'
$LogPath = "X:\BitOSDT\Logs\deploy.log"
$StatusPath = "X:\BitOSDT\State\deploy-status.json"
$RuntimeDriverConfigPath = "X:\BitOSDT\Config\runtime-drivers.json"

if (-not (Test-Path (Split-Path -Parent $LogPath))) {{
    New-Item -Path (Split-Path -Parent $LogPath) -ItemType Directory -Force | Out-Null
}}
if (-not (Test-Path (Split-Path -Parent $StatusPath))) {{
    New-Item -Path (Split-Path -Parent $StatusPath) -ItemType Directory -Force | Out-Null
}}

function Write-Log {{
    param([string]$Message, [string]$Level = "INFO")
    $stamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    "$stamp [$Level] $Message" | Out-File -FilePath $LogPath -Append -Encoding utf8
    Write-Host "[$Level] $Message"
}}

function Write-Status {{
    param(
        [int]$StageIndex,
        [int]$PercentComplete,
        [string]$StatusText,
        [string]$DetailText,
        [bool]$IsError = $false,
        [string]$ErrorMessage = $null
    )

    try {{
        $payload = @{{
            schema_version = 1
            mode = "lightweight"
            stage_index = $StageIndex
            stage_total = 4
            percent_complete = $PercentComplete
            status_text = $StatusText
            detail_text = $DetailText
            last_updated_utc = (Get-Date).ToUniversalTime().ToString("o")
            is_error = $IsError
            error_message = $ErrorMessage
        }} | ConvertTo-Json -Depth 4
        $tmpPath = "$StatusPath.tmp"
        Set-Content -Path $tmpPath -Value $payload -Encoding utf8
        Move-Item -Path $tmpPath -Destination $StatusPath -Force
    }} catch {{
        Write-Log "Status update failed: $_" "WARN"
    }}
}}

Write-Host @"
===========================================
   BitOSDT Network Boot
===========================================
Server: $ServerUrl
"@ -ForegroundColor Cyan
Write-Log "BitOSDT lightweight bootstrap started."
Write-Status -StageIndex 1 -PercentComplete 2 -StatusText "Preparing WinPE..." -DetailText "Waiting for network connectivity."

# Wait for network connectivity
Write-Host "Waiting for network connection..." -ForegroundColor Yellow
$timer = 0
while (!(Test-Connection -ComputerName "1.1.1.1" -Count 1 -Quiet -ErrorAction SilentlyContinue)) {{
    if ($timer -ge $NetworkTimeout) {{
        Write-Error "Network timeout after $NetworkTimeout seconds"
        Write-Status -StageIndex 1 -PercentComplete 100 -StatusText "Network timeout" -DetailText "Unable to obtain connectivity in WinPE." -IsError $true -ErrorMessage "Network timeout"
        Read-Host "Press Enter to continue to command prompt"
        exit 1
    }}
    Start-Sleep -Seconds 1
    $timer++
    Write-Progress -Activity "Waiting for network" -Status "$timer seconds" -PercentComplete (($timer / $NetworkTimeout) * 100)
}}
Write-Progress -Activity "Waiting for network" -Completed
Write-Host "Network connected!" -ForegroundColor Green
Write-Status -StageIndex 1 -PercentComplete 20 -StatusText "Preparing WinPE..." -DetailText "Network connection established."

# Check server connectivity with retry
Write-Host "Connecting to deployment server..." -ForegroundColor Yellow
Write-Status -StageIndex 2 -PercentComplete 30 -StatusText "Preparing download..." -DetailText "Checking deployment server availability."
$healthAttempts = 0
$healthMaxAttempts = 30
$healthConnected = $false
while ($healthAttempts -lt $healthMaxAttempts) {{
    try {{
        $response = Invoke-WebRequest -Uri "$ServerUrl/health" -UseBasicParsing -TimeoutSec 5
        Write-Host "Server connected!" -ForegroundColor Green
        Write-Log "Deployment server health check succeeded."
        $healthConnected = $true
        break
    }} catch {{
        $healthAttempts++
        Write-Log "Server health check attempt $healthAttempts/$healthMaxAttempts failed: $_" "WARN"
        Start-Sleep -Seconds 2
    }}
}}
if (-not $healthConnected) {{
    Write-Warning "Server health check failed after $healthMaxAttempts attempts"
    Write-Host "Attempting to continue anyway..." -ForegroundColor Yellow
    Write-Log "Deployment server health check failed after $healthMaxAttempts attempts" "WARN"
}}

# Download deployment manifest
Write-Host "Downloading deployment manifest..." -ForegroundColor Yellow
$manifestPath = "X:\BitOSDT\Config\manifest.json"
Write-Status -StageIndex 2 -PercentComplete 45 -StatusText "Downloading payload..." -DetailText "Downloading deployment manifest."
try {{
    Invoke-WebRequest -Uri "$ServerUrl/api/manifest" -OutFile $manifestPath -UseBasicParsing
    $manifest = Get-Content $manifestPath | ConvertFrom-Json
    Write-Host "Manifest downloaded: $($manifest.name)" -ForegroundColor Green
    Write-Log "Manifest downloaded: $($manifest.name)"
}} catch {{
    Write-Warning "Failed to download manifest: $_"
    $manifest = $null
    Write-Log "Manifest download failed: $_" "WARN"
}}

# Resolve executable source
$exePath = "X:\BitOSDT\bitosdt.exe"
$useLocalExe = $false
if ({prefer_local_exe} -and (Test-Path $exePath)) {{
    $useLocalExe = $true
    Write-Log "Using embedded BitOSDT executable at $exePath"
}} else {{
    Write-Status -StageIndex 2 -PercentComplete 65 -StatusText "Downloading payload..." -DetailText "Downloading BitOSDT runtime executable."
    Write-Host "Downloading BitOSDT executable..." -ForegroundColor Yellow
    try {{
        Invoke-WebRequest -Uri "$ServerUrl/download/bitosdt.exe" -OutFile $exePath -UseBasicParsing
        Write-Host "Download complete!" -ForegroundColor Green
        Write-Log "BitOSDT executable downloaded."
    }} catch {{
        Write-Error "Failed to download BitOSDT: $_"
        Write-Status -StageIndex 2 -PercentComplete 100 -StatusText "Download failed" -DetailText "Unable to download BitOSDT runtime." -IsError $true -ErrorMessage "$_"
        Read-Host "Press Enter to continue to command prompt"
        exit 1
    }}
}}

# Verify download
if (-not (Test-Path $exePath)) {{
    Write-Error "BitOSDT executable not found after download"
    Write-Status -StageIndex 2 -PercentComplete 100 -StatusText "Runtime missing" -DetailText "BitOSDT executable is unavailable." -IsError $true -ErrorMessage "Executable missing"
    Read-Host "Press Enter to continue to command prompt"
    exit 1
}}

# Launch BitOSDT
if (Test-Path $RuntimeDriverConfigPath) {{
    Write-Status -StageIndex 3 -PercentComplete 70 -StatusText "Preparing drivers..." -DetailText "Resolving runtime DriverPack for the current hardware."
    try {{
        & $exePath runtime-drivers --config $RuntimeDriverConfigPath --prepare-only --server-url $ServerUrl
        if ($LASTEXITCODE -ne 0) {{
            Write-Log "Runtime driver prepare command exited with code $LASTEXITCODE" "WARN"
        }} else {{
            Write-Log "Runtime driver prepare command completed."
        }}
    }} catch {{
        Write-Log "Runtime driver prepare command failed: $_" "WARN"
    }}
}}

Write-Status -StageIndex 3 -PercentComplete 80 -StatusText "Launching deployment runtime..." -DetailText "Starting BitOSDT executable."
Write-Host "Starting BitOSDT..." -ForegroundColor Cyan
try {{
    $env:BITOSDT_WINPE_FULLSCREEN = "1"
    $runtimeExit = $null

    Write-Status -StageIndex 3 -PercentComplete 82 -StatusText "Launching deployment runtime..." -DetailText "Attempt 1: launch with --server argument."
    Write-Log "Launching runtime attempt 1 with --server argument."
    $runtimeProcess = Start-Process -FilePath $exePath -ArgumentList @("--server", $ServerUrl) -WindowStyle Maximized -PassThru
    $runtimeProcess.WaitForExit()
    $runtimeExit = $runtimeProcess.ExitCode
    Write-Log "Runtime attempt 1 exit code: $runtimeExit"

    if ($runtimeExit -ne 0) {{
        Write-Status -StageIndex 3 -PercentComplete 88 -StatusText "Retrying deployment runtime..." -DetailText "Attempt 1 failed (exit $runtimeExit). Retrying without startup arguments."
        Write-Log "Launching runtime attempt 2 without arguments due non-zero exit." "WARN"
        $fallbackProcess = Start-Process -FilePath $exePath -WindowStyle Maximized -PassThru
        $fallbackProcess.WaitForExit()
        $runtimeExit = $fallbackProcess.ExitCode
        Write-Log "Runtime attempt 2 exit code: $runtimeExit"
    }}

    if ($runtimeExit -ne 0) {{
        throw "BitOSDT runtime exited with code $runtimeExit after two launch attempts"
    }}
    Write-Status -StageIndex 4 -PercentComplete 100 -StatusText "Finalizing handoff..." -DetailText "Deployment runtime completed successfully."
    Write-Log "BitOSDT runtime exited successfully."
}} catch {{
    Write-Error "Failed to start BitOSDT: $_"
    Write-Status -StageIndex 4 -PercentComplete 100 -StatusText "Runtime failed" -DetailText "Deployment runtime failed to start or exited in error." -IsError $true -ErrorMessage "$_"
    Write-Log "BitOSDT runtime failed: $_" "ERROR"
    exit 1
}}

Write-Host "BitOSDT session ended" -ForegroundColor Yellow
Write-Log "BitOSDT session ended."
"#,
            server_url = server_url,
            timeout = timeout,
            prefer_local_exe = if prefer_local_exe { "$true" } else { "$false" }
        )
    }

    fn generate_shell_launcher_script() -> String {
        r#"param(
    [string]$ScriptPath = "X:\BitOSDT\Scripts\Start-BitOSDT.ps1"
)

$ErrorActionPreference = 'Continue'
if (-not (Test-Path $ScriptPath)) {
    Write-Host "Bootstrap script not found at $ScriptPath"
    exit 1
}

& powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$ScriptPath"
exit $LASTEXITCODE
"#
        .replace('\n', "\r\n")
    }

    fn generate_startnet(prefer_native_runtime: bool) -> String {
        if prefer_native_runtime {
            return r#"@echo off
setlocal EnableDelayedExpansion
echo Starting BitOSDT lightweight deployment...

wpeinit

set DEPLOY_EXE=X:\BitOSDT\bitosdt.exe
set DEPLOY_CONFIG=X:\BitOSDT\Config\deploy.json
set RUNTIME_DRIVER_CONFIG=X:\BitOSDT\Config\runtime-drivers.json
set STARTNET_LOG=X:\BitOSDT\Logs\startnet.log

if not exist "X:\BitOSDT\Logs" (
    mkdir "X:\BitOSDT\Logs" >nul 2>&1
)
echo [%DATE% %TIME%] lightweight startnet.cmd initialized for native runtime>>"%STARTNET_LOG%"

if not exist "%DEPLOY_EXE%" (
    echo.
    echo Native BitOSDT runtime missing: "%DEPLOY_EXE%"
    echo [%DATE% %TIME%] Native runtime missing at "%DEPLOY_EXE%".>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

if not exist "%DEPLOY_CONFIG%" (
    echo.
    echo Lightweight deployment config missing: "%DEPLOY_CONFIG%"
    echo [%DATE% %TIME%] Deployment config missing at "%DEPLOY_CONFIG%".>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

echo [%DATE% %TIME%] Invoking native lightweight runtime.>>"%STARTNET_LOG%"
"%DEPLOY_EXE%" winpe-deploy --config "%DEPLOY_CONFIG%" --runtime-driver-config "%RUNTIME_DRIVER_CONFIG%"
set DEPLOY_EXIT=!ERRORLEVEL!
if !DEPLOY_EXIT! NEQ 0 (
    echo.
    echo Lightweight deployment failed. Review logs at X:\BitOSDT\Logs\deploy.log
    echo [%DATE% %TIME%] Native lightweight runtime failed with exit code !DEPLOY_EXIT!.>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)
goto :eof
"#
            .replace('\n', "\r\n");
        }

        r#"@echo off
setlocal EnableDelayedExpansion
echo Starting BitOSDT lightweight deployment...

:: Initialize WinPE hardware and networking first.
wpeinit

set COMPAT_FLAG=X:\BitOSDT\Config\enable-winpe-compat-spoof.flag
set COMPAT_SCRIPT=X:\BitOSDT\Scripts\Set-WinPE-CompatibilitySpoof.ps1
if exist "%COMPAT_FLAG%" (
    if exist "%COMPAT_SCRIPT%" (
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%COMPAT_SCRIPT%" -Mode Apply >nul 2>&1
    )
)

:: Launch BitOSDT.
set WRAPPER=X:\BitOSDT\Scripts\Launch-BitOSDT-WinPE.cmd
set BOOTSTRAP_SCRIPT=X:\BitOSDT\Scripts\Start-BitOSDT.ps1

if exist "X:\BitOSDT\Scripts\Launch-BitOSDT-WinPE.cmd" (
    call "%WRAPPER%"
    if !ERRORLEVEL! EQU 0 goto :eof
    echo Shell wrapper returned error !ERRORLEVEL!. Running fallback bootstrap script...
) else (
    echo Shell wrapper missing at "%WRAPPER%". Running fallback bootstrap script...
)

if not exist "%BOOTSTRAP_SCRIPT%" (
    echo.
    echo Lightweight bootstrap script missing: "%BOOTSTRAP_SCRIPT%"
    cmd /k
    goto :eof
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%BOOTSTRAP_SCRIPT%"
set DEPLOY_EXIT=!ERRORLEVEL!
if !DEPLOY_EXIT! NEQ 0 (
    echo.
    echo Lightweight deployment failed. Review logs at X:\BitOSDT\Logs\deploy.log
    cmd /k
    goto :eof
)
goto :eof
"#
        .replace('\n', "\r\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FileInjection;
    use crate::core::RuntimeDriverContext;
    use crate::tasks::{RebootConfig, TaskDefinition, TaskSequence, TaskSettings, TaskType};
    use tempfile::tempdir;
    use uuid::Uuid;

    fn first_script_executable_line(script: &str) -> Option<&str> {
        for line in script.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                return Some(trimmed);
            }
        }

        None
    }

    #[test]
    fn test_default_config() {
        let config = LightweightConfig::default();
        assert_eq!(config.volume_label, "BITOSDT");
        assert!(config.enable_powershell);
        assert!(!config.include_gui);
        assert!(config.winpe_packages_dir.is_none());
    }

    #[test]
    fn test_bootstrap_script_generation() {
        let script =
            LightweightBuilder::generate_bootstrap_script("http://test.local:8080", 30, false);
        assert!(script.contains("http://test.local:8080"));
        assert!(script.contains("NetworkTimeout = 30"));
        assert!(script.contains("function Write-Status"));
        assert!(script.contains("mode = \"lightweight\""));
        assert!(script.contains("runtime-drivers"));
        assert!(script.contains("--prepare-only"));
        assert!(script.contains("runtime-drivers.json"));
        assert!(script.contains("Launching runtime attempt 1 with --server argument."));
        assert!(script.contains("Launching runtime attempt 2 without arguments due non-zero exit."));
        assert!(script
            .contains("Attempt 1 failed (exit $runtimeExit). Retrying without startup arguments."));
        assert!(script.contains("after two launch attempts"));
        assert_eq!(first_script_executable_line(&script), Some("param("));
    }

    #[test]
    fn test_shell_startnet_prefers_wrapper() {
        let startnet = LightweightBuilder::generate_startnet(false);
        assert!(startnet.contains("set WRAPPER=X:\\BitOSDT\\Scripts\\Launch-BitOSDT-WinPE.cmd"));
        assert!(startnet
            .contains("set COMPAT_FLAG=X:\\BitOSDT\\Config\\enable-winpe-compat-spoof.flag"));
        assert!(startnet.contains("Set-WinPE-CompatibilitySpoof.ps1"));
        assert!(startnet.contains("set BOOTSTRAP_SCRIPT=X:\\BitOSDT\\Scripts\\Start-BitOSDT.ps1"));
        assert!(startnet.contains("Shell wrapper missing at \"%WRAPPER%\""));
        assert!(startnet.contains("if not exist \"%BOOTSTRAP_SCRIPT%\""));
        assert!(!startnet.contains("loading.html"));
        assert!(!startnet.contains("scapp.exe"));
        assert!(startnet.contains("goto :eof"));
    }

    #[test]
    fn test_native_lightweight_startnet_uses_winpe_deploy() {
        let startnet = LightweightBuilder::generate_startnet(true);
        assert!(startnet.contains("set DEPLOY_EXE=X:\\BitOSDT\\bitosdt.exe"));
        assert!(startnet.contains("winpe-deploy --config"));
        assert!(startnet.contains("runtime-drivers.json"));
        assert!(!startnet.contains("Start-BitOSDT.ps1"));
    }

    #[test]
    fn test_copy_packages_into_winpe_destination() {
        let temp = tempdir().expect("temp dir");
        let mount_dir = temp.path().join("mount");
        let packages_dir = temp.path().join("Packages");
        fs::create_dir_all(packages_dir.join("tools")).expect("create package dir");
        fs::write(packages_dir.join("tools").join("utility.exe"), b"dummy").expect("write utility");

        let builder = LightweightBuilder::new(temp.path().join("workspace"))
            .expect("create lightweight builder");
        builder
            .winpe_builder
            .add_files(&mount_dir, &packages_dir, r"BitOSDT\Packages")
            .expect("copy packages");

        assert!(mount_dir
            .join("BitOSDT")
            .join("Packages")
            .join("tools")
            .join("utility.exe")
            .exists());
    }

    #[test]
    fn test_build_lightweight_deploy_config_preserves_source_image_index_and_edition() {
        let mut config = LightweightConfig::default();
        config.wim_index = 0;
        config.source_image_index = Some(4);
        config.windows_edition = "Education".to_string();
        config.http_image_url = Some("https://cdn.example.test/fr-fr.esd".to_string());
        config.unc_image_path = Some(r"\\server\share\install.wim".to_string());
        config.runtime_domain_join = Some(RuntimeDomainJoinConfig {
            enabled: true,
            prompt_for_credentials_at_runtime: true,
            default_domain: Some("contoso.local".to_string()),
            default_ou_path: Some("OU=Devices,DC=contoso,DC=local".to_string()),
        });

        let task_sequence = Some(TaskSequence {
            id: Uuid::new_v4(),
            name: "Deploy".to_string(),
            tasks: vec![TaskDefinition {
                id: Uuid::new_v4(),
                name: "Example".to_string(),
                task_type: TaskType::Reboot(RebootConfig {
                    delay_seconds: 0,
                    message: None,
                    force: false,
                }),
                order: 1,
                enabled: true,
                continue_on_error: false,
                requires_reboot: false,
            }],
            settings: TaskSettings {
                scripts_dir: r"C:\BitOSDT\Scripts".to_string(),
                logs_dir: r"C:\BitOSDT\Logs".to_string(),
                continue_on_error: false,
                create_completion_marker: true,
            },
        });
        let inject_files = vec![FileInjection {
            source: PathBuf::from(r"C:\BitOSDT\payload.txt"),
            destination: r"Windows\Temp\payload.txt".to_string(),
        }];

        let deploy_config = build_lightweight_deploy_config(
            &config,
            RuntimeDriverContext::default(),
            task_sequence.clone(),
            inject_files.clone(),
        );
        let serialized =
            serde_json::to_value(&deploy_config).expect("serialize lightweight deploy config");

        assert_eq!(deploy_config.wim_index, 1);
        assert_eq!(deploy_config.source_image_index, Some(4));
        assert_eq!(deploy_config.windows_edition, "Education");
        assert_eq!(
            deploy_config
                .task_sequence
                .as_ref()
                .map(|value| value.name.as_str()),
            Some("Deploy")
        );
        assert_eq!(deploy_config.inject_files.len(), 1);
        assert_eq!(serialized["source_image_index"], 4);
        assert_eq!(serialized["windows_edition"], "Education");
        assert_eq!(
            serialized["http_image_url"],
            "https://cdn.example.test/fr-fr.esd"
        );
        assert_eq!(serialized["unc_image_path"], r"\\server\share\install.wim");
        assert_eq!(
            serialized["runtime_domain_join"]["default_domain"],
            "contoso.local"
        );
    }
}
