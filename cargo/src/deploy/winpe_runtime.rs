#[cfg(target_os = "windows")]
use super::wim::{describe_available_images, resolve_requested_edition_image};
#[cfg(target_os = "windows")]
use super::{prepare_runtime_drivers, BootManager, DiskManager, WimManager};
use crate::build::winpe_ui::{
    WinPEStatus, WinPEUiMode, LOG_FILE_WINPE_PATH, STATUS_FILE_WINPE_PATH,
};
use crate::build::DiskSelectionPolicy;
use crate::build::FileInjection;
use crate::build::RuntimeDomainJoinConfig;
#[cfg(target_os = "windows")]
use crate::build::{ImagePrepConfig, ImagePreparer};
#[cfg(target_os = "windows")]
use crate::config::DomainJoinConfig;
#[cfg(target_os = "windows")]
use crate::config::{AutopilotGenerator, UnattendGenerator};
use crate::config::{AutopilotProfile, UnattendConfig};
use crate::core::errors::{BitOSDTError, BitOSDTResult};
#[cfg(target_os = "windows")]
use crate::core::RuntimeDriverConfig;
use crate::core::{RuntimeDriverContext, RuntimeDriverPolicy};
use crate::download::HashValidator;
#[cfg(target_os = "windows")]
use crate::download::{EsdDownloader, EsdInfo};
#[cfg(target_os = "windows")]
use crate::tasks::TaskRunner;
use crate::tasks::TaskSequence;
use chrono::Utc;
#[cfg(target_os = "windows")]
use reqwest::StatusCode;
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use windows::core::{PCWSTR, PWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::NetworkManagement::WNet::{
    WNetAddConnection2W, WNetCancelConnection2W, CONNECT_TEMPORARY, NETRESOURCEW, RESOURCETYPE_DISK,
};

#[derive(Debug, Clone)]
pub struct WinpeDeployOptions {
    pub log_path: PathBuf,
    pub status_path: PathBuf,
    pub runtime_driver_config_path: Option<PathBuf>,
    pub skip_reboot: bool,
}

impl Default for WinpeDeployOptions {
    fn default() -> Self {
        Self {
            log_path: PathBuf::from(LOG_FILE_WINPE_PATH),
            status_path: PathBuf::from(STATUS_FILE_WINPE_PATH),
            runtime_driver_config_path: Some(PathBuf::from(
                r"X:\BitOSDT\Config\runtime-drivers.json",
            )),
            skip_reboot: false,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct FullIsoDeployConfig {
    mode: String,
    os_version: String,
    wim_index: u32,
    target_disk: Option<u32>,
    #[serde(default)]
    disk_selection_policy: DiskSelectionPolicy,
    #[serde(default)]
    runtime_driver_policy: RuntimeDriverPolicy,
    #[serde(default)]
    runtime_driver_context: RuntimeDriverContext,
    unc_image_path: Option<String>,
    unc_auth_username: Option<String>,
    unc_auth_password: Option<String>,
    http_image_url: Option<String>,
    expected_payload_size_bytes: Option<u64>,
    expected_payload_sha256: Option<String>,
    expected_payload_file_name: Option<String>,
    unattend: UnattendConfig,
    autopilot: Option<AutopilotProfile>,
    task_sequence: Option<TaskSequence>,
    runtime_domain_join: Option<RuntimeDomainJoinConfig>,
    #[serde(default)]
    prompt_unc_credentials_at_runtime: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct LightweightDeployConfig {
    mode: String,
    os_version: String,
    server_url: String,
    wim_index: u32,
    source_image_index: Option<u32>,
    windows_edition: String,
    http_image_url: Option<String>,
    unc_image_path: Option<String>,
    unc_auth_username: Option<String>,
    unc_auth_password: Option<String>,
    #[serde(default)]
    runtime_driver_policy: RuntimeDriverPolicy,
    #[serde(default)]
    runtime_driver_context: RuntimeDriverContext,
    unattend: UnattendConfig,
    autopilot: Option<AutopilotProfile>,
    task_sequence: Option<TaskSequence>,
    runtime_domain_join: Option<RuntimeDomainJoinConfig>,
    #[serde(default)]
    inject_files: Vec<FileInjection>,
    #[serde(default)]
    prompt_unc_credentials_at_runtime: Option<bool>,
}

#[allow(dead_code)]
struct WinpeReporter {
    log_path: PathBuf,
    status_path: PathBuf,
    mode: WinPEUiMode,
}

#[cfg(target_os = "windows")]
fn stage_full_iso_first_boot_assets(
    windows_partition: &Path,
    unattend: &UnattendConfig,
    autopilot: Option<&AutopilotProfile>,
    task_sequence: Option<&TaskSequence>,
) -> BitOSDTResult<()> {
    let windows_dir = windows_partition.join("Windows");
    let panther_dir = windows_dir.join("Panther");
    let panther_unattend_dir = panther_dir.join("Unattend");
    let setup_scripts_dir = windows_dir.join("Setup").join("Scripts");

    fs::create_dir_all(&panther_dir)?;
    fs::create_dir_all(&panther_unattend_dir)?;
    fs::create_dir_all(&setup_scripts_dir)?;

    let unattend_content = UnattendGenerator::generate(unattend)?;
    fs::write(panther_dir.join("unattend.xml"), &unattend_content)?;
    fs::write(panther_dir.join("Unattend.xml"), &unattend_content)?;
    fs::write(panther_unattend_dir.join("Unattend.xml"), &unattend_content)?;

    if let Some(profile) = autopilot {
        let autopilot_dir = windows_dir.join("Provisioning").join("Autopilot");
        AutopilotGenerator::save_configuration(profile, &autopilot_dir)?;
    }

    if let Some(sequence) = task_sequence {
        TaskRunner::write_task_files(sequence, &setup_scripts_dir)?;
        let files = TaskRunner::generate_task_files(sequence)?;
        if let Some(setup_complete) = files.get("SetupComplete.cmd") {
            fs::write(setup_scripts_dir.join("SetupComplete.cmd"), setup_complete)?;
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_prompted_domain_join_to_unattend(
    unattend: &mut UnattendConfig,
    prompted: crate::build::provisioning_ui::DomainCredentialPromptResult,
) {
    let ou_path = prompted.ou_path.clone();
    unattend.domain_join = Some(DomainJoinConfig {
        domain: prompted.domain,
        username: prompted.username,
        password: prompted.password,
        ou_path: ou_path.clone(),
        machine_object_ou: ou_path,
    });
}

#[cfg(target_os = "windows")]
fn apply_runtime_domain_join_prompt(
    unattend: &mut UnattendConfig,
    runtime_domain_join: Option<&RuntimeDomainJoinConfig>,
    reporter: &WinpeReporter,
) -> BitOSDTResult<()> {
    let Some(runtime_domain_join) = runtime_domain_join else {
        return Ok(());
    };

    if !runtime_domain_join.enabled || !runtime_domain_join.prompt_for_credentials_at_runtime {
        return Ok(());
    }

    reporter.log(
        "INFO",
        "Domain join is configured to prompt for credentials at runtime. Launching domain prompt...",
    )?;

    let prompted = crate::build::provisioning_ui::launch_domain_credential_prompt(
        runtime_domain_join.default_domain.as_deref(),
        runtime_domain_join.default_ou_path.as_deref(),
    )
    .map_err(BitOSDTError::InvalidInput)?;

    apply_prompted_domain_join_to_unattend(unattend, prompted.clone());

    reporter.log(
        "INFO",
        &format!(
            "Captured runtime domain join details for {}.",
            prompted.domain
        ),
    )?;

    Ok(())
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum RemoteImageAttempt {
    Failed { target: String, reason: String },
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageResolutionDiagnostics {
    local_searched: Vec<String>,
    local_matches: Vec<String>,
    unc: Option<RemoteImageAttempt>,
    http: Option<RemoteImageAttempt>,
}

#[cfg(target_os = "windows")]
struct ResolvedWindowsImage {
    path: PathBuf,
    unc_connection: Option<WinpeUncConnection>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct UncPathParts {
    share_root: String,
    relative_path: String,
}

#[cfg(target_os = "windows")]
struct WinpeUncConnection {
    local_name: String,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn format_image_resolution_failure(diagnostics: &ImageResolutionDiagnostics) -> String {
    let mut details = Vec::new();
    let searched = if diagnostics.local_searched.is_empty() {
        "<none>".to_string()
    } else {
        diagnostics.local_searched.join(", ")
    };

    if diagnostics.local_matches.is_empty() {
        details.push(format!(
            "Local search: no Windows image found. Searched: {}",
            searched
        ));
    } else {
        details.push(format!(
            "Local search: found Windows images: {}",
            diagnostics.local_matches.join(", ")
        ));
    }

    if let Some(RemoteImageAttempt::Failed { target, reason }) = diagnostics.unc.as_ref() {
        details.push(format!(
            "UNC path configured but not accessible from WinPE: {} ({})",
            target, reason
        ));
    }

    if let Some(RemoteImageAttempt::Failed { target, reason }) = diagnostics.http.as_ref() {
        details.push(format!(
            "HTTP download attempted from {} and failed: {}",
            target, reason
        ));
    }

    format!(
        "No accessible Windows image was found in WinPE. {}",
        details.join(" ")
    )
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn build_payload_validation_failure(
    resolved_path: &Path,
    expected_file_name: Option<&str>,
    detail: &str,
) -> BitOSDTError {
    let mut message = format!(
        "WDS/PXE resolved Windows image '{}' does not match the prepared payload exported by BitOSDT. {} Host the exported prepared WIM from C:\\BitOSDT\\WDS at the configured runtime path.",
        resolved_path.display(),
        detail
    );

    if let Some(file_name) = expected_file_name.filter(|value| !value.trim().is_empty()) {
        message.push_str(&format!(" Expected exported file name: {}.", file_name));
    }

    BitOSDTError::Validation(message)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn validate_resolved_payload_provenance(
    resolved_path: &Path,
    expected_size_bytes: Option<u64>,
    expected_sha256: Option<&str>,
    expected_file_name: Option<&str>,
) -> BitOSDTResult<()> {
    let Some(expected_size_bytes) = expected_size_bytes else {
        return Ok(());
    };

    let metadata = fs::metadata(resolved_path).map_err(|e| {
        BitOSDTError::NotFound(format!(
            "Failed to inspect resolved Windows image {}: {}",
            resolved_path.display(),
            e
        ))
    })?;

    let actual_size_bytes = metadata.len();
    if actual_size_bytes != expected_size_bytes {
        return Err(build_payload_validation_failure(
            resolved_path,
            expected_file_name,
            &format!(
                "Size mismatch: expected {} bytes, found {} bytes.",
                expected_size_bytes, actual_size_bytes
            ),
        ));
    }

    if let Some(expected_sha256) = expected_sha256
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let actual_sha256 = HashValidator::calculate_sha256(resolved_path)?;
        if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
            return Err(build_payload_validation_failure(
                resolved_path,
                expected_file_name,
                &format!(
                    "SHA-256 mismatch: expected {}, found {}.",
                    expected_sha256, actual_sha256
                ),
            ));
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn parse_unc_path_parts(unc_path: &str) -> BitOSDTResult<UncPathParts> {
    let trimmed = unc_path.trim();
    if !trimmed.starts_with(r"\\") {
        return Err(BitOSDTError::InvalidInput(format!(
            "UNC path must start with \\\\: {}",
            unc_path
        )));
    }

    let without_prefix = &trimmed[2..];
    let parts: Vec<&str> = without_prefix.split('\\').collect();
    if parts.len() < 3
        || parts[0].trim().is_empty()
        || parts[1].trim().is_empty()
        || parts[2..].iter().any(|segment| segment.trim().is_empty())
    {
        return Err(BitOSDTError::InvalidInput(format!(
            "UNC path must include a server, share, and file path beneath the share: {}",
            unc_path
        )));
    }

    let server = parts[0].trim();
    let share = parts[1].trim();
    let relative_path = parts[2..].join("\\");

    Ok(UncPathParts {
        share_root: format!(r"\\{}\{}", server, share),
        relative_path,
    })
}

#[cfg(target_os = "windows")]
fn map_unc_path_to_local_path(local_name: &str, parts: &UncPathParts) -> PathBuf {
    PathBuf::from(format!(r"{}\{}", local_name, parts.relative_path))
}

#[cfg(target_os = "windows")]
fn pick_available_unc_drive() -> BitOSDTResult<String> {
    for candidate in ["Z:", "Y:", "V:", "U:", "T:", "R:", "Q:", "P:", "O:", "N:"] {
        let root = format!("{}\\", candidate);
        if !Path::new(&root).exists() {
            return Ok(candidate.to_string());
        }
    }

    Err(BitOSDTError::Validation(
        "No free drive letters are available for temporary UNC image mapping.".to_string(),
    ))
}

#[cfg(target_os = "windows")]
impl WinpeUncConnection {
    fn connect(unc_path: &str, username: &str, password: &str) -> BitOSDTResult<(Self, PathBuf)> {
        let parts = parse_unc_path_parts(unc_path)?;
        let local_name = pick_available_unc_drive()?;

        let mut local_name_wide = to_wide(&local_name);
        let mut share_root_wide = to_wide(&parts.share_root);
        let username_wide = to_wide(username);
        let password_wide = to_wide(password);
        let resource = NETRESOURCEW {
            dwType: RESOURCETYPE_DISK,
            lpLocalName: PWSTR(local_name_wide.as_mut_ptr()),
            lpRemoteName: PWSTR(share_root_wide.as_mut_ptr()),
            ..Default::default()
        };

        let status = unsafe {
            WNetAddConnection2W(
                &resource,
                PCWSTR(password_wide.as_ptr()),
                PCWSTR(username_wide.as_ptr()),
                CONNECT_TEMPORARY.0,
            )
        };

        if let Err(error) = status {
            return Err(BitOSDTError::NotFound(format!(
                "Failed to authenticate UNC image share {}: {}",
                parts.share_root, error
            )));
        }

        Ok((
            Self {
                local_name: local_name.clone(),
            },
            map_unc_path_to_local_path(&local_name, &parts),
        ))
    }
}

#[cfg(target_os = "windows")]
impl Drop for WinpeUncConnection {
    fn drop(&mut self) {
        let local_name_wide = to_wide(&self.local_name);
        let _ = unsafe { WNetCancelConnection2W(PCWSTR(local_name_wide.as_ptr()), 0, true) };
    }
}

#[allow(dead_code)]
impl WinpeReporter {
    fn new(log_path: PathBuf, status_path: PathBuf, mode: WinPEUiMode) -> BitOSDTResult<Self> {
        ensure_parent_directory(&log_path)?;
        ensure_parent_directory(&status_path)?;
        Ok(Self {
            log_path,
            status_path,
            mode,
        })
    }

    fn log(&self, level: &str, message: &str) -> BitOSDTResult<()> {
        ensure_parent_directory(&self.log_path)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        writeln!(
            file,
            "{} [{}] {}",
            Utc::now().format("%Y-%m-%d %H:%M:%S"),
            level,
            message
        )?;
        Ok(())
    }

    fn status(
        &self,
        stage_index: u32,
        percent_complete: u32,
        status_text: &str,
        detail_text: &str,
        is_error: bool,
        error_message: Option<String>,
    ) -> BitOSDTResult<WinPEStatus> {
        let status = WinPEStatus {
            schema_version: 1,
            mode: self.mode.as_str().to_string(),
            stage_index,
            stage_total: 4,
            percent_complete,
            status_text: status_text.to_string(),
            detail_text: detail_text.to_string(),
            last_updated_utc: Utc::now().to_rfc3339(),
            is_error,
            error_message,
        };

        ensure_parent_directory(&self.status_path)?;
        let temp_path = self.status_path.with_extension("tmp");
        fs::write(&temp_path, serde_json::to_string_pretty(&status)?)?;
        fs::rename(temp_path, &self.status_path)?;
        Ok(status)
    }
}

pub async fn run_winpe_deploy(
    config_path: &Path,
    options: &WinpeDeployOptions,
    progress_callback: Option<&dyn Fn(WinPEStatus)>,
) -> BitOSDTResult<()> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (config_path, options, progress_callback);
        return Err(BitOSDTError::NotImplemented(
            "Native WinPE deployment is only supported when running inside Windows/WinPE"
                .to_string(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let payload = fs::read_to_string(config_path).map_err(|e| {
            BitOSDTError::NotFound(format!(
                "Failed to read WinPE deploy config {}: {}",
                config_path.display(),
                e
            ))
        })?;
        let raw: serde_json::Value = serde_json::from_str(&payload)?;
        let mode = raw
            .get("mode")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                BitOSDTError::InvalidInput(format!(
                    "WinPE deploy config {} is missing a mode field",
                    config_path.display()
                ))
            })?;

        match mode {
            "full_iso" => {
                let config: FullIsoDeployConfig = serde_json::from_value(raw)?;
                let reporter = WinpeReporter::new(
                    options.log_path.clone(),
                    options.status_path.clone(),
                    WinPEUiMode::FullIso,
                )?;
                run_full_iso_deploy(config, options, &reporter, progress_callback).await
            }
            "lightweight" => {
                let config: LightweightDeployConfig = serde_json::from_value(raw)?;
                let reporter = WinpeReporter::new(
                    options.log_path.clone(),
                    options.status_path.clone(),
                    WinPEUiMode::Lightweight,
                )?;
                run_lightweight_deploy(config, options, &reporter, progress_callback)
                    .await
                    .map(|_| ())
            }
            other => Err(BitOSDTError::InvalidInput(format!(
                "Unsupported WinPE deploy mode '{}' in {}",
                other,
                config_path.display()
            ))),
        }
    }
}

#[cfg(target_os = "windows")]
async fn run_full_iso_deploy(
    config: FullIsoDeployConfig,
    options: &WinpeDeployOptions,
    reporter: &WinpeReporter,
    progress_callback: Option<&dyn Fn(WinPEStatus)>,
) -> BitOSDTResult<()> {
    let mut config = config;
    reporter.log("INFO", "Native WinPE full ISO deployment starting.")?;
    emit_status(
        progress_callback,
        reporter.status(
            1,
            5,
            "Preparing deployment...",
            "Resolving Windows image and target disk.",
            false,
            None,
        )?,
    );

    let resolved_image = resolve_windows_image_path_for_source(
        &config.os_version,
        None,
        config.unc_image_path.as_deref(),
        config.unc_auth_username.as_deref(),
        config.unc_auth_password.as_deref(),
        config.http_image_url.as_deref(),
        reporter,
        config.prompt_unc_credentials_at_runtime,
    )
    .await?;
    let _unc_connection = resolved_image.unc_connection;
    let image_path = resolved_image.path;
    reporter.log(
        "INFO",
        &format!("Resolved Windows image source: {}", image_path.display()),
    )?;
    validate_resolved_payload_provenance(
        &image_path,
        config.expected_payload_size_bytes,
        config.expected_payload_sha256.as_deref(),
        config.expected_payload_file_name.as_deref(),
    )?;
    reporter.log(
        "INFO",
        &format!(
            "Validated resolved Windows image against exported payload provenance: {}",
            image_path.display()
        ),
    )?;

    let firmware_is_uefi = detect_winpe_firmware_mode(reporter)?;
    let target_disk = resolve_target_disk(&config)?;
    reporter.log(
        "INFO",
        &format!(
            "Target disk {} selected with firmware mode {}.",
            target_disk,
            if firmware_is_uefi { "UEFI" } else { "BIOS" }
        ),
    )?;

    emit_status(
        progress_callback,
        reporter.status(
            1,
            20,
            "Preparing deployment...",
            "Partitioning destination disk.",
            false,
            None,
        )?,
    );

    let disk_manager = DiskManager::new(target_disk);
    disk_manager.initialize_disk(firmware_is_uefi)?;

    // Verify partitions are accessible before proceeding
    disk_manager.verify_partitions(firmware_is_uefi)?;
    reporter.log(
        "INFO",
        "Verified that partition drive letters are correctly assigned and accessible.",
    )?;

    let windows_partition = disk_manager.get_windows_partition()?;
    let system_partition = PathBuf::from(r"S:\");

    emit_status(
        progress_callback,
        reporter.status(
            2,
            30,
            "Applying Windows image...",
            "Applying the selected WIM/ESD to the Windows partition.",
            false,
            None,
        )?,
    );

    let wim_manager = WimManager::new();
    let wim_index = config.wim_index.max(1);
    wim_manager.apply_wim(&image_path, wim_index, &windows_partition, None)?;
    reporter.log(
        "INFO",
        &format!(
            "Applied image index {} into {}.",
            wim_index,
            windows_partition.display()
        ),
    )?;

    emit_status(
        progress_callback,
        reporter.status(
            3,
            60,
            "Staging first-boot configuration...",
            "Restaging unattend, Autopilot, and setup scripts onto the deployed Windows partition.",
            false,
            None,
        )?,
    );

    apply_runtime_domain_join_prompt(
        &mut config.unattend,
        config.runtime_domain_join.as_ref(),
        reporter,
    )?;

    stage_full_iso_first_boot_assets(
        &windows_partition,
        &config.unattend,
        config.autopilot.as_ref(),
        config.task_sequence.as_ref(),
    )?;
    reporter.log(
        "INFO",
        "Restaged first-boot configuration assets onto the deployed Windows partition.",
    )?;

    emit_status(
        progress_callback,
        reporter.status(
            3,
            78,
            "Installing drivers...",
            "Running shared runtime driver preparation and offline injection.",
            false,
            None,
        )?,
    );

    run_runtime_driver_stage(
        &config.os_version,
        &config.runtime_driver_policy,
        &config.runtime_driver_context,
        options.runtime_driver_config_path.as_deref(),
        &windows_partition,
        reporter,
    )
    .await?;

    emit_status(
        progress_callback,
        reporter.status(
            3,
            92,
            "Configuring bootloader...",
            "Running BCDBoot for the deployed Windows installation.",
            false,
            None,
        )?,
    );

    let boot_manager = BootManager::new();
    boot_manager.configure_bootloader(
        &windows_partition.join("Windows"),
        &system_partition,
        firmware_is_uefi,
    )?;

    emit_status(
        progress_callback,
        reporter.status(
            4,
            100,
            "Finalizing deployment...",
            if options.skip_reboot {
                "Deployment complete. Reboot skipped by request."
            } else {
                "Deployment complete. Rebooting into Windows."
            },
            false,
            None,
        )?,
    );

    reporter.log(
        "INFO",
        "Native WinPE full ISO deployment completed successfully.",
    )?;

    if options.skip_reboot {
        reporter.log(
            "WARN",
            "Skipping reboot because --skip-reboot was requested.",
        )?;
        return Ok(());
    }

    issue_winpe_reboot(reporter)
}

#[cfg(target_os = "windows")]
async fn run_lightweight_deploy(
    config: LightweightDeployConfig,
    options: &WinpeDeployOptions,
    reporter: &WinpeReporter,
    progress_callback: Option<&dyn Fn(WinPEStatus)>,
) -> BitOSDTResult<PathBuf> {
    let mut config = config;
    reporter.log(
        "INFO",
        &format!(
            "Native WinPE lightweight deployment starting against {}.",
            config.server_url
        ),
    )?;
    emit_status(
        progress_callback,
        reporter.status(
            1,
            5,
            "Preparing lightweight deployment...",
            "Validating network access and deployment source.",
            false,
            None,
        )?,
    );

    verify_lightweight_server(&config.server_url, reporter).await?;
    let resolved_image = resolve_windows_image_path_for_source(
        &config.os_version,
        Some(config.unattend.language.as_str()),
        config.unc_image_path.as_deref(),
        config.unc_auth_username.as_deref(),
        config.unc_auth_password.as_deref(),
        config.http_image_url.as_deref(),
        reporter,
        config.prompt_unc_credentials_at_runtime,
    )
    .await?;
    let _unc_connection = resolved_image.unc_connection;
    let image_path = resolved_image.path;
    reporter.log(
        "INFO",
        &format!(
            "Resolved lightweight deployment image: {}",
            image_path.display()
        ),
    )?;
    let source_image_index = resolve_runtime_source_image_index(
        &image_path,
        config.source_image_index,
        &config.windows_edition,
    )?;
    reporter.log(
        "INFO",
        &format!(
            "Selected source image index {} for Windows edition '{}'.",
            source_image_index, config.windows_edition
        ),
    )?;

    emit_status(
        progress_callback,
        reporter.status(
            2,
            35,
            "Preparing deployment image...",
            "Applying lightweight deployment customizations to the downloaded image.",
            false,
            None,
        )?,
    );

    apply_runtime_domain_join_prompt(
        &mut config.unattend,
        config.runtime_domain_join.as_ref(),
        reporter,
    )?;

    let prepared_image =
        prepare_lightweight_image(&config, &image_path, source_image_index, reporter)?;

    let mapped = FullIsoDeployConfig {
        mode: "lightweight".to_string(),
        os_version: config.os_version.clone(),
        wim_index: config.wim_index.max(1),
        target_disk: None,
        disk_selection_policy: DiskSelectionPolicy::ConfigFirstSafeFallback,
        runtime_driver_policy: config.runtime_driver_policy.clone(),
        runtime_driver_context: config.runtime_driver_context.clone(),
        unc_image_path: None,
        unc_auth_username: None,
        unc_auth_password: None,
        http_image_url: None,
        expected_payload_size_bytes: None,
        expected_payload_sha256: None,
        expected_payload_file_name: None,
        unattend: config.unattend.clone(),
        autopilot: config.autopilot.clone(),
        task_sequence: config.task_sequence.clone(),
        runtime_domain_join: config.runtime_domain_join.clone(),
        prompt_unc_credentials_at_runtime: config.prompt_unc_credentials_at_runtime,
    };

    let firmware_is_uefi = detect_winpe_firmware_mode(reporter)?;
    let target_disk = resolve_target_disk(&mapped)?;
    let disk_manager = DiskManager::new(target_disk);
    disk_manager.initialize_disk(firmware_is_uefi)?;

    // Verify partitions are accessible before proceeding
    disk_manager.verify_partitions(firmware_is_uefi)?;
    reporter.log(
        "INFO",
        "Verified that partition drive letters are correctly assigned and accessible.",
    )?;

    let windows_partition = disk_manager.get_windows_partition()?;
    let system_partition = PathBuf::from(r"S:\");

    emit_status(
        progress_callback,
        reporter.status(
            2,
            60,
            "Applying Windows image...",
            "Applying the prepared lightweight image to the destination disk.",
            false,
            None,
        )?,
    );

    let wim_manager = WimManager::new();
    wim_manager.apply_wim(
        &prepared_image,
        config.wim_index.max(1),
        &windows_partition,
        None,
    )?;

    emit_status(
        progress_callback,
        reporter.status(
            3,
            82,
            "Installing drivers...",
            "Running runtime DriverPack preparation and offline injection.",
            false,
            None,
        )?,
    );

    run_runtime_driver_stage(
        &config.os_version,
        &config.runtime_driver_policy,
        &config.runtime_driver_context,
        options.runtime_driver_config_path.as_deref(),
        &windows_partition,
        reporter,
    )
    .await?;

    emit_status(
        progress_callback,
        reporter.status(
            3,
            94,
            "Configuring bootloader...",
            "Configuring Windows boot files.",
            false,
            None,
        )?,
    );

    let boot_manager = BootManager::new();
    boot_manager.configure_bootloader(
        &windows_partition.join("Windows"),
        &system_partition,
        firmware_is_uefi,
    )?;

    emit_status(
        progress_callback,
        reporter.status(
            4,
            100,
            "Finalizing deployment...",
            if options.skip_reboot {
                "Lightweight deployment complete. Reboot skipped by request."
            } else {
                "Lightweight deployment complete. Rebooting into Windows."
            },
            false,
            None,
        )?,
    );

    if options.skip_reboot {
        reporter.log(
            "WARN",
            "Skipping reboot because --skip-reboot was requested.",
        )?;
        return Ok(prepared_image);
    }

    issue_winpe_reboot(reporter)?;
    Ok(prepared_image)
}

#[cfg(target_os = "windows")]
async fn verify_lightweight_server(
    server_url: &str,
    reporter: &WinpeReporter,
) -> BitOSDTResult<()> {
    let trimmed = server_url.trim_end_matches('/');
    let health_url = format!("{}/health", trimmed);
    match reqwest::get(&health_url).await {
        Ok(response) if response.status() == StatusCode::OK => {
            reporter.log(
                "INFO",
                &format!("Lightweight server health check succeeded: {}", health_url),
            )?;
        }
        Ok(response) => {
            reporter.log(
                "WARN",
                &format!(
                    "Lightweight server health check returned HTTP {} for {}.",
                    response.status(),
                    health_url
                ),
            )?;
        }
        Err(err) => {
            reporter.log(
                "WARN",
                &format!(
                    "Lightweight server health check failed for {}: {}",
                    health_url, err
                ),
            )?;
        }
    }

    let manifest_url = format!("{}/api/manifest", trimmed);
    match reqwest::get(&manifest_url).await {
        Ok(response) if response.status().is_success() => {
            reporter.log(
                "INFO",
                &format!("Fetched lightweight manifest from {}.", manifest_url),
            )?;
        }
        Ok(response) => {
            reporter.log(
                "WARN",
                &format!(
                    "Lightweight manifest fetch returned HTTP {} for {}.",
                    response.status(),
                    manifest_url
                ),
            )?;
        }
        Err(err) => {
            reporter.log(
                "WARN",
                &format!(
                    "Lightweight manifest fetch failed for {}: {}",
                    manifest_url, err
                ),
            )?;
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn resolve_runtime_source_image_index(
    image_path: &Path,
    source_image_index: Option<u32>,
    windows_edition: &str,
) -> BitOSDTResult<u32> {
    if let Some(source_image_index) = source_image_index.filter(|index| *index > 0) {
        return Ok(source_image_index);
    }

    let wim_info = WimManager::new().get_wim_info(image_path)?;
    let selected_image = resolve_requested_edition_image(&wim_info.images, windows_edition)
        .ok_or_else(|| {
            BitOSDTError::InvalidInput(format!(
                "The selected Windows edition '{}' was not found in {}. Available images: {}",
                windows_edition,
                image_path.display(),
                describe_available_images(&wim_info.images)
            ))
        })?;

    Ok(selected_image.index)
}

#[cfg(target_os = "windows")]
fn prepare_lightweight_image(
    config: &LightweightDeployConfig,
    image_path: &Path,
    source_image_index: u32,
    reporter: &WinpeReporter,
) -> BitOSDTResult<PathBuf> {
    let state_root = PathBuf::from(r"X:\BitOSDT\State\lightweight-prepare");
    if state_root.exists() {
        let _ = fs::remove_dir_all(&state_root);
    }
    fs::create_dir_all(&state_root)?;

    let source_for_preparation = state_root.join("downloaded-source.wim");
    WimManager::new().export_wim(
        image_path,
        source_image_index.max(1),
        &source_for_preparation,
    )?;

    let output_wim = state_root.join("prepared-install.wim");
    let preparer = ImagePreparer::new(state_root.join("work"))?;
    reporter.log(
        "INFO",
        &format!(
            "Preparing lightweight image {} into {}.",
            source_for_preparation.display(),
            output_wim.display()
        ),
    )?;
    preparer.prepare_image(
        &ImagePrepConfig {
            source_wim: source_for_preparation,
            wim_index: 1,
            unattend: Some(config.unattend.clone()),
            autopilot: config.autopilot.clone(),
            task_sequence: config.task_sequence.clone(),
            inject_files: config.inject_files.clone(),
            driver_paths: Vec::new(),
            remove_apps: Vec::new(),
            enable_features: Vec::new(),
            disable_features: Vec::new(),
        },
        &output_wim,
    )?;
    Ok(output_wim)
}

#[cfg(target_os = "windows")]
async fn resolve_windows_image_path_for_source(
    os_version: &str,
    download_language: Option<&str>,
    unc_image_path: Option<&str>,
    unc_auth_username: Option<&str>,
    unc_auth_password: Option<&str>,
    http_image_url: Option<&str>,
    reporter: &WinpeReporter,
    prompt_unc_credentials_at_runtime: Option<bool>,
) -> BitOSDTResult<ResolvedWindowsImage> {
    let candidates = [
        PathBuf::from(r"X:\sources\install.wim"),
        PathBuf::from(r"X:\sources\install.esd"),
        PathBuf::from(r"X:\BitOSDT\install.wim"),
        PathBuf::from(r"X:\BitOSDT\install.esd"),
    ];

    let local_matches: Vec<PathBuf> = candidates
        .iter()
        .filter(|candidate| candidate.exists())
        .cloned()
        .collect();
    let mut diagnostics = ImageResolutionDiagnostics {
        local_searched: candidates
            .iter()
            .map(|candidate| candidate.display().to_string())
            .collect(),
        local_matches: local_matches
            .iter()
            .map(|candidate| candidate.display().to_string())
            .collect(),
        unc: None,
        http: None,
    };

    if local_matches.len() > 1 {
        return Err(BitOSDTError::InvalidInput(format!(
            "Multiple local Windows images are available in WinPE: {}",
            local_matches
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    if let Some(match_path) = local_matches.into_iter().next() {
        return Ok(ResolvedWindowsImage {
            path: match_path,
            unc_connection: None,
        });
    }

    if let Some(unc_path) = unc_image_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let unc_auth_username = unc_auth_username
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let unc_auth_password = unc_auth_password.filter(|value| !value.trim().is_empty());

        match (unc_auth_username, unc_auth_password) {
            (Some(username), Some(password)) => {
                match WinpeUncConnection::connect(unc_path, username, password) {
                    Ok((connection, mapped_path)) => {
                        if mapped_path.exists() {
                            reporter.log(
                                "INFO",
                                &format!(
                                    "Authenticated UNC image path {} via temporary mapping {}.",
                                    unc_path,
                                    mapped_path.display()
                                ),
                            )?;
                            return Ok(ResolvedWindowsImage {
                                path: mapped_path,
                                unc_connection: Some(connection),
                            });
                        }

                        reporter.log(
                            "WARN",
                            &format!(
                                "Configured UNC image path was not accessible after authenticated mapping: {}",
                                unc_path
                            ),
                        )?;
                        diagnostics.unc = Some(RemoteImageAttempt::Failed {
                            target: unc_path.to_string(),
                            reason: "mapped path was not accessible from WinPE".to_string(),
                        });
                    }
                    Err(error) => {
                        reporter.log(
                            "WARN",
                            &format!(
                                "Configured UNC image path authentication failed for {}: {}",
                                unc_path, error
                            ),
                        )?;
                        diagnostics.unc = Some(RemoteImageAttempt::Failed {
                            target: unc_path.to_string(),
                            reason: error.to_string(),
                        });
                    }
                }
            }
            _ => {
                if prompt_unc_credentials_at_runtime.unwrap_or(false) {
                    reporter.log(
                        "INFO",
                        &format!(
                            "Runtime credential prompt enabled for UNC path: {}. Launching credential prompt...",
                            unc_path
                        ),
                    )?;
                    match crate::build::provisioning_ui::launch_credential_prompt("UNC") {
                        Ok((username, password)) => {
                            match WinpeUncConnection::connect(unc_path, &username, &password) {
                                Ok((connection, mapped_path)) => {
                                    if mapped_path.exists() {
                                        reporter.log(
                                            "INFO",
                                            &format!(
                                                "Authenticated UNC image path {} via runtime credential prompt, mapped to {}.",
                                                unc_path,
                                                mapped_path.display()
                                            ),
                                        )?;
                                        return Ok(ResolvedWindowsImage {
                                            path: mapped_path,
                                            unc_connection: Some(connection),
                                        });
                                    }
                                    reporter.log(
                                        "WARN",
                                        &format!(
                                            "UNC path was not accessible after runtime credential prompt: {}",
                                            unc_path
                                        ),
                                    )?;
                                    diagnostics.unc = Some(RemoteImageAttempt::Failed {
                                        target: unc_path.to_string(),
                                        reason: "mapped path was not accessible after runtime credential prompt".to_string(),
                                    });
                                }
                                Err(error) => {
                                    reporter.log(
                                        "WARN",
                                        &format!(
                                            "UNC authentication with runtime credentials failed for {}: {}",
                                            unc_path, error
                                        ),
                                    )?;
                                    diagnostics.unc = Some(RemoteImageAttempt::Failed {
                                        target: unc_path.to_string(),
                                        reason: format!(
                                            "runtime credential auth failed: {}",
                                            error
                                        ),
                                    });
                                }
                            }
                        }
                        Err(error) => {
                            reporter.log(
                                "WARN",
                                &format!(
                                    "Runtime credential prompt was cancelled or failed for UNC path {}: {}",
                                    unc_path, error
                                ),
                            )?;
                            diagnostics.unc = Some(RemoteImageAttempt::Failed {
                                target: unc_path.to_string(),
                                reason: format!(
                                    "runtime credential prompt cancelled or failed: {}",
                                    error
                                ),
                            });
                        }
                    }
                } else {
                    reporter.log(
                        "WARN",
                        &format!(
                            "Configured UNC image path is missing credentials in deploy.json: {}",
                            unc_path
                        ),
                    )?;
                    diagnostics.unc = Some(RemoteImageAttempt::Failed {
                        target: unc_path.to_string(),
                        reason: "username or password was missing from deploy.json".to_string(),
                    });
                }
            }
        }
    }

    if let Some(url) = http_image_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let download_root = PathBuf::from(r"X:\BitOSDT");
        fs::create_dir_all(&download_root)?;
        reporter.log("INFO", &format!("Downloading Windows image from {}.", url))?;
        let downloader = EsdDownloader::new_with_adk(download_root, None).map_err(|e| {
            BitOSDTError::Download(format!("Failed to initialize downloader: {}", e))
        })?;
        let file_size = downloader.get_file_size(url).await.unwrap_or(0);
        let esd_info = EsdInfo {
            id: format!("native-image-{}", os_version),
            display_name: format!("Windows {}", os_version),
            url: url.to_string(),
            size_bytes: file_size,
            sha256: None,
            language: download_language.unwrap_or("en-US").to_string(),
            architecture: "amd64".to_string(),
            version: os_version.to_string(),
            build: os_version.to_string(),
        };
        match downloader.download_esd(&esd_info, |_| {}).await {
            Ok(path) => {
                return Ok(ResolvedWindowsImage {
                    path,
                    unc_connection: None,
                })
            }
            Err(error) => {
                diagnostics.http = Some(RemoteImageAttempt::Failed {
                    target: url.to_string(),
                    reason: error.to_string(),
                });
                reporter.log(
                    "WARN",
                    &format!("Windows image download failed from {}: {}", url, error),
                )?;
            }
        }
    }

    Err(BitOSDTError::NotFound(format_image_resolution_failure(
        &diagnostics,
    )))
}

#[cfg(target_os = "windows")]
fn resolve_target_disk(config: &FullIsoDeployConfig) -> BitOSDTResult<u32> {
    match config.disk_selection_policy {
        DiskSelectionPolicy::AlwaysDisk0 => Ok(0),
        DiskSelectionPolicy::RequireExplicitDisk => config.target_disk.ok_or_else(|| {
            BitOSDTError::InvalidInput(
                "Disk selection policy requires target_disk to be set in deploy.json".to_string(),
            )
        }),
        DiskSelectionPolicy::ConfigFirstSafeFallback => Ok(config.target_disk.unwrap_or(0)),
    }
}

#[cfg(target_os = "windows")]
async fn run_runtime_driver_stage(
    os_version: &str,
    runtime_driver_policy: &RuntimeDriverPolicy,
    runtime_driver_context: &RuntimeDriverContext,
    runtime_driver_config_path: Option<&Path>,
    windows_partition: &Path,
    reporter: &WinpeReporter,
) -> BitOSDTResult<()> {
    let runtime_config = if let Some(path) = runtime_driver_config_path.filter(|path| path.exists())
    {
        let payload = fs::read_to_string(path).map_err(|e| {
            BitOSDTError::NotFound(format!(
                "Failed to read runtime driver config {}: {}",
                path.display(),
                e
            ))
        })?;
        serde_json::from_str::<RuntimeDriverConfig>(&payload).map_err(|e| {
            BitOSDTError::InvalidInput(format!(
                "Failed to parse runtime driver config {}: {}",
                path.display(),
                e
            ))
        })?
    } else {
        RuntimeDriverConfig {
            os_version: os_version.to_string(),
            runtime_driver_policy: runtime_driver_policy.clone(),
            runtime_driver_context: runtime_driver_context.clone(),
        }
    };

    let manifest = prepare_runtime_drivers(&runtime_config, Some(windows_partition)).await?;
    if !manifest.warnings.is_empty() {
        for warning in manifest.warnings {
            reporter.log("WARN", &warning)?;
        }
    }
    if let Some(driverpack) = manifest.matched_driverpack {
        reporter.log(
            "INFO",
            &format!(
                "Resolved DriverPack {} for offline injection.",
                driverpack.name
            ),
        )?;
    } else {
        reporter.log(
            "WARN",
            "No matching DriverPack was resolved for this hardware.",
        )?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn detect_winpe_firmware_mode(reporter: &WinpeReporter) -> BitOSDTResult<bool> {
    let _ = Command::new("wpeutil").arg("UpdateBootInfo").status();

    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control",
            "/v",
            "PEFirmwareType",
        ])
        .output()
        .map_err(|e| {
            BitOSDTError::Deployment(format!("Failed to query WinPE firmware mode: {}", e))
        })?;

    if !output.status.success() {
        reporter.log(
            "WARN",
            "Failed to query PEFirmwareType from the registry; defaulting to UEFI.",
        )?;
        return Ok(true);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.contains("PEFirmwareType") {
            continue;
        }

        let normalized = line.to_ascii_lowercase();
        if normalized.contains("0x1") {
            return Ok(false);
        }
        if normalized.contains("0x2") {
            return Ok(true);
        }
    }

    reporter.log(
        "WARN",
        "PEFirmwareType was present but could not be parsed; defaulting to UEFI.",
    )?;
    Ok(true)
}

#[cfg(target_os = "windows")]
fn issue_winpe_reboot(reporter: &WinpeReporter) -> BitOSDTResult<()> {
    reporter.log("INFO", "Requesting reboot via wpeutil reboot.")?;
    if let Ok(status) = Command::new("wpeutil").arg("reboot").status() {
        if status.success() {
            return Ok(());
        }
        reporter.log(
            "WARN",
            &format!(
                "wpeutil reboot returned exit code {:?}; trying shutdown fallback.",
                status.code()
            ),
        )?;
    } else {
        reporter.log(
            "WARN",
            "Failed to start wpeutil reboot; trying shutdown fallback.",
        )?;
    }

    let status = Command::new("shutdown")
        .args(["/r", "/t", "0", "/f"])
        .status()
        .map_err(|e| {
            BitOSDTError::Deployment(format!("Failed to invoke shutdown fallback: {}", e))
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(BitOSDTError::Deployment(format!(
            "Shutdown reboot fallback failed with exit code {:?}",
            status.code()
        )))
    }
}

#[allow(dead_code)]
fn ensure_parent_directory(path: &Path) -> BitOSDTResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[allow(dead_code)]
fn emit_status(progress_callback: Option<&dyn Fn(WinPEStatus)>, status: WinPEStatus) {
    if let Some(callback) = progress_callback {
        callback(status);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use super::{
        apply_prompted_domain_join_to_unattend, map_unc_path_to_local_path, parse_unc_path_parts,
        resolve_runtime_source_image_index,
    };
    use super::{
        format_image_resolution_failure, validate_resolved_payload_provenance,
        ImageResolutionDiagnostics, RemoteImageAttempt,
    };
    #[cfg(target_os = "windows")]
    use crate::build::provisioning_ui::DomainCredentialPromptResult;
    #[cfg(target_os = "windows")]
    use crate::config::UnattendConfig;
    use crate::download::HashValidator;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_file_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bitosdt-winpe-runtime-{}-{}", Uuid::new_v4(), name))
    }

    #[test]
    fn format_image_resolution_failure_reports_unc_only_without_http() {
        let message = format_image_resolution_failure(&ImageResolutionDiagnostics {
            local_searched: vec![
                r"X:\sources\install.wim".to_string(),
                r"X:\sources\install.esd".to_string(),
            ],
            local_matches: vec![],
            unc: Some(RemoteImageAttempt::Failed {
                target: r"\\server\share\install.wim".to_string(),
                reason: "path was not accessible from WinPE".to_string(),
            }),
            http: None,
        });

        assert!(message.contains("Local search: no Windows image found."));
        assert!(message.contains(r"\\server\share\install.wim"));
        assert!(!message.contains("HTTP"));
    }

    #[test]
    fn format_image_resolution_failure_reports_http_only_without_unc() {
        let message = format_image_resolution_failure(&ImageResolutionDiagnostics {
            local_searched: vec![r"X:\BitOSDT\install.wim".to_string()],
            local_matches: vec![],
            unc: None,
            http: Some(RemoteImageAttempt::Failed {
                target: "http://deploy.local/install.wim".to_string(),
                reason: "connection timed out".to_string(),
            }),
        });

        assert!(message.contains("Local search: no Windows image found."));
        assert!(message.contains("HTTP download attempted from http://deploy.local/install.wim and failed: connection timed out"));
        assert!(!message.contains("UNC"));
    }

    #[test]
    fn format_image_resolution_failure_reports_unc_then_http_for_full_iso_fallback() {
        let message = format_image_resolution_failure(&ImageResolutionDiagnostics {
            local_searched: vec![r"X:\sources\install.wim".to_string()],
            local_matches: vec![],
            unc: Some(RemoteImageAttempt::Failed {
                target: r"\\server\share\install.wim".to_string(),
                reason: "path was not accessible from WinPE".to_string(),
            }),
            http: Some(RemoteImageAttempt::Failed {
                target: "http://deploy.local/install.wim".to_string(),
                reason: "404 Not Found".to_string(),
            }),
        });

        let unc_index = message.find("UNC path configured").expect("unc message");
        let http_index = message
            .find("HTTP download attempted")
            .expect("http message");
        assert!(unc_index < http_index);
    }

    #[test]
    fn format_image_resolution_failure_reports_local_only_when_no_remote_sources_exist() {
        let message = format_image_resolution_failure(&ImageResolutionDiagnostics {
            local_searched: vec![r"X:\sources\install.wim".to_string()],
            local_matches: vec![],
            unc: None,
            http: None,
        });

        assert!(message.contains("Local search: no Windows image found."));
        assert!(!message.contains("UNC"));
        assert!(!message.contains("HTTP"));
    }

    #[test]
    fn format_image_resolution_failure_can_describe_local_match_state() {
        let message = format_image_resolution_failure(&ImageResolutionDiagnostics {
            local_searched: vec![r"X:\sources\install.wim".to_string()],
            local_matches: vec![r"X:\sources\install.wim".to_string()],
            unc: None,
            http: None,
        });

        assert!(message.contains(r"Local search: found Windows images: X:\sources\install.wim"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_unc_path_parts_extracts_share_root_for_install_wim() {
        let parsed = parse_unc_path_parts(r"\\server\share\install.wim").expect("valid UNC path");

        assert_eq!(parsed.share_root, r"\\server\share");
        assert_eq!(parsed.relative_path, "install.wim");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_unc_path_parts_extracts_nested_relative_path() {
        let parsed = parse_unc_path_parts(r"\\server\share\images\win11\install.wim")
            .expect("valid nested UNC path");

        assert_eq!(parsed.share_root, r"\\server\share");
        assert_eq!(parsed.relative_path, r"images\win11\install.wim");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_unc_path_parts_rejects_host_only_unc_file_path() {
        let err =
            parse_unc_path_parts(r"\\server\install.wim").expect_err("missing share should fail");

        assert!(err
            .to_string()
            .contains("server, share, and file path beneath the share"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_unc_path_parts_rejects_empty_share_segment() {
        let err = parse_unc_path_parts(r"\\server\\install.wim")
            .expect_err("empty share segment should fail");

        assert!(err
            .to_string()
            .contains("server, share, and file path beneath the share"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn apply_prompted_domain_join_to_unattend_sets_domain_join_values() {
        let mut unattend = UnattendConfig::default();
        apply_prompted_domain_join_to_unattend(
            &mut unattend,
            DomainCredentialPromptResult {
                domain: "contoso.local".to_string(),
                ou_path: Some("OU=Devices,DC=contoso,DC=local".to_string()),
                username: "CONTOSO\\joiner".to_string(),
                password: "Secret123!".to_string(),
            },
        );

        let domain_join = unattend.domain_join.expect("domain join config");
        assert_eq!(domain_join.domain, "contoso.local");
        assert_eq!(domain_join.username, "CONTOSO\\joiner");
        assert_eq!(
            domain_join.machine_object_ou.as_deref(),
            Some("OU=Devices,DC=contoso,DC=local")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn map_unc_path_to_local_path_rewrites_share_relative_path() {
        let parsed =
            parse_unc_path_parts(r"\\server\share\images\install.wim").expect("valid UNC path");

        assert_eq!(
            map_unc_path_to_local_path("Z:", &parsed),
            PathBuf::from(r"Z:\images\install.wim")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resolve_runtime_source_image_index_prefers_stored_value() {
        let resolved = resolve_runtime_source_image_index(
            PathBuf::from(r"X:\sources\missing.esd").as_path(),
            Some(5),
            "Education",
        )
        .expect("stored source image index should be used");

        assert_eq!(resolved, 5);
    }

    #[test]
    fn validate_resolved_payload_provenance_accepts_matching_file() {
        let payload_path = temp_file_path("matching.wim");
        fs::write(&payload_path, b"prepared-payload").expect("write payload");
        let metadata = fs::metadata(&payload_path).expect("payload metadata");
        let sha256 = HashValidator::calculate_sha256(&payload_path).expect("payload hash");

        validate_resolved_payload_provenance(
            &payload_path,
            Some(metadata.len()),
            Some(&sha256),
            Some("install.wim"),
        )
        .expect("payload provenance should match");

        let _ = fs::remove_file(payload_path);
    }

    #[test]
    fn validate_resolved_payload_provenance_rejects_wrong_size() {
        let payload_path = temp_file_path("wrong-size.wim");
        fs::write(&payload_path, b"prepared-payload").expect("write payload");
        let metadata = fs::metadata(&payload_path).expect("payload metadata");

        let err = validate_resolved_payload_provenance(
            &payload_path,
            Some(metadata.len() + 1),
            None,
            Some("install.wim"),
        )
        .expect_err("size mismatch should fail");

        let message = err.to_string();
        assert!(message.contains("does not match the prepared payload exported by BitOSDT"));
        assert!(message.contains("Size mismatch"));
        assert!(message.contains("C:\\BitOSDT\\WDS"));
        assert!(message.contains("Expected exported file name: install.wim."));

        let _ = fs::remove_file(payload_path);
    }

    #[test]
    fn validate_resolved_payload_provenance_rejects_wrong_hash() {
        let payload_path = temp_file_path("wrong-hash.wim");
        fs::write(&payload_path, b"prepared-payload").expect("write payload");
        let metadata = fs::metadata(&payload_path).expect("payload metadata");

        let err = validate_resolved_payload_provenance(
            &payload_path,
            Some(metadata.len()),
            Some("deadbeef"),
            Some("install.wim"),
        )
        .expect_err("hash mismatch should fail");

        let message = err.to_string();
        assert!(message.contains("SHA-256 mismatch"));
        assert!(message.contains("deadbeef"));
        assert!(message.contains("C:\\BitOSDT\\WDS"));

        let _ = fs::remove_file(payload_path);
    }
}
