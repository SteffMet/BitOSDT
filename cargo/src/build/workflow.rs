use crate::build::full_iso_builder::{
    build_full_iso as build_full_iso_core, DiskSelectionPolicy, FullIsoBuildConfig,
};
use crate::build::lightweight_builder::{LightweightBuilder, LightweightConfig};
use crate::build::publish::stage_lightweight_media_tree;
use crate::build::shell_layout::{
    empty_shell_layout_value, generate_shell_layout_script, ShellLayoutConfig,
};
use crate::build::RuntimeDomainJoinConfig;
use crate::catalog::get_builtin_os_catalog;
use crate::config::{
    resolve_unattend_locale_settings, AutopilotOobeConfig, AutopilotProfile, DeploymentMode,
    DomainJoinConfig, NetworkLocation, OobeConfig, ProtectYourPc, UnattendConfig,
    UserAccountConfig, UserGroup,
};
use crate::core::models::{
    Architecture, Image, ImageStatus, LicenseInfo, LicenseType, OsInfo, OsType,
};
#[cfg(target_os = "windows")]
use crate::core::run_tracked_command_streaming;
use crate::core::{Config, Database, DriverPack, RuntimeDriverPolicy};
use crate::deploy::wim::{describe_available_images, resolve_requested_edition_image};
use crate::deploy::WimManager;
use crate::download::{EsdDownloader, EsdInfo};
use crate::policy::{
    empty_group_policy_selection_value, resolve_policy_registry_config, GroupPolicySelection,
};
use crate::tasks::{
    AppInstallConfig, ChocolateyPackage, CustomInstaller, CustomScript, InstallerSourceType,
    InstallerType, LocalPayloadItem, LocalPayloadKind, RebootBehavior, ScriptType, TaskDefinition,
    TaskSequence, TaskSettings, TaskType, WindowsUpdateConfig, WingetPackage,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::info;
use uuid::Uuid;

const SIMPLE_PUBLISH_FOLDER: &str = "pxe-simple";
const SIMPLE_PORT: u16 = 8080;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBuildRequest {
    pub windows_version: String,
    pub windows_build: String,
    pub windows_edition: String,
    /// The license channel type: "Retail" (Consumer/Home/Pro) or "Volume" (Enterprise/Education)
    #[serde(default)]
    pub windows_channel: Option<String>,
    pub language: Option<String>,
    pub output_type: String,
    pub output_path: String,
    pub volume_label: String,
    pub source_path: Option<String>,
    pub download_url: Option<String>,
    pub target_disk: Option<u32>,
    pub delivery_mode: Option<String>,
    pub server_url: Option<String>,
    #[serde(default)]
    pub driver_paths: Vec<String>,
    #[serde(default)]
    pub boot_driver_unc_path: Option<String>,
    pub apply_to_offline_windows: Option<bool>,
    #[serde(default)]
    pub runtime_driver_policy: Option<RuntimeDriverPolicy>,
    pub pxe_export_path: Option<String>,
    pub full_iso_unc_path: Option<String>,
    pub full_iso_unc_username: Option<String>,
    pub full_iso_unc_password: Option<String>,
    pub full_iso_http_url: Option<String>,
    #[serde(default)]
    pub prompt_unc_credentials_at_runtime: Option<bool>,
    pub include_gui: Option<bool>,
    pub existing_image_id: Option<String>,
    pub save_mode: Option<String>,
    pub oobe_config: serde_json::Value,
    pub user_accounts: Vec<serde_json::Value>,
    pub domain_join: serde_json::Value,
    pub autopilot: serde_json::Value,
    pub apps: serde_json::Value,
    pub windows_update: serde_json::Value,
    #[serde(default = "empty_group_policy_selection_value")]
    pub group_policies: serde_json::Value,
    #[serde(default = "empty_shell_layout_value")]
    pub shell_layout: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildProgress {
    pub step: String,
    pub progress: u32,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryModeKind {
    Simple,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInReadinessLevel {
    Ready,
    Warning,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInReadiness {
    pub level: SignInReadinessLevel,
    pub summary: String,
    pub details: Vec<String>,
    pub local_admin_count: u32,
    pub skip_user_oobe: bool,
    pub domain_join_enabled: bool,
    pub autopilot_enabled: bool,
    pub relies_on_external_identity: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ImageBuildContext {
    pub ui_dir: Option<PathBuf>,
    pub winpe_packages_dir: Option<PathBuf>,
    pub common_boot_driver_dir: Option<PathBuf>,
    pub runtime_driver_catalog: Vec<DriverPack>,
    pub native_runtime_executable: Option<PathBuf>,
    pub gui_executable: Option<PathBuf>,
    pub simple_publish_path: Option<PathBuf>,
    pub simple_runtime_url: Option<String>,
    pub winpe_assets_dir: Option<PathBuf>,
    pub persist_built_image: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSaveMode {
    Overwrite,
    Copy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendOobeConfig {
    skip_machine_oobe: bool,
    skip_user_oobe: bool,
    hide_eula: bool,
    hide_wireless_setup: bool,
    hide_local_account_screen: bool,
    hide_online_account_screens: bool,
    network_location: String,
    protect_your_pc: String,
    #[serde(default)]
    computer_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendUserAccount {
    username: String,
    password: String,
    display_name: Option<String>,
    group: String,
    password_never_expires: bool,
    require_password_change: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendDomainJoin {
    enabled: bool,
    domain: String,
    username: String,
    password: String,
    ou_path: Option<String>,
    #[serde(default)]
    prompt_for_domain_credentials_at_runtime: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendAutopilot {
    enabled: bool,
    tenant_id: String,
    deployment_mode: String,
    skip_user_oobe: bool,
    skip_device_oobe: bool,
    allow_whiteglove: bool,
    group_tag: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendWingetPackage {
    package_id: String,
    version: Option<String>,
    custom_args: Option<String>,
    enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendChocolateyPackage {
    package_name: String,
    version: Option<String>,
    source: Option<String>,
    custom_args: Option<String>,
    enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendLocalPayloadItem {
    source_path: String,
    source_kind: String,
    display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendCustomInstaller {
    name: String,
    path: String,
    source_type: Option<String>,
    source_file_name: Option<String>,
    #[serde(default)]
    dependencies: Vec<FrontendLocalPayloadItem>,
    dependency_destination: Option<String>,
    silent_args: String,
    installer_type: String,
    enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendCustomScript {
    #[serde(default)]
    name: String,
    #[serde(default)]
    content: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_true")]
    continue_on_error: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendApps {
    winget_packages: Vec<FrontendWingetPackage>,
    chocolatey_packages: Vec<FrontendChocolateyPackage>,
    custom_installers: Vec<FrontendCustomInstaller>,
    #[serde(default)]
    copied_items: Vec<FrontendLocalPayloadItem>,
    copy_destination: Option<String>,
    auto_install_chocolatey: bool,
    continue_on_error: bool,
    #[serde(default)]
    enable_custom_scripts: bool,
    #[serde(default)]
    custom_scripts: Vec<FrontendCustomScript>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendWindowsUpdate {
    enabled: bool,
    install_security_updates: bool,
    install_critical_updates: bool,
    install_driver_updates: bool,
    exclude_preview: bool,
    exclude_optional: bool,
    reboot_behavior: String,
}

fn default_true() -> bool {
    true
}

pub fn output_includes_lightweight(output_type: &str) -> bool {
    matches!(output_type, "LightweightISO" | "Both")
}

pub fn resolve_delivery_mode(request: &ImageBuildRequest) -> DeliveryModeKind {
    match request.delivery_mode.as_deref() {
        Some("Advanced") => DeliveryModeKind::Advanced,
        _ => DeliveryModeKind::Simple,
    }
}

fn trim_optional_string(value: Option<&String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

fn normalize_optional_password(value: Option<&String>) -> Option<String> {
    value.cloned().filter(|raw| !raw.trim().is_empty())
}

fn validate_unc_file_path(value: &str, path_label: &str) -> Result<(), String> {
    if !value.starts_with(r"\\") {
        return Err(format!("{} must start with \\\\: {}", path_label, value));
    }

    let segments: Vec<&str> = value[2..].split('\\').collect();
    if segments.len() < 3
        || segments[0].trim().is_empty()
        || segments[1].trim().is_empty()
        || segments[2..]
            .iter()
            .any(|segment| segment.trim().is_empty())
    {
        return Err(format!(
            "{} must be a full UNC file path like \\\\server\\share\\install.wim: {}",
            path_label, value
        ));
    }

    Ok(())
}

fn validate_unc_runtime_credentials(
    unc_path: Option<&String>,
    unc_username: Option<&String>,
    unc_password: Option<&String>,
    path_label: &str,
    prompt_at_runtime: bool,
) -> Result<(Option<String>, Option<String>), String> {
    let username = trim_optional_string(unc_username);
    let password = normalize_optional_password(unc_password);

    match (
        trim_optional_string(unc_path).is_some(),
        username.is_some(),
        password.is_some(),
    ) {
        (false, false, false) => Ok((None, None)),
        (false, _, _) => Err("UNC credentials require a UNC runtime path.".to_string()),
        (true, true, true) => Ok((username, password)),
        (true, false, false) if prompt_at_runtime => Ok((None, None)),
        (true, _, _) => Err(format!(
            "{} requires both a username and password, or enable the runtime credential prompt.",
            path_label
        )),
    }
}

fn normalize_unc_credentials_for_wizard_state(
    unc_path: Option<&String>,
    unc_username: Option<&String>,
    unc_password: Option<&String>,
) -> (Option<String>, Option<String>) {
    if trim_optional_string(unc_path).is_none() {
        return (None, None);
    }

    (
        trim_optional_string(unc_username),
        normalize_optional_password(unc_password),
    )
}

fn normalize_remote_sources_for_wizard_state(
    output_type: &str,
    unc_path: Option<&String>,
    unc_username: Option<&String>,
    unc_password: Option<&String>,
    http_url: Option<&String>,
    prompt_unc_credentials_at_runtime: bool,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    &'static str,
) {
    let (unc, http) = if output_type == "WDSPXE" {
        validate_wds_pxe_runtime_source(
            unc_path,
            unc_username,
            unc_password,
            http_url,
            prompt_unc_credentials_at_runtime,
        )
        .map(|(unc, http, _, _)| (unc, http))
        .unwrap_or_else(|_| {
            let unc = trim_optional_string(unc_path);
            let http = trim_optional_string(http_url);
            match (unc.clone(), http) {
                (Some(_), Some(_)) => (unc, None),
                _ => (unc, trim_optional_string(http_url)),
            }
        })
    } else {
        validate_full_iso_remote_sources(
            unc_path,
            unc_username,
            unc_password,
            http_url,
            prompt_unc_credentials_at_runtime,
        )
        .map(|(unc, http, _, _)| (unc, http))
        .unwrap_or_else(|_| {
            (
                trim_optional_string(unc_path),
                trim_optional_string(http_url),
            )
        })
    };
    let (unc_username, unc_password) =
        normalize_unc_credentials_for_wizard_state(unc.as_ref(), unc_username, unc_password);

    let runtime_source = if output_type == "WDSPXE" && http.is_some() && unc.is_none() {
        "HTTP"
    } else {
        "UNC"
    };

    (unc, http, unc_username, unc_password, runtime_source)
}

pub fn default_simple_publish_path() -> Result<PathBuf, String> {
    let workspace = Config::configured_workspace_path()
        .map_err(|e| format!("Failed to resolve BitOSDT workspace path: {}", e))?;
    Ok(workspace.join(SIMPLE_PUBLISH_FOLDER))
}

pub fn default_simple_runtime_url() -> String {
    let host = preferred_hostname()
        .or_else(local_ipv4_fallback)
        .unwrap_or_else(|| "127.0.0.1".to_string());
    format!("http://{}:{}", host, SIMPLE_PORT)
}

pub fn build_manifest_json(base_url: &str) -> String {
    json!({
        "name": "BitOSDT Lightweight Runtime",
        "mode": "simple",
        "baseUrl": base_url,
        "healthUrl": format!("{}/health", base_url),
        "downloadUrl": format!("{}/download/bitosdt.exe", base_url),
        "generatedAtUtc": Utc::now().to_rfc3339(),
    })
    .to_string()
}

#[derive(Debug, Clone)]
struct LightweightPublishedSource {
    source_path: PathBuf,
    published_file_name: String,
}

#[derive(Debug, Clone, Default)]
struct LightweightSourcePlan {
    http_image_url: Option<String>,
    unc_image_path: Option<String>,
    published_source: Option<LightweightPublishedSource>,
}

#[derive(Debug, Clone)]
pub struct ResolvedBuildSource {
    pub source_path: PathBuf,
    pub canonical_language: String,
    pub source_image_index: Option<u32>,
    pub normalized_to_single_image_wim: bool,
}

fn resolve_requested_language_tag(request: &ImageBuildRequest) -> Result<String, String> {
    resolve_language_settings(request).map(|(language, _)| language)
}

fn request_language_for_metadata(request: &ImageBuildRequest) -> String {
    resolve_requested_language_tag(request).unwrap_or_else(|_| {
        request
            .language
            .clone()
            .unwrap_or_else(|| "en-US".to_string())
    })
}

fn extract_download_filename(url: &str, fallback: &str) -> String {
    url.split('/')
        .next_back()
        .filter(|value| value.contains('.'))
        .map(|value| value.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn cached_download_path_for_url(download_dir: &Path, url: &str, fallback: &str) -> PathBuf {
    download_dir.join(extract_download_filename(url, fallback))
}

fn resolve_requested_source_image_index(
    source_path: &Path,
    requested_edition: &str,
) -> Result<u32, String> {
    let wim_info = WimManager::new().get_wim_info(source_path).map_err(|e| {
        format!(
            "Failed to inspect Windows source {}: {}",
            source_path.display(),
            e
        )
    })?;

    let image =
        resolve_requested_edition_image(&wim_info.images, requested_edition).ok_or_else(|| {
            format!(
                "The selected Windows edition '{}' was not found in {}. Available images: {}",
                requested_edition,
                source_path.display(),
                describe_available_images(&wim_info.images)
            )
        })?;

    Ok(image.index)
}

fn normalize_source_image_to_workspace_wim<F>(
    source_path: &Path,
    source_image_index: u32,
    workspace: &Path,
    progress_callback: &mut F,
) -> Result<PathBuf, String>
where
    F: FnMut(BuildProgress),
{
    let (normalized_wim, temp_wim) = normalized_wim_paths(source_path, workspace);
    if temp_wim.exists() {
        fs::remove_file(&temp_wim).map_err(|e| {
            format!(
                "Failed to remove stale normalized WIM {}: {}",
                temp_wim.display(),
                e
            )
        })?;
    }

    let export_destination = if source_path == normalized_wim {
        temp_wim.as_path()
    } else {
        normalized_wim.as_path()
    };

    progress_callback(BuildProgress {
        step: "source-normalize".to_string(),
        progress: 18,
        message: format!(
            "Normalizing Windows source image index {} into {}...",
            source_image_index,
            export_destination.display()
        ),
    });

    WimManager::new()
        .export_wim_with_progress(
            source_path,
            source_image_index,
            export_destination,
            "source-normalize",
            |progress, message| {
                progress_callback(BuildProgress {
                    step: "source-normalize".to_string(),
                    progress: 17 + ((progress as u32 * 2) / 100),
                    message,
                });
            },
        )
        .map_err(|e| {
            format!(
                "Failed to export Windows image index {} from {} into {}: {}",
                source_image_index,
                source_path.display(),
                export_destination.display(),
                e
            )
        })?;

    if export_destination != normalized_wim {
        if normalized_wim.exists() {
            fs::remove_file(&normalized_wim).map_err(|e| {
                format!(
                    "Failed to remove previous normalized WIM {}: {}",
                    normalized_wim.display(),
                    e
                )
            })?;
        }
        fs::rename(export_destination, &normalized_wim).map_err(|e| {
            format!(
                "Failed to replace normalized WIM {} with {}: {}",
                normalized_wim.display(),
                export_destination.display(),
                e
            )
        })?;
    }

    Ok(normalized_wim)
}

fn normalized_wim_paths(source_path: &Path, workspace: &Path) -> (PathBuf, PathBuf) {
    let normalized_wim = workspace.join("install.wim");
    let temp_wim = if source_path == normalized_wim {
        workspace.join("install.normalized.wim")
    } else {
        normalized_wim.clone()
    };
    (normalized_wim, temp_wim)
}

fn resolve_local_source_media_path<F>(
    source_path: &Path,
    workspace: &Path,
    progress_callback: &mut F,
) -> Result<PathBuf, String>
where
    F: FnMut(BuildProgress),
{
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "esd" | "wim" => Ok(source_path.to_path_buf()),
        "iso" => {
            let extract_dir = workspace.join("extracted");
            if extract_dir.exists() {
                let _ = fs::remove_dir_all(&extract_dir);
            }
            fs::create_dir_all(&extract_dir).map_err(|e| {
                format!(
                    "Failed to create ISO extraction directory {}: {}",
                    extract_dir.display(),
                    e
                )
            })?;

            progress_callback(BuildProgress {
                step: "extract".to_string(),
                progress: 10,
                message: "Extracting Windows ISO...".to_string(),
            });
            extract_iso_to_dir(source_path, &extract_dir)?;

            let sources_dir = extract_dir.join("sources");
            let install_esd = sources_dir.join("install.esd");
            if install_esd.exists() {
                return Ok(install_esd);
            }

            let install_wim = sources_dir.join("install.wim");
            if install_wim.exists() {
                return Ok(install_wim);
            }

            Err(format!(
                "Extracted ISO {} did not contain sources/install.wim or sources/install.esd",
                source_path.display()
            ))
        }
        other => Err(format!(
            "Unsupported Windows source format '.{}' for {}",
            other,
            source_path.display()
        )),
    }
}

#[cfg(target_os = "windows")]
fn extract_iso_to_dir(source_path: &Path, extract_dir: &Path) -> Result<(), String> {
    let source = source_path.to_string_lossy().replace('\'', "''");
    let destination = extract_dir.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$iso = Mount-DiskImage -ImagePath '{source}' -PassThru; \
         $drive = ($iso | Get-Volume).DriveLetter; \
         Copy-Item -Path \"$drive`:\\*\" -Destination '{destination}' -Recurse -Force; \
         Dismount-DiskImage -ImagePath '{source}'"
    );

    let executable = Path::new("powershell");
    let args = vec![
        "-NoProfile".to_string(),
        "-Command".to_string(),
        script.clone(),
    ];
    let mut command = Command::new(executable);
    command.args(&args);
    let output =
        run_tracked_command_streaming(command, executable, &args, "source-extract", |_| {})
            .map_err(|e| format!("Failed to extract ISO {}: {}", source_path.display(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "ISO extraction failed for {} (exit={:?}, stderr={})",
            source_path.display(),
            output.status.code(),
            if stderr.is_empty() {
                "<empty>"
            } else {
                &stderr
            }
        ));
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn extract_iso_to_dir(source_path: &Path, extract_dir: &Path) -> Result<(), String> {
    crate::build::linux_support::extract_iso_image(source_path, extract_dir)
        .map_err(|e| format!("Failed to extract ISO {}: {}", source_path.display(), e))
}

pub fn validate_driver_paths(paths: &[String]) -> Result<Vec<PathBuf>, String> {
    validate_driver_paths_with_network(paths, false)
}

pub fn validate_driver_paths_with_network(
    paths: &[String],
    allow_network: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut resolved = Vec::new();
    for raw_path in paths {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with(r"\\") && !allow_network {
            return Err(format!(
                "Driver path must be a local path, not a network share: {}",
                trimmed
            ));
        }

        let path = PathBuf::from(trimmed);
        if !path.exists() {
            return Err(format!("Driver path not found: {}", trimmed));
        }
        resolved.push(path);
    }
    Ok(resolved)
}

pub fn validate_full_iso_remote_sources(
    unc_path: Option<&String>,
    unc_username: Option<&String>,
    unc_password: Option<&String>,
    http_url: Option<&String>,
    prompt_unc_credentials_at_runtime: bool,
) -> Result<
    (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    String,
> {
    let unc = unc_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(value) = unc.as_ref() {
        validate_unc_file_path(value, "Full ISO UNC fallback path")?;
    }

    let http = http_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(value) = http.as_ref() {
        let lowered = value.to_ascii_lowercase();
        if !(lowered.starts_with("http://") || lowered.starts_with("https://")) {
            return Err(format!(
                "Full ISO HTTP fallback URL must start with http:// or https://: {}",
                value
            ));
        }
    }
    let (unc_username, unc_password) = validate_unc_runtime_credentials(
        unc_path,
        unc_username,
        unc_password,
        "Full ISO UNC fallback path",
        prompt_unc_credentials_at_runtime,
    )?;

    Ok((unc, http, unc_username, unc_password))
}

pub fn validate_wds_pxe_runtime_source(
    unc_path: Option<&String>,
    unc_username: Option<&String>,
    unc_password: Option<&String>,
    http_url: Option<&String>,
    prompt_unc_credentials_at_runtime: bool,
) -> Result<
    (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    String,
> {
    let (unc, http, unc_username, unc_password) = validate_full_iso_remote_sources(
        unc_path,
        unc_username,
        unc_password,
        http_url,
        prompt_unc_credentials_at_runtime,
    )?;
    match (unc.is_some(), http.is_some()) {
        (true, false) | (false, true) => Ok((unc, http, unc_username, unc_password)),
        _ => Err(
            "WDS/PXE output requires exactly one final runtime path: either UNC or HTTP."
                .to_string(),
        ),
    }
}

fn resolve_existing_local_source_candidate(
    request: &ImageBuildRequest,
    workspace: &Path,
) -> Result<Option<PathBuf>, String> {
    if let Some(source) = request.source_path.as_ref() {
        let source_path = PathBuf::from(source);
        if !source_path.exists() {
            return Err(format!("Source file not found: {}", source));
        }
        return Ok(Some(source_path));
    }

    for candidate in [
        workspace.join("install.iso"),
        workspace.join("install.esd"),
        workspace.join("install.wim"),
    ] {
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

fn plan_lightweight_source(
    request: &ImageBuildRequest,
    workspace: &Path,
    runtime_server_url: &str,
) -> Result<LightweightSourcePlan, String> {
    if let Some(download_url) = request
        .download_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(LightweightSourcePlan {
            http_image_url: Some(download_url.to_string()),
            ..Default::default()
        });
    }

    let Some(local_source) = resolve_existing_local_source_candidate(request, workspace)? else {
        return Ok(LightweightSourcePlan::default());
    };

    let extension = local_source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let published_source = match extension.as_str() {
        "wim" | "esd" => {
            let file_name = local_source
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("install.{}", extension));
            LightweightPublishedSource {
                source_path: local_source.clone(),
                published_file_name: file_name,
            }
        }
        "iso" => {
            #[cfg(not(target_os = "windows"))]
            {
                let extract_dir = workspace.join("lightweight-source");
                if extract_dir.exists() {
                    let _ = std::fs::remove_dir_all(&extract_dir);
                }
                crate::build::linux_support::extract_iso_image(&local_source, &extract_dir)
                    .map_err(|e| format!("Failed to extract local ISO source: {}", e))?;

                let staged_source = [
                    extract_dir.join("sources").join("install.wim"),
                    extract_dir.join("sources").join("install.esd"),
                ]
                .into_iter()
                .find(|candidate| candidate.is_file())
                .ok_or_else(|| {
                    format!(
                        "Extracted ISO {} did not contain sources/install.wim or sources/install.esd",
                        local_source.display()
                    )
                })?;

                let file_name = staged_source
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| "install.wim".to_string());

                LightweightPublishedSource {
                    source_path: staged_source,
                    published_file_name: file_name,
                }
            }

            #[cfg(target_os = "windows")]
            {
                return Err(format!(
                    "Lightweight deployment from a local ISO source is not supported by the native CLI builder on Windows hosts yet: {}",
                    local_source.display()
                ));
            }
        }
        other => {
            return Err(format!(
                "Unsupported lightweight source type '.{}' for {}",
                other,
                local_source.display()
            ));
        }
    };

    Ok(LightweightSourcePlan {
        http_image_url: Some(format!(
            "{}/images/{}",
            runtime_server_url.trim_end_matches('/'),
            published_source.published_file_name
        )),
        published_source: Some(published_source),
        ..Default::default()
    })
}

fn stage_lightweight_published_source(
    publish_path: &Path,
    source: &LightweightPublishedSource,
) -> Result<PathBuf, String> {
    let images_dir = publish_path.join("images");
    std::fs::create_dir_all(&images_dir)
        .map_err(|e| format!("Failed to create publish images directory: {}", e))?;
    let destination = images_dir.join(&source.published_file_name);
    std::fs::copy(&source.source_path, &destination).map_err(|e| {
        format!(
            "Failed to stage lightweight source {} into {}: {}",
            source.source_path.display(),
            destination.display(),
            e
        )
    })?;
    Ok(destination)
}

fn resolve_lightweight_source_image_index(
    request: &ImageBuildRequest,
    source_plan: &LightweightSourcePlan,
    download_dir: &Path,
) -> Result<Option<u32>, String> {
    if let Some(source) = source_plan.published_source.as_ref() {
        return resolve_requested_source_image_index(&source.source_path, &request.windows_edition)
            .map(Some);
    }

    let Some(download_url) = request
        .download_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let cached_path = cached_download_path_for_url(
        download_dir,
        download_url,
        &format!(
            "{}-{}-{}.esd",
            request.windows_version, request.windows_build, request.windows_edition
        ),
    );
    if !cached_path.exists() {
        return Ok(None);
    }

    resolve_requested_source_image_index(&cached_path, &request.windows_edition).map(Some)
}

pub async fn prepare_full_build_source<F>(
    request: &ImageBuildRequest,
    workspace: &Path,
    download_dir: &Path,
    mut progress_callback: F,
) -> Result<ResolvedBuildSource, String>
where
    F: FnMut(BuildProgress) + Send,
{
    let canonical_language = resolve_requested_language_tag(request)?;
    let source_path = resolve_source_path_for_request(
        request,
        workspace,
        download_dir,
        &canonical_language,
        |progress| progress_callback(progress),
    )
    .await?;

    let source_image_index =
        resolve_requested_source_image_index(&source_path, &request.windows_edition)?;
    progress_callback(BuildProgress {
        step: "source".to_string(),
        progress: 17,
        message: format!(
            "Selected {} from source image index {}.",
            request.windows_edition, source_image_index
        ),
    });

    let normalized_source_path = normalize_source_image_to_workspace_wim(
        &source_path,
        source_image_index,
        workspace,
        &mut progress_callback,
    )?;
    progress_callback(BuildProgress {
        step: "source".to_string(),
        progress: 19,
        message: format!(
            "Prepared normalized Windows source at {}.",
            normalized_source_path.display()
        ),
    });

    Ok(ResolvedBuildSource {
        source_path: normalized_source_path,
        canonical_language,
        source_image_index: Some(source_image_index),
        normalized_to_single_image_wim: true,
    })
}

pub fn resolve_language_settings(request: &ImageBuildRequest) -> Result<(String, String), String> {
    let language = request.language.as_deref().unwrap_or("en-US");
    resolve_unattend_locale_settings(language).map_err(|e| e.to_string())
}

fn validate_user_oobe_sign_in_path(
    oobe: &FrontendOobeConfig,
    user_accounts: &[FrontendUserAccount],
) -> Result<(), String> {
    let local_admin_count = user_accounts
        .iter()
        .filter(|user| matches!(map_user_group(&user.group), UserGroup::Administrators))
        .count() as u32;
    validate_sign_in_readiness(oobe.skip_user_oobe, local_admin_count, false, false).map(|_| ())
}

pub fn assess_sign_in_readiness(
    skip_user_oobe: bool,
    local_admin_count: u32,
    domain_join_enabled: bool,
    autopilot_enabled: bool,
) -> SignInReadiness {
    let relies_on_external_identity =
        local_admin_count == 0 && (domain_join_enabled || autopilot_enabled);

    if skip_user_oobe && local_admin_count == 0 {
        return SignInReadiness {
            level: SignInReadinessLevel::Blocked,
            summary: "Skip User OOBE requires at least one local administrator account so deployed Windows still has a usable sign-in path.".to_string(),
            details: vec![
                "BitOSDT cannot safely skip user OOBE when no local administrator account is configured.".to_string(),
                "Add a local administrator account or turn off Skip User OOBE for this deployment.".to_string(),
            ],
            local_admin_count,
            skip_user_oobe,
            domain_join_enabled,
            autopilot_enabled,
            relies_on_external_identity,
        };
    }

    if local_admin_count > 0 {
        let mut details = vec![format!(
            "{} local administrator account(s) are configured for recovery or first sign-in.",
            local_admin_count
        )];
        if domain_join_enabled {
            details.push(
                "Domain join is enabled in addition to the local administrator fallback."
                    .to_string(),
            );
        }
        if autopilot_enabled {
            details.push(
                "Autopilot is enabled in addition to the local administrator fallback.".to_string(),
            );
        }
        return SignInReadiness {
            level: SignInReadinessLevel::Ready,
            summary: "A local administrator sign-in path is configured for the deployed image."
                .to_string(),
            details,
            local_admin_count,
            skip_user_oobe,
            domain_join_enabled,
            autopilot_enabled,
            relies_on_external_identity: false,
        };
    }

    let (summary, mut details) = if relies_on_external_identity {
        (
            "No local administrator account is configured, so first sign-in depends on external identity provisioning.".to_string(),
            vec![
                "Keep domain join, Autopilot, networking, and identity dependencies available during first boot.".to_string(),
                "If external identity enrollment fails, there is no local administrator recovery account on this image.".to_string(),
            ],
        )
    } else {
        (
            "No local administrator account is configured, so first sign-in depends on the default Windows OOBE flow.".to_string(),
            vec![
                "This deployment is relying on standard Windows OOBE to establish the first usable account.".to_string(),
                "If you need an offline recovery path, add a local administrator account before exporting the image.".to_string(),
            ],
        )
    };

    if domain_join_enabled {
        details.push("Domain join is enabled for the deployed image.".to_string());
    }
    if autopilot_enabled {
        details.push("Autopilot is enabled for the deployed image.".to_string());
    }

    SignInReadiness {
        level: SignInReadinessLevel::Warning,
        summary,
        details,
        local_admin_count,
        skip_user_oobe,
        domain_join_enabled,
        autopilot_enabled,
        relies_on_external_identity,
    }
}

pub fn validate_sign_in_readiness(
    skip_user_oobe: bool,
    local_admin_count: u32,
    domain_join_enabled: bool,
    autopilot_enabled: bool,
) -> Result<SignInReadiness, String> {
    let readiness = assess_sign_in_readiness(
        skip_user_oobe,
        local_admin_count,
        domain_join_enabled,
        autopilot_enabled,
    );
    if readiness.level == SignInReadinessLevel::Blocked {
        return Err(readiness.summary.clone());
    }
    Ok(readiness)
}

pub fn build_unattend_config(request: &ImageBuildRequest) -> Result<UnattendConfig, String> {
    let oobe: FrontendOobeConfig = parse_frontend(request.oobe_config.clone(), "OOBE")?;
    let user_accounts: Vec<FrontendUserAccount> = request
        .user_accounts
        .iter()
        .cloned()
        .map(|value| parse_frontend(value, "user account"))
        .collect::<Result<Vec<_>, _>>()?;
    let domain_join: FrontendDomainJoin =
        parse_frontend(request.domain_join.clone(), "domain join")?;
    let (language, input_locale) = resolve_language_settings(request)?;
    let computer_name = validate_computer_name(oobe.computer_name.as_deref())?;
    validate_user_oobe_sign_in_path(&oobe, &user_accounts)?;
    let prompt_domain_credentials_at_runtime = domain_join
        .prompt_for_domain_credentials_at_runtime
        .unwrap_or(false);

    if domain_join.enabled
        && !prompt_domain_credentials_at_runtime
        && (domain_join.domain.trim().is_empty()
            || domain_join.username.trim().is_empty()
            || domain_join.password.trim().is_empty())
    {
        return Err("Domain Join is enabled but required fields are missing".to_string());
    }

    Ok(UnattendConfig {
        language,
        input_locale,
        timezone: "Pacific Standard Time".to_string(),
        oobe: OobeConfig {
            skip_machine_oobe: oobe.skip_machine_oobe,
            skip_user_oobe: oobe.skip_user_oobe,
            hide_eula: oobe.hide_eula,
            hide_wireless_setup: oobe.hide_wireless_setup,
            hide_local_account_screen: oobe.hide_local_account_screen,
            hide_online_account_screens: oobe.hide_online_account_screens,
            network_location: map_network_location(&oobe.network_location),
            protect_your_pc: map_protect_your_pc(&oobe.protect_your_pc),
        },
        users: user_accounts
            .into_iter()
            .map(|user| UserAccountConfig {
                username: user.username,
                password: user.password,
                display_name: user.display_name,
                group: map_user_group(&user.group),
                password_never_expires: user.password_never_expires,
                require_password_change: user.require_password_change,
            })
            .collect(),
        administrator_password: None,
        computer_name,
        product_key: None,
        domain_join: if domain_join.enabled && !prompt_domain_credentials_at_runtime {
            Some(DomainJoinConfig {
                domain: domain_join.domain,
                username: domain_join.username,
                password: domain_join.password,
                ou_path: domain_join.ou_path.clone(),
                machine_object_ou: domain_join.ou_path,
            })
        } else {
            None
        },
        wifi_profile: None,
        auto_logon: None,
        first_logon_commands: vec![],
    })
}

fn build_runtime_domain_join_config(
    request: &ImageBuildRequest,
) -> Result<Option<RuntimeDomainJoinConfig>, String> {
    let domain_join: FrontendDomainJoin =
        parse_frontend(request.domain_join.clone(), "domain join")?;

    if !domain_join.enabled {
        return Ok(None);
    }

    let default_domain = domain_join.domain.trim().to_string().trim().to_string();
    let default_ou_path = domain_join
        .ou_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    Ok(Some(RuntimeDomainJoinConfig {
        enabled: true,
        prompt_for_credentials_at_runtime: domain_join
            .prompt_for_domain_credentials_at_runtime
            .unwrap_or(false),
        default_domain: if default_domain.is_empty() {
            None
        } else {
            Some(default_domain)
        },
        default_ou_path,
    }))
}

pub fn build_autopilot_profile(
    request: &ImageBuildRequest,
) -> Result<Option<AutopilotProfile>, String> {
    let autopilot: FrontendAutopilot = parse_frontend(request.autopilot.clone(), "autopilot")?;

    if !autopilot.enabled {
        return Ok(None);
    }
    if autopilot.tenant_id.trim().is_empty() {
        return Err("Autopilot is enabled but Tenant ID is missing".to_string());
    }

    Ok(Some(AutopilotProfile {
        tenant_id: autopilot.tenant_id.clone(),
        tenant_domain: format!("{}.onmicrosoft.com", autopilot.tenant_id),
        device_name_template: None,
        deployment_mode: map_deployment_mode(&autopilot.deployment_mode),
        oobe_config: AutopilotOobeConfig {
            hide_keyboard: autopilot.skip_device_oobe,
            hide_escape: autopilot.skip_device_oobe,
            hide_privacy: autopilot.skip_user_oobe,
            hide_eula: autopilot.skip_user_oobe,
            enable_white_glove: autopilot.allow_whiteglove,
            user_accept_terms: false,
        },
        group_tag: autopilot.group_tag,
        assigned_user: None,
    }))
}

pub fn build_task_sequence(request: &ImageBuildRequest) -> Result<Option<TaskSequence>, String> {
    let apps: FrontendApps = parse_frontend(request.apps.clone(), "applications")?;
    let windows_update: FrontendWindowsUpdate =
        parse_frontend(request.windows_update.clone(), "windows update")?;
    let group_policies: GroupPolicySelection =
        parse_frontend(request.group_policies.clone(), "group policies")?;
    let shell_layout: ShellLayoutConfig =
        parse_frontend(request.shell_layout.clone(), "shell layout")?;
    let _preview_excluded = windows_update.exclude_preview;
    let copy_destination = apps.copy_destination.clone();
    let defer_shell_layout_to_first_logon =
        shell_layout_should_defer_to_first_logon(&apps, &shell_layout);

    if shell_layout.has_work() {
        if request.output_type == "LightweightISO" {
            return Err(
                "Live shell layout canvas is supported only for Windows 11 Full ISO or WDS/PXE builds in this release."
                    .to_string(),
            );
        }
        if !request.windows_version.to_ascii_lowercase().contains("11") {
            return Err(
                "Live shell layout canvas is currently supported only for Windows 11 builds."
                    .to_string(),
            );
        }
    }

    let mut tasks = Vec::new();
    let mut order = 10u32;
    if let Some(registry_config) = resolve_policy_registry_config(&group_policies)? {
        tasks.push(TaskDefinition {
            id: Uuid::new_v4(),
            name: "Apply Local Policy Baseline".to_string(),
            task_type: TaskType::SetRegistry(registry_config),
            order,
            enabled: true,
            continue_on_error: false,
            requires_reboot: false,
        });
        order += 10;
    }

    let has_app_work = !apps.copied_items.is_empty()
        || apps.winget_packages.iter().any(|package| package.enabled)
        || apps
            .chocolatey_packages
            .iter()
            .any(|package| package.enabled)
        || apps
            .custom_installers
            .iter()
            .any(|installer| installer.enabled);

    if has_app_work {
        let app_config = AppInstallConfig {
            winget_packages: apps
                .winget_packages
                .into_iter()
                .map(|package| WingetPackage {
                    package_id: package.package_id,
                    version: package.version,
                    custom_args: package.custom_args,
                    enabled: package.enabled,
                })
                .collect(),
            chocolatey_packages: apps
                .chocolatey_packages
                .into_iter()
                .map(|package| ChocolateyPackage {
                    package_name: package.package_name,
                    version: package.version,
                    source: package.source,
                    custom_args: package.custom_args,
                    enabled: package.enabled,
                })
                .collect(),
            custom_installers: apps
                .custom_installers
                .into_iter()
                .map(|installer| CustomInstaller {
                    name: installer.name,
                    path: installer.path,
                    source_type: map_custom_installer_source_type(installer.source_type.as_deref()),
                    source_file_name: installer.source_file_name,
                    dependencies: installer
                        .dependencies
                        .into_iter()
                        .map(|item| LocalPayloadItem {
                            source_path: item.source_path,
                            source_kind: map_local_payload_kind(&item.source_kind),
                            display_name: item.display_name,
                        })
                        .collect(),
                    dependency_destination: installer.dependency_destination,
                    silent_args: installer.silent_args,
                    installer_type: map_custom_installer_type(&installer.installer_type),
                    success_codes: vec![0, 3010],
                    enabled: installer.enabled,
                })
                .collect(),
            copied_items: apps
                .copied_items
                .into_iter()
                .map(|item| LocalPayloadItem {
                    source_path: item.source_path,
                    source_kind: map_local_payload_kind(&item.source_kind),
                    display_name: item.display_name,
                })
                .collect(),
            copy_destination: copy_destination.clone(),
            auto_install_chocolatey: apps.auto_install_chocolatey,
            continue_on_error: apps.continue_on_error,
            log_path: "C:\\BitOSDT\\Logs\\app-install.log".to_string(),
            progress_json_path: None,
        };

        tasks.push(TaskDefinition {
            id: Uuid::new_v4(),
            name: "Install Applications".to_string(),
            task_type: TaskType::InstallApps(app_config),
            order,
            enabled: true,
            continue_on_error: apps.continue_on_error,
            requires_reboot: false,
        });
        order += 10;
    }

    if shell_layout.has_work() {
        let shell_layout_script = generate_shell_layout_script(
            &shell_layout,
            copy_destination.as_deref(),
            defer_shell_layout_to_first_logon,
        )
        .map_err(|e| format!("Failed to generate shell layout task: {}", e))?;
        tasks.push(TaskDefinition {
            id: Uuid::new_v4(),
            name: "Apply Shell Layout".to_string(),
            task_type: TaskType::CustomScript(CustomScript {
                name: "Apply Shell Layout".to_string(),
                content: shell_layout_script,
                script_type: ScriptType::PowerShell,
                run_as_admin: true,
                continue_on_error: true,
                timeout_seconds: 0,
            }),
            order,
            enabled: true,
            continue_on_error: true,
            requires_reboot: false,
        });
        order += 10;
    }

    let update_effectively_enabled = windows_update.enabled
        && (windows_update.install_security_updates
            || windows_update.install_critical_updates
            || windows_update.install_driver_updates);
    if update_effectively_enabled {
        tasks.push(TaskDefinition {
            id: Uuid::new_v4(),
            name: "Windows Update".to_string(),
            task_type: TaskType::WindowsUpdate(WindowsUpdateConfig {
                enabled: true,
                include_drivers: windows_update.install_driver_updates,
                include_optional: !windows_update.exclude_optional,
                specific_kbs: vec![],
                timeout_minutes: 120,
                max_cycles: 3,
                reboot_behavior: map_windows_update_reboot(&windows_update.reboot_behavior),
                log_path: "C:\\BitOSDT\\Logs\\windows-update.log".to_string(),
            }),
            order,
            enabled: true,
            continue_on_error: true,
            requires_reboot: false,
        });
        order += 10;
    }

    if apps.enable_custom_scripts {
        for (index, script) in apps.custom_scripts.into_iter().enumerate() {
            if !script.enabled {
                continue;
            }
            if script.content.trim().is_empty() {
                return Err(format!(
                    "Custom script {} is enabled but script content is empty",
                    index + 1
                ));
            }

            let name = if script.name.trim().is_empty() {
                format!("Custom Script {}", index + 1)
            } else {
                script.name.trim().to_string()
            };

            tasks.push(TaskDefinition {
                id: Uuid::new_v4(),
                name: name.clone(),
                task_type: TaskType::CustomScript(CustomScript {
                    name,
                    content: script.content,
                    script_type: ScriptType::PowerShell,
                    run_as_admin: true,
                    continue_on_error: script.continue_on_error,
                    timeout_seconds: 0,
                }),
                order,
                enabled: true,
                continue_on_error: script.continue_on_error,
                requires_reboot: false,
            });
            order += 10;
        }
    }

    if tasks.is_empty() {
        return Ok(None);
    }

    Ok(Some(TaskSequence {
        id: Uuid::new_v4(),
        name: format!(
            "{} {} setup",
            request.windows_version, request.windows_build
        ),
        tasks,
        settings: TaskSettings {
            scripts_dir: "C:\\BitOSDT\\Tasks".to_string(),
            logs_dir: "C:\\BitOSDT\\Logs".to_string(),
            continue_on_error: true,
            create_completion_marker: true,
        },
    }))
}

pub fn parse_save_mode(request: &ImageBuildRequest) -> ImageSaveMode {
    match request
        .save_mode
        .as_deref()
        .unwrap_or("copy")
        .to_ascii_lowercase()
        .as_str()
    {
        "overwrite" => ImageSaveMode::Overwrite,
        _ => ImageSaveMode::Copy,
    }
}

pub fn build_wizard_state_json(request: &ImageBuildRequest) -> serde_json::Value {
    let request_language = request_language_for_metadata(request);
    let (
        full_iso_unc_path,
        full_iso_http_url,
        full_iso_unc_username,
        full_iso_unc_password,
        wds_runtime_source,
    ) = normalize_remote_sources_for_wizard_state(
        &request.output_type,
        request.full_iso_unc_path.as_ref(),
        request.full_iso_unc_username.as_ref(),
        request.full_iso_unc_password.as_ref(),
        request.full_iso_http_url.as_ref(),
        request.prompt_unc_credentials_at_runtime.unwrap_or(false),
    );
    json!({
        "currentStep": 0,
        "windowsVersion": {
            "name": request.windows_version.clone(),
            "build": request.windows_build.clone(),
            "edition": request.windows_edition.clone(),
            "language": request_language,
            "downloadUrl": request.download_url.clone(),
            "sourcePath": request.source_path.clone(),
            "sourceType": if request.source_path.as_ref().is_some_and(|value| !value.trim().is_empty()) { "local" } else { "cloud" }
        },
        "oobeConfig": request.oobe_config.clone(),
        "userAccounts": request.user_accounts.clone(),
        "domainJoin": request.domain_join.clone(),
        "autopilot": request.autopilot.clone(),
        "apps": request.apps.clone(),
        "windowsUpdate": request.windows_update.clone(),
        "groupPolicies": request.group_policies.clone(),
        "shellLayout": request.shell_layout.clone(),
        "output": {
            "outputType": request.output_type.clone(),
            "outputPath": request.output_path.clone(),
            "volumeLabel": request.volume_label.clone(),
            "deliveryMode": request.delivery_mode.clone().unwrap_or_else(|| "Simple".to_string()),
            "serverUrl": request.server_url.clone(),
            "pxeExportPath": request.pxe_export_path.clone(),
            "driverPaths": request.driver_paths.iter().map(|path| {
                json!({
                    "sourcePath": path,
                    "sourceKind": "Directory"
                })
            }).collect::<Vec<_>>(),
            "bootDriverUncPath": request.boot_driver_unc_path.clone().unwrap_or_default(),
            "applyDriversToOfflineWindows": request.apply_to_offline_windows.unwrap_or(false),
            "includeGui": request.include_gui.unwrap_or(true),
            "wdsRuntimeSource": wds_runtime_source,
            "fullIsoUncPath": full_iso_unc_path,
            "fullIsoUncUsername": full_iso_unc_username,
            "fullIsoUncPassword": full_iso_unc_password,
            "fullIsoHttpUrl": full_iso_http_url,
            "promptUncCredentialsAtRuntime": request.prompt_unc_credentials_at_runtime.unwrap_or(false)
        }
    })
}

pub fn persist_built_image(
    request: &ImageBuildRequest,
    produced_iso_path: &Path,
) -> Result<(), String> {
    let config = Config::load().map_err(|e| format!("Failed to load config: {}", e))?;
    let db = Database::new(&config.database_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    let now = Utc::now();
    let request_language = request_language_for_metadata(request);
    let wizard_state_json = Some(build_wizard_state_json(request));
    let mut image = Image {
        id: Uuid::new_v4(),
        name: format!(
            "{} {} {}",
            request.windows_version, request.windows_build, request.windows_edition
        ),
        description: Some("Generated from BitOSDT wizard".to_string()),
        os_info: OsInfo {
            os_type: infer_os_type(&request.windows_version),
            version: request.windows_build.clone(),
            architecture: Architecture::X64,
            language: request_language,
        },
        license: LicenseInfo {
            license_type: infer_license_type(&request.windows_edition),
            activation_type: None,
        },
        status: ImageStatus::Ready,
        created_at: now,
        updated_at: now,
        built_at: Some(now),
        workspace_path: None,
        wim_path: None,
        iso_path: Some(produced_iso_path.to_path_buf()),
        config: crate::core::DeployConfig {
            os_version: request.windows_build.clone(),
            ..Default::default()
        },
        wizard_state_json,
        size_bytes: std::fs::metadata(produced_iso_path)
            .ok()
            .map(|meta| meta.len()),
        hash_sha256: None,
    };

    if parse_save_mode(request) == ImageSaveMode::Overwrite {
        let existing_image_id = request
            .existing_image_id
            .as_deref()
            .ok_or_else(|| "existing_image_id is required when save_mode=overwrite".to_string())?;
        let parsed_id =
            Uuid::parse_str(existing_image_id).map_err(|e| format!("Invalid image id: {}", e))?;
        let existing = db
            .get_image(parsed_id)
            .map_err(|e| format!("Failed to load existing image: {}", e))?
            .ok_or_else(|| format!("Image not found for overwrite: {}", existing_image_id))?;
        image.id = existing.id;
        image.created_at = existing.created_at;
        if db
            .update_image(&image)
            .map_err(|e| format!("Failed to overwrite image metadata: {}", e))?
        {
            return Ok(());
        }
        return Err(format!(
            "Overwrite failed because image {} could not be updated.",
            existing_image_id
        ));
    }

    db.create_image(&image)
        .map_err(|e| format!("Failed to persist built image metadata: {}", e))
}

pub async fn build_image_with_context<F>(
    request: &ImageBuildRequest,
    context: &ImageBuildContext,
    mut progress_callback: F,
) -> Result<String, String>
where
    F: FnMut(BuildProgress) + Send,
{
    let apps: FrontendApps = parse_frontend(request.apps.clone(), "applications")?;
    let oobe: FrontendOobeConfig = parse_frontend(request.oobe_config.clone(), "OOBE")?;
    validate_lightweight_restrictions(&request.output_type, &oobe, &apps)?;

    let has_enabled_custom_installers = apps.custom_installers.iter().any(|item| item.enabled);
    if output_includes_lightweight(&request.output_type)
        && (has_enabled_custom_installers || has_local_payload_copy_work(&apps))
    {
        progress_callback(BuildProgress {
            step: "warning".to_string(),
            progress: 0,
            message: "Local file/folder payloads and custom installer payload handling are only applied to Full ISO output in this release. Lightweight output will skip them.".to_string(),
        });
    }

    match request.output_type.as_str() {
        "FullISO" => build_full_iso_with_context(request, context, progress_callback).await,
        "LightweightISO" => {
            build_lightweight_iso_with_context(request, context, progress_callback).await
        }
        "Both" => {
            progress_callback(BuildProgress {
                step: "init".to_string(),
                progress: 0,
                message: "Starting dual build process...".to_string(),
            });
            build_lightweight_iso_with_context(request, context, |progress| {
                progress_callback(progress)
            })
            .await?;
            progress_callback(BuildProgress {
                step: "midpoint".to_string(),
                progress: 50,
                message: "Lightweight ISO complete. Starting Full ISO build...".to_string(),
            });
            build_full_iso_with_context(request, context, progress_callback).await
        }
        other => Err(format!("Unsupported shared build output: {}", other)),
    }
}

pub async fn build_lightweight_iso_with_context<F>(
    request: &ImageBuildRequest,
    context: &ImageBuildContext,
    mut progress_callback: F,
) -> Result<String, String>
where
    F: FnMut(BuildProgress) + Send,
{
    let delivery_mode = resolve_delivery_mode(request);
    let driver_paths = validate_driver_paths(&request.driver_paths)?;
    let workspace =
        Config::configured_workspace_path().unwrap_or_else(|_| PathBuf::from("workspace"));
    let download_dir =
        Config::configured_download_path().unwrap_or_else(|_| PathBuf::from("downloads"));
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Failed to create workspace: {}", e))?;
    std::fs::create_dir_all(&download_dir)
        .map_err(|e| format!("Failed to create download directory: {}", e))?;

    let publish_path = resolve_lightweight_publish_path(request, delivery_mode, context)?;
    let runtime_server_url = resolve_lightweight_server_url(request, delivery_mode, context)?;
    let lightweight_source_plan =
        plan_lightweight_source(request, &workspace, &runtime_server_url)?;
    let source_image_index =
        resolve_lightweight_source_image_index(request, &lightweight_source_plan, &download_dir)?;
    if lightweight_source_plan.http_image_url.is_none()
        && lightweight_source_plan.unc_image_path.is_none()
    {
        return Err(
            "Lightweight deployment requires either a cloud download URL or a local ISO/ESD/WIM source."
                .to_string(),
        );
    }
    let output_path = if request.output_type == "Both" {
        let path = PathBuf::from(&request.output_path);
        let parent = path.parent().unwrap_or(Path::new("."));
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().unwrap_or_default().to_string_lossy();
        parent.join(format!("{}-Lightweight.{}", stem, ext))
    } else {
        PathBuf::from(&request.output_path)
    };

    let include_gui_requested = request
        .include_gui
        .unwrap_or(delivery_mode == DeliveryModeKind::Simple);
    let gui_executable = if include_gui_requested {
        context.gui_executable.clone()
    } else {
        None
    };

    let unattend_config = build_unattend_config(request)?;
    let autopilot_profile = build_autopilot_profile(request)?;
    let task_sequence = build_task_sequence(request)?;
    let runtime_domain_join = build_runtime_domain_join_config(request)?;

    let config = LightweightConfig {
        output_path: output_path.clone(),
        volume_label: request.volume_label.clone(),
        server_url: runtime_server_url.clone(),
        include_gui: include_gui_requested && gui_executable.is_some(),
        gui_executable,
        native_executable: context.native_runtime_executable.clone(),
        winpe_assets_dir: context.winpe_assets_dir.clone(),
        common_boot_driver_dir: context.common_boot_driver_dir.clone(),
        driver_cache_dir: Some(download_dir.join("drivers")),
        runtime_driver_policy: request.runtime_driver_policy.clone().unwrap_or_default(),
        runtime_driver_catalog: context.runtime_driver_catalog.clone(),
        http_image_url: lightweight_source_plan.http_image_url.clone(),
        unc_image_path: lightweight_source_plan.unc_image_path.clone(),
        wim_index: 1,
        source_image_index,
        windows_edition: request.windows_edition.clone(),
        unattend: unattend_config,
        autopilot: autopilot_profile,
        task_sequence,
        runtime_domain_join,
        os_version: request.windows_build.clone(),
        driver_paths: driver_paths.clone(),
        winpe_packages_dir: context.winpe_packages_dir.clone(),
        ui_dir: context.ui_dir.clone(),
        ..Default::default()
    };

    let mut builder = LightweightBuilder::new(workspace.clone())
        .map_err(|e| format!("Failed to create builder: {}", e))?;

    progress_callback(BuildProgress {
        step: "init".to_string(),
        progress: 0,
        message: "Starting Lightweight ISO build...".to_string(),
    });

    builder
        .build(&config, |progress, message| {
            progress_callback(BuildProgress {
                step: "build".to_string(),
                progress: progress as u32,
                message,
            });
        })
        .map_err(|e| format!("Build failed: {}", e))?;

    progress_callback(BuildProgress {
        step: "publish".to_string(),
        progress: 90,
        message: "Staging PXE/lightweight publish files...".to_string(),
    });
    let media_dir = workspace.join("winpe").join("media");
    let runtime_executable = if delivery_mode == DeliveryModeKind::Simple {
        context.native_runtime_executable.as_deref()
    } else {
        None
    };
    let manifest_json = if delivery_mode == DeliveryModeKind::Simple {
        Some(build_manifest_json(&runtime_server_url))
    } else {
        None
    };
    let publish_result = stage_lightweight_media_tree(
        &media_dir,
        &publish_path,
        runtime_executable,
        manifest_json.as_deref(),
    )
    .map_err(|e| format!("PXE publish staging failed: {}", e))?;
    if let Some(source) = lightweight_source_plan.published_source.as_ref() {
        let staged_source = stage_lightweight_published_source(&publish_path, source)?;
        progress_callback(BuildProgress {
            step: "publish".to_string(),
            progress: 93,
            message: format!("Deployment image staged at {}.", staged_source.display()),
        });
    }
    progress_callback(BuildProgress {
        step: "publish".to_string(),
        progress: 95,
        message: format!(
            "PXE/lightweight files staged at {} ({} files).",
            publish_result.destination.display(),
            publish_result.copied_files
        ),
    });

    if context.persist_built_image {
        persist_built_image(request, &output_path)?;
    }

    Ok(output_path.to_string_lossy().to_string())
}

pub async fn build_full_iso_with_context<F>(
    request: &ImageBuildRequest,
    context: &ImageBuildContext,
    mut progress_callback: F,
) -> Result<String, String>
where
    F: FnMut(BuildProgress) + Send,
{
    let download_dir =
        Config::configured_download_path().unwrap_or_else(|_| PathBuf::from("downloads"));
    let workspace =
        Config::configured_workspace_path().unwrap_or_else(|_| PathBuf::from("workspace"));
    std::fs::create_dir_all(&download_dir)
        .map_err(|e| format!("Failed to create download directory: {}", e))?;
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Failed to create workspace: {}", e))?;
    let resolved_adk = crate::core::resolve_adk_paths(None, "amd64");

    progress_callback(BuildProgress {
        step: "init".to_string(),
        progress: 0,
        message: "Starting full ISO build...".to_string(),
    });
    progress_callback(BuildProgress {
        step: "source".to_string(),
        progress: 5,
        message: "Locating Windows source...".to_string(),
    });

    let resolved_source =
        prepare_full_build_source(request, &workspace, &download_dir, |progress| {
            progress_callback(progress)
        })
        .await?;

    progress_callback(BuildProgress {
        step: "source".to_string(),
        progress: 20,
        message: format!(
            "Using source: {:?}",
            resolved_source.source_path.file_name().unwrap_or_default()
        ),
    });

    let unattend_config = build_unattend_config(request)?;
    let autopilot_profile = build_autopilot_profile(request)?;
    let task_sequence = build_task_sequence(request)?;
    let runtime_domain_join = build_runtime_domain_join_config(request)?;
    let output_path = PathBuf::from(&request.output_path);
    let (unc_image_path, http_image_url, unc_auth_username, unc_auth_password) =
        validate_full_iso_remote_sources(
            request.full_iso_unc_path.as_ref(),
            request.full_iso_unc_username.as_ref(),
            request.full_iso_unc_password.as_ref(),
            request.full_iso_http_url.as_ref(),
            request.prompt_unc_credentials_at_runtime.unwrap_or(false),
        )?;

    let full_iso_config = FullIsoBuildConfig {
        source_path: resolved_source.source_path.clone(),
        output_path: output_path.clone(),
        volume_label: request.volume_label.clone(),
        windows_version: request.windows_version.clone(),
        windows_build: request.windows_build.clone(),
        windows_edition: request.windows_edition.clone(),
        language: resolved_source.canonical_language.clone(),
        architecture: "amd64".to_string(),
        wim_index: if resolved_source.normalized_to_single_image_wim {
            1
        } else {
            resolved_source.source_image_index.unwrap_or(1)
        },
        target_disk: request.target_disk,
        disk_selection_policy: DiskSelectionPolicy::ConfigFirstSafeFallback,
        unattend: unattend_config,
        autopilot: autopilot_profile,
        task_sequence,
        runtime_domain_join,
        workspace: Some(workspace),
        download_dir: Some(download_dir.clone()),
        adk_paths: resolved_adk,
        winpe_assets_dir: context.winpe_assets_dir.clone(),
        winpe_packages_dir: context.winpe_packages_dir.clone(),
        ui_dir: context.ui_dir.clone(),
        native_executable: context.native_runtime_executable.clone(),
        common_boot_driver_dir: context.common_boot_driver_dir.clone(),
        runtime_driver_catalog: context.runtime_driver_catalog.clone(),
        runtime_driver_cache_source: Some(download_dir.join("drivers")),
        driver_paths: validate_driver_paths(&request.driver_paths)?,
        apply_drivers_to_offline_windows: request.apply_to_offline_windows.unwrap_or(false),
        runtime_driver_policy: request.runtime_driver_policy.clone().unwrap_or_default(),
        unc_image_path,
        unc_auth_username,
        unc_auth_password,
        http_image_url,
        prompt_unc_credentials_at_runtime: request.prompt_unc_credentials_at_runtime,
    };

    build_full_iso_core(&full_iso_config, |progress| {
        progress_callback(BuildProgress {
            step: progress.step,
            progress: progress.progress,
            message: progress.message,
        });
    })
    .map_err(|e| format!("Full ISO build failed: {}", e))?;

    if context.persist_built_image {
        persist_built_image(request, &output_path)?;
    }

    Ok(output_path.to_string_lossy().to_string())
}

async fn resolve_source_path_for_request<F>(
    request: &ImageBuildRequest,
    workspace: &Path,
    download_dir: &Path,
    download_language: &str,
    progress_callback: F,
) -> Result<PathBuf, String>
where
    F: FnMut(BuildProgress) + Send,
{
    if let Some(source) = request.source_path.as_ref() {
        let source_path = PathBuf::from(source);
        if !source_path.exists() {
            return Err(format!("Source file not found: {}", source));
        }
        let mut progress_callback = progress_callback;
        return resolve_local_source_media_path(&source_path, workspace, &mut progress_callback);
    }

    if let Some(download_url) = request.download_url.as_ref() {
        let progress_callback = Arc::new(Mutex::new(progress_callback));

        progress_callback
            .lock()
            .expect("build progress callback poisoned")
            (BuildProgress {
                step: "download".to_string(),
                progress: 5,
                message:
                    "Downloading Windows image from Microsoft CDN... [0.0% - 0 B/s - ETA: Calculating...]"
                        .to_string(),
            });

        let downloader = EsdDownloader::new_with_adk(download_dir.to_path_buf(), None)
            .map_err(|e| format!("Failed to create downloader: {}", e))?;
        progress_callback
            .lock()
            .expect("build progress callback poisoned")(BuildProgress {
            step: "download".to_string(),
            progress: 5,
            message: "Fetching file size...".to_string(),
        });
        let file_size = downloader.get_file_size(download_url).await.unwrap_or(0);
        let esd_info = EsdInfo {
            id: format!(
                "{}-{}-{}",
                request.windows_version, request.windows_build, request.windows_edition
            ),
            display_name: format!(
                "{} {} {}",
                request.windows_version, request.windows_build, request.windows_edition
            ),
            url: download_url.clone(),
            size_bytes: file_size,
            sha256: None,
            language: download_language.to_string(),
            architecture: "amd64".to_string(),
            version: request.windows_version.clone(),
            build: request.windows_build.clone(),
        };

        let last_emit = Arc::new(Mutex::new(Instant::now()));
        let progress_emitter = Arc::clone(&progress_callback);
        let downloaded_path = downloader
            .download_esd(&esd_info, move |progress| {
                let mut last = last_emit.lock().expect("download progress poisoned");
                let now = Instant::now();
                let should_emit = now.duration_since(*last).as_millis() >= 100
                    || progress.percent >= 99.9
                    || progress.percent < 0.1;
                if should_emit {
                    *last = now;
                    let percent = progress.percent as u32;
                    progress_emitter
                        .lock()
                        .expect("build progress callback poisoned")(
                        BuildProgress {
                            step: "download".to_string(),
                            progress: 5 + (percent * 10 / 100),
                            message: format!(
                                "Downloading: {:.1}% - {} - ETA: {}",
                                progress.percent,
                                progress.format_speed(),
                                progress.format_eta()
                            ),
                        },
                    );
                }
            })
            .await
            .map_err(|e| format!("Download failed: {}", e))?;

        // Validate that the downloaded ESD contains the requested edition
        progress_callback
            .lock()
            .expect("build progress callback poisoned")(BuildProgress {
            step: "source".to_string(),
            progress: 14,
            message: "Validating downloaded ESD editions...".to_string(),
        });

        let editions = EsdDownloader::validate_esd_contains_edition(
            &downloaded_path,
            &request.windows_edition,
        )
        .map_err(|e| {
            let channel_hint = if let Some(ref channel) = request.windows_channel {
                format!(" (selected channel: {})", channel)
            } else {
                String::new()
            };
            format!(
                "ESD edition validation failed{}. Error: {}",
                channel_hint, e
            )
        })?;

        info!(
            "ESD validated successfully. Available editions: {}",
            editions
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );

        progress_callback
            .lock()
            .expect("build progress callback poisoned")(BuildProgress {
            step: "source".to_string(),
            progress: 15,
            message: "Download complete.".to_string(),
        });
        return Ok(downloaded_path);
    }

    for candidate in [
        workspace.join("install.iso"),
        workspace.join("install.esd"),
        workspace.join("install.wim"),
    ] {
        if candidate.exists() {
            let mut progress_callback = progress_callback;
            return resolve_local_source_media_path(&candidate, workspace, &mut progress_callback);
        }
    }

    Err(
        "Windows source not found. Select an ISO/ESD/WIM file or choose cloud download."
            .to_string(),
    )
}

fn resolve_lightweight_server_url(
    request: &ImageBuildRequest,
    delivery_mode: DeliveryModeKind,
    context: &ImageBuildContext,
) -> Result<String, String> {
    match delivery_mode {
        DeliveryModeKind::Simple => Ok(context
            .simple_runtime_url
            .clone()
            .unwrap_or_else(default_simple_runtime_url)),
        DeliveryModeKind::Advanced => request
            .server_url
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Runtime server URL is required for Advanced PXE delivery.".to_string()),
    }
}

fn resolve_lightweight_publish_path(
    request: &ImageBuildRequest,
    delivery_mode: DeliveryModeKind,
    context: &ImageBuildContext,
) -> Result<PathBuf, String> {
    match delivery_mode {
        DeliveryModeKind::Simple => {
            if let Some(path) = context.simple_publish_path.clone() {
                Ok(path)
            } else {
                default_simple_publish_path()
            }
        }
        DeliveryModeKind::Advanced => request
            .pxe_export_path
            .clone()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                "PXE/WDS export path is required for Advanced PXE delivery.".to_string()
            }),
    }
}

fn validate_lightweight_restrictions(
    output_type: &str,
    oobe: &FrontendOobeConfig,
    apps: &FrontendApps,
) -> Result<(), String> {
    if !output_includes_lightweight(output_type) {
        return Ok(());
    }
    if validate_computer_name(oobe.computer_name.as_deref())?.is_some() {
        return Err(
            "Computer name customization is supported only for Full ISO in this release."
                .to_string(),
        );
    }
    if has_enabled_custom_post_scripts(apps) {
        return Err(
            "Custom post-install scripts are supported only for Full ISO in this release."
                .to_string(),
        );
    }
    Ok(())
}

fn has_enabled_custom_post_scripts(apps: &FrontendApps) -> bool {
    apps.enable_custom_scripts && apps.custom_scripts.iter().any(|script| script.enabled)
}

fn has_local_payload_copy_work(apps: &FrontendApps) -> bool {
    if !apps.copied_items.is_empty() {
        return true;
    }
    apps.custom_installers
        .iter()
        .filter(|installer| installer.enabled)
        .any(|installer| !installer.dependencies.is_empty())
}

fn shell_layout_should_defer_to_first_logon(
    apps: &FrontendApps,
    shell_layout: &ShellLayoutConfig,
) -> bool {
    if !shell_layout.has_work() {
        return false;
    }

    let has_winget_layout_items = shell_layout
        .items
        .iter()
        .any(|item| item.item_type.eq_ignore_ascii_case("winget"));
    if has_winget_layout_items {
        return true;
    }

    let has_custom_layout_items = shell_layout
        .items
        .iter()
        .any(|item| item.item_type.eq_ignore_ascii_case("custom"));
    let has_deferred_network_installers = apps.custom_installers.iter().any(|installer| {
        installer.enabled && matches!(installer.source_type.as_deref(), Some("NetworkDirectory"))
    });

    has_custom_layout_items && has_deferred_network_installers
}

fn validate_computer_name(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw_value) = value else {
        return Ok(None);
    };
    let trimmed = raw_value.trim();
    if trimmed.is_empty() || trimmed == "*" {
        return Ok(None);
    }
    if trimmed.len() > 15 {
        return Err("Computer name must be 1-15 characters.".to_string());
    }
    if trimmed.starts_with('-') || trimmed.ends_with('-') {
        return Err("Computer name cannot start or end with '-'.".to_string());
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("Computer name can only contain ASCII letters, numbers, and '-'.".to_string());
    }
    Ok(Some(trimmed.to_string()))
}

fn parse_frontend<T: for<'de> Deserialize<'de>>(
    value: serde_json::Value,
    label: &str,
) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("Invalid {} configuration: {}", label, e))
}

fn map_network_location(value: &str) -> NetworkLocation {
    match value {
        "Home" => NetworkLocation::Home,
        "Other" => NetworkLocation::Other,
        _ => NetworkLocation::Work,
    }
}

fn map_protect_your_pc(value: &str) -> ProtectYourPc {
    match value {
        "Custom" => ProtectYourPc::Custom,
        "Off" => ProtectYourPc::Off,
        _ => ProtectYourPc::Recommended,
    }
}

fn map_user_group(value: &str) -> UserGroup {
    match value {
        "Users" => UserGroup::Users,
        _ => UserGroup::Administrators,
    }
}

fn map_deployment_mode(value: &str) -> DeploymentMode {
    match value {
        "SelfDeploying" => DeploymentMode::SelfDeploying,
        "PreProvisioned" => DeploymentMode::PreProvisioned,
        _ => DeploymentMode::UserDriven,
    }
}

fn map_custom_installer_type(value: &str) -> InstallerType {
    match value {
        "Msi" => InstallerType::Msi,
        "Msix" => InstallerType::Msix,
        "Msp" => InstallerType::Msp,
        _ => InstallerType::Exe,
    }
}

fn map_custom_installer_source_type(value: Option<&str>) -> InstallerSourceType {
    match value {
        Some("EmbeddedFile") => InstallerSourceType::EmbeddedFile,
        Some("NetworkDirectory") => InstallerSourceType::NetworkDirectory,
        _ => InstallerSourceType::DirectPathOrUrl,
    }
}

fn map_windows_update_reboot(value: &str) -> RebootBehavior {
    match value {
        "NoReboot" => RebootBehavior::SuppressReboot,
        "ScheduleReboot" => RebootBehavior::PromptReboot,
        _ => RebootBehavior::AutoReboot,
    }
}

fn map_local_payload_kind(value: &str) -> LocalPayloadKind {
    match value {
        "Directory" => LocalPayloadKind::Directory,
        _ => LocalPayloadKind::File,
    }
}

fn infer_os_type(name: &str) -> OsType {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("server 2025") {
        OsType::WindowsServer2025
    } else if lowered.contains("server 2022") {
        OsType::WindowsServer2022
    } else if lowered.contains("10") {
        OsType::Windows10
    } else if lowered.contains("11") {
        OsType::Windows11
    } else {
        OsType::Other
    }
}

fn infer_license_type(edition: &str) -> LicenseType {
    match edition.to_ascii_lowercase().as_str() {
        "home" => LicenseType::Home,
        "enterprise" => LicenseType::Enterprise,
        "education" => LicenseType::Education,
        "ltsc" => LicenseType::Ltsc,
        _ => LicenseType::Pro,
    }
}

fn preferred_hostname() -> Option<String> {
    for key in ["BITOSDT_SIMPLE_HOSTNAME", "COMPUTERNAME", "HOSTNAME"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn local_ipv4_fallback() -> Option<String> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    let address = socket.local_addr().ok()?;
    let ip = address.ip();
    if ip.is_ipv4() {
        Some(ip.to_string())
    } else {
        None
    }
}

pub fn resolve_cloud_catalog_entry(
    release: &str,
    language: &str,
    arch: &str,
) -> Result<(String, String), String> {
    let config = Config::load().map_err(|e| format!("Failed to load config: {}", e))?;
    let db = Database::new(&config.database_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    if let Ok(entries) =
        db.get_os_versions_filtered(None, Some(release), Some(arch), Some(language))
    {
        if let Some(entry) = entries.into_iter().next() {
            return Ok((entry.display_name, entry.download_url));
        }
    }

    let built_in = get_builtin_os_catalog()
        .into_iter()
        .find(|entry| {
            entry.version.eq_ignore_ascii_case(release)
                && entry
                    .languages
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(language))
        })
        .ok_or_else(|| {
            format!(
                "No catalog entry found for release={}, language={}, arch={}",
                release, language, arch
            )
        })?;

    Ok((built_in.display_name(), built_in.esd_url))
}

#[cfg(test)]
mod tests {
    use super::{
        build_task_sequence, build_unattend_config, build_wizard_state_json,
        cached_download_path_for_url, normalized_wim_paths, resolve_requested_language_tag,
        ImageBuildRequest,
    };
    use crate::config::UserGroup;
    use crate::policy::empty_group_policy_selection_value;
    use crate::tasks::TaskType;
    use serde_json::json;
    use std::path::{Path, PathBuf};

    fn make_request(output_type: &str) -> ImageBuildRequest {
        ImageBuildRequest {
            windows_version: "Windows 11".to_string(),
            windows_build: "24H2".to_string(),
            windows_edition: "Pro".to_string(),
            windows_channel: None,
            language: Some("en-US".to_string()),
            output_type: output_type.to_string(),
            output_path: "C:\\BitOSDT\\test.iso".to_string(),
            volume_label: "BITOSDT".to_string(),
            source_path: None,
            download_url: Some("https://example.test/install.esd".to_string()),
            target_disk: None,
            delivery_mode: Some("Simple".to_string()),
            server_url: None,
            driver_paths: Vec::new(),
            boot_driver_unc_path: None,
            apply_to_offline_windows: Some(false),
            runtime_driver_policy: None,
            pxe_export_path: None,
            full_iso_unc_path: None,
            full_iso_unc_username: None,
            full_iso_unc_password: None,
            full_iso_http_url: None,
            prompt_unc_credentials_at_runtime: None,
            include_gui: Some(true),
            existing_image_id: None,
            save_mode: Some("copy".to_string()),
            oobe_config: json!({
                "skipMachineOobe": false,
                "skipUserOobe": false,
                "hideEula": true,
                "hideWirelessSetup": true,
                "hideLocalAccountScreen": false,
                "hideOnlineAccountScreens": true,
                "networkLocation": "Work",
                "protectYourPc": "Recommended",
                "computerName": ""
            }),
            user_accounts: Vec::new(),
            domain_join: json!({
                "enabled": false,
                "domain": "",
                "username": "",
                "password": "",
                "ouPath": null,
                "promptForDomainCredentialsAtRuntime": false
            }),
            autopilot: json!({
                "enabled": false,
                "tenantId": "",
                "deploymentMode": "UserDriven",
                "skipUserOobe": true,
                "skipDeviceOobe": true,
                "allowWhiteglove": false,
                "groupTag": null
            }),
            apps: json!({
                "wingetPackages": [],
                "chocolateyPackages": [],
                "customInstallers": [],
                "copiedItems": [],
                "copyDestination": "",
                "autoInstallChocolatey": true,
                "continueOnError": true,
                "enableCustomScripts": false,
                "customScripts": []
            }),
            windows_update: json!({
                "enabled": true,
                "installSecurityUpdates": true,
                "installCriticalUpdates": true,
                "installDriverUpdates": false,
                "excludePreview": true,
                "excludeOptional": true,
                "rebootBehavior": "NoReboot"
            }),
            group_policies: empty_group_policy_selection_value(),
            shell_layout: crate::build::empty_shell_layout_value(),
        }
    }

    #[test]
    fn resolve_requested_language_tag_canonicalizes_french_locale() {
        let mut request = make_request("FullISO");
        request.language = Some("fr-fr".to_string());

        let language = resolve_requested_language_tag(&request).expect("canonical language");
        assert_eq!(language, "fr-FR");
    }

    #[test]
    fn build_wizard_state_json_uses_canonical_language() {
        let mut request = make_request("FullISO");
        request.language = Some("fr-fr".to_string());

        let wizard_state = build_wizard_state_json(&request);
        assert_eq!(wizard_state["windowsVersion"]["language"], "fr-FR");
    }

    #[test]
    fn build_unattend_config_allows_default_oobe_without_local_admin() {
        let request = make_request("FullISO");
        let config = build_unattend_config(&request).expect("default oobe should be valid");

        assert!(!config.oobe.skip_machine_oobe);
        assert!(!config.oobe.skip_user_oobe);
    }

    #[test]
    fn build_unattend_config_rejects_skip_user_oobe_without_local_admin() {
        let mut request = make_request("FullISO");
        request.oobe_config = json!({
            "skipMachineOobe": false,
            "skipUserOobe": true,
            "hideEula": true,
            "hideWirelessSetup": true,
            "hideLocalAccountScreen": false,
            "hideOnlineAccountScreens": true,
            "networkLocation": "Work",
            "protectYourPc": "Recommended",
            "computerName": ""
        });

        let err = build_unattend_config(&request).expect_err("missing local admin should fail");
        assert!(err.contains("Skip User OOBE requires at least one local administrator account"));
    }

    #[test]
    fn build_unattend_config_allows_skip_user_oobe_with_local_admin() {
        let mut request = make_request("FullISO");
        request.oobe_config = json!({
            "skipMachineOobe": false,
            "skipUserOobe": true,
            "hideEula": true,
            "hideWirelessSetup": true,
            "hideLocalAccountScreen": false,
            "hideOnlineAccountScreens": true,
            "networkLocation": "Work",
            "protectYourPc": "Recommended",
            "computerName": ""
        });
        request.user_accounts = vec![json!({
            "username": "deploy-admin",
            "password": "Secret123!",
            "displayName": "Deployment Admin",
            "group": "Administrators",
            "passwordNeverExpires": true,
            "requirePasswordChange": false
        })];

        let config =
            build_unattend_config(&request).expect("local admin should satisfy skip user oobe");
        assert!(config.oobe.skip_user_oobe);
        assert_eq!(config.users.len(), 1);
        assert!(matches!(config.users[0].group, UserGroup::Administrators));
    }

    #[test]
    fn build_unattend_config_allows_runtime_prompted_domain_join_with_blank_fields() {
        let mut request = make_request("FullISO");
        request.domain_join = json!({
            "enabled": true,
            "domain": "",
            "username": "",
            "password": "",
            "ouPath": null,
            "promptForDomainCredentialsAtRuntime": true
        });

        let config =
            build_unattend_config(&request).expect("runtime prompted domain join should be valid");
        assert!(config.domain_join.is_none());
    }

    #[test]
    fn build_runtime_domain_join_config_preserves_defaults_for_winpe_prompt() {
        let mut request = make_request("FullISO");
        request.domain_join = json!({
            "enabled": true,
            "domain": "contoso.local",
            "username": "",
            "password": "",
            "ouPath": "OU=Devices,DC=contoso,DC=local",
            "promptForDomainCredentialsAtRuntime": true
        });

        let config = super::build_runtime_domain_join_config(&request)
            .expect("runtime domain join config")
            .expect("enabled config");
        assert!(config.prompt_for_credentials_at_runtime);
        assert_eq!(config.default_domain.as_deref(), Some("contoso.local"));
        assert_eq!(
            config.default_ou_path.as_deref(),
            Some("OU=Devices,DC=contoso,DC=local")
        );
    }

    #[test]
    fn normalized_wim_paths_use_temp_export_when_source_is_workspace_install_wim() {
        let workspace = PathBuf::from("C:\\BitOSDT\\workspace");
        let source_path = workspace.join("install.wim");

        let (normalized_wim, export_wim) = normalized_wim_paths(&source_path, &workspace);
        assert_eq!(normalized_wim, workspace.join("install.wim"));
        assert_eq!(export_wim, workspace.join("install.normalized.wim"));
    }

    #[test]
    fn normalized_wim_paths_export_directly_when_source_differs_from_workspace_install_wim() {
        let workspace = PathBuf::from("C:\\BitOSDT\\workspace");
        let source_path = Path::new("D:\\Downloads\\fr-fr.esd");

        let (normalized_wim, export_wim) = normalized_wim_paths(source_path, &workspace);
        assert_eq!(normalized_wim, workspace.join("install.wim"));
        assert_eq!(export_wim, workspace.join("install.wim"));
    }

    #[test]
    fn cached_download_path_for_url_uses_remote_filename() {
        let path = cached_download_path_for_url(
            Path::new("C:\\BitOSDT\\Downloads"),
            "https://cdn.example.test/files/win11-fr-fr.esd",
            "fallback.esd",
        );

        assert_eq!(
            path,
            PathBuf::from("C:\\BitOSDT\\Downloads").join("win11-fr-fr.esd")
        );
    }

    #[test]
    fn build_wizard_state_json_preserves_group_policies() {
        let mut request = make_request("FullISO");
        request.group_policies = json!({
            "selectedPolicyIds": ["curated-disable-telemetry"],
            "customRegistryEntries": [{
                "id": "custom-1",
                "keyPath": "HKLM:\\SOFTWARE\\Policies\\Test",
                "valueName": "Enabled",
                "valueType": "dword",
                "valueData": "1"
            }],
            "lastAppliedPresetId": "builtin-privacy-hardened",
            "lastAppliedPresetName": "Privacy Hardened"
        });

        let wizard_state = build_wizard_state_json(&request);
        assert_eq!(
            wizard_state["groupPolicies"]["selectedPolicyIds"][0],
            "curated-disable-telemetry"
        );
        assert_eq!(
            wizard_state["groupPolicies"]["customRegistryEntries"][0]["keyPath"],
            "HKLM:\\SOFTWARE\\Policies\\Test"
        );
    }

    #[test]
    fn build_wizard_state_json_preserves_shell_layout() {
        let mut request = make_request("FullISO");
        request.shell_layout = json!({
            "enabled": true,
            "items": [{
                "id": "winget:Microsoft.WindowsTerminal",
                "label": "Windows Terminal",
                "itemType": "winget",
                "sourceRef": "Microsoft.WindowsTerminal",
                "desktop": true,
                "start": true,
                "taskbar": false
            }]
        });

        let wizard_state = build_wizard_state_json(&request);
        assert_eq!(wizard_state["shellLayout"]["enabled"], true);
        assert_eq!(
            wizard_state["shellLayout"]["items"][0]["label"],
            "Windows Terminal"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn build_task_sequence_inserts_policy_baseline_first() {
        let mut request = make_request("FullISO");
        request.group_policies = json!({
            "selectedPolicyIds": [],
            "customRegistryEntries": [{
                "id": "custom-1",
                "keyPath": "HKLM:\\SOFTWARE\\Policies\\BitOSDT",
                "valueName": "Enabled",
                "valueType": "DWord",
                "valueData": "1"
            }],
            "lastAppliedPresetId": null,
            "lastAppliedPresetName": null
        });
        request.apps = json!({
            "wingetPackages": [{
                "packageId": "Mozilla.Firefox",
                "version": null,
                "customArgs": null,
                "enabled": true
            }],
            "chocolateyPackages": [],
            "customInstallers": [],
            "copiedItems": [],
            "copyDestination": "",
            "autoInstallChocolatey": true,
            "continueOnError": true,
            "enableCustomScripts": false,
            "customScripts": []
        });

        let sequence = build_task_sequence(&request)
            .expect("task sequence should build")
            .expect("task sequence should be present");

        assert_eq!(sequence.tasks[0].name, "Apply Local Policy Baseline");
        assert!(matches!(
            sequence.tasks[0].task_type,
            TaskType::SetRegistry(_)
        ));
        assert_eq!(sequence.tasks[0].order, 10);
        assert_eq!(sequence.tasks[1].name, "Install Applications");
        assert_eq!(sequence.tasks[2].name, "Windows Update");
    }

    #[test]
    fn build_task_sequence_adds_shell_layout_task_after_app_install() {
        let mut request = make_request("FullISO");
        request.apps = json!({
            "wingetPackages": [{
                "packageId": "Microsoft.WindowsTerminal",
                "version": null,
                "customArgs": null,
                "enabled": true
            }],
            "chocolateyPackages": [],
            "customInstallers": [],
            "copiedItems": [],
            "copyDestination": "",
            "autoInstallChocolatey": true,
            "continueOnError": true,
            "enableCustomScripts": false,
            "customScripts": []
        });
        request.shell_layout = json!({
            "enabled": true,
            "items": [{
                "id": "winget:Microsoft.WindowsTerminal",
                "label": "Windows Terminal",
                "itemType": "winget",
                "sourceRef": "Microsoft.WindowsTerminal",
                "desktop": true,
                "start": true,
                "taskbar": true
            }]
        });

        let sequence = build_task_sequence(&request)
            .expect("task sequence should build")
            .expect("task sequence should be present");

        assert_eq!(sequence.tasks[0].name, "Install Applications");
        assert_eq!(sequence.tasks[1].name, "Apply Shell Layout");
        match &sequence.tasks[1].task_type {
            TaskType::CustomScript(script) => {
                assert!(script.content.contains("LayoutModification.xml"));
                assert!(script.content.contains("$ShouldDeferToFirstLogon = $true"));
                assert!(script.content.contains("Install-WingetApps.ps1"));
            }
            other => panic!("expected shell layout custom script, got {:?}", other),
        }
    }

    #[test]
    fn assess_sign_in_readiness_warns_when_external_identity_is_required() {
        let readiness = crate::build::assess_sign_in_readiness(false, 0, true, true);
        assert_eq!(readiness.level, crate::build::SignInReadinessLevel::Warning);
        assert!(readiness.relies_on_external_identity);
    }
}
