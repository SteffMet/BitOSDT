// Prevents additional console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use bitosdt::build::{
    assess_sign_in_readiness, build_image_with_context, empty_shell_layout_value,
    generate_shell_layout_script, prepare_full_build_source, validate_driver_paths_with_network,
    ImageBuildContext as SharedImageBuildContext, ImageBuildRequest as SharedImageBuildRequest,
    RuntimeDomainJoinConfig, ShellLayoutConfig, SignInReadiness, SignInReadinessLevel,
};
use bitosdt::core::{run_dism, set_build_runtime_hooks, BuildRuntimeHooks, TrackedBuildProcess};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{Manager, Position, Size};
use uuid::Uuid;

mod lightweight_host;
mod oobe_profiles;
mod ppkg;
mod updater;
mod usb;

use lightweight_host::{
    build_manifest_json, default_simple_publish_path, default_simple_runtime_url,
    ensure_lightweight_host_running, resolve_simple_delivery_defaults,
    stop_lightweight_host as stop_embedded_lightweight_host, LightweightHostState,
    LightweightHostStatus, SimpleDeliveryDefaults,
};
use updater::{
    check_for_update, current_app_release_metadata, AppReleaseMetadata, UpdateCheckResponse,
    DEFAULT_UPDATE_ENDPOINT,
};

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub url: String,
    pub output_path: String,
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoRequest {
    pub source_dir: String,
    pub output_path: String,
    pub volume_label: String,
}

pub struct DownloadCancelFlag(Arc<AtomicBool>);

impl DownloadCancelFlag {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

impl Default for DownloadCancelFlag {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildProcessInfo {
    pid: u32,
    executable: String,
    command_line: String,
    role: String,
}

#[derive(Default)]
struct BuildProcessRegistry {
    active: bool,
    cancel_requested: bool,
    processes: BTreeMap<u32, BuildProcessInfo>,
}

#[derive(Clone)]
pub struct BuildProcessState(Arc<Mutex<BuildProcessRegistry>>);

impl BuildProcessState {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(BuildProcessRegistry::default())))
    }

    pub fn begin_build(&self) -> Result<(), String> {
        let mut registry = self
            .0
            .lock()
            .map_err(|_| "Build process state is unavailable".to_string())?;
        if registry.active {
            return Err("A build is already in progress.".to_string());
        }
        registry.active = true;
        registry.cancel_requested = false;
        registry.processes.clear();
        Ok(())
    }

    pub fn finish_build(&self) {
        if let Ok(mut registry) = self.0.lock() {
            registry.active = false;
            registry.cancel_requested = false;
            registry.processes.clear();
        }
    }

    pub fn is_active(&self) -> bool {
        self.0
            .lock()
            .map(|registry| registry.active)
            .unwrap_or(false)
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.0
            .lock()
            .map(|registry| registry.cancel_requested)
            .unwrap_or(false)
    }

    pub fn register_process(&self, process: TrackedBuildProcess) {
        if let Ok(mut registry) = self.0.lock() {
            if !registry.active {
                return;
            }
            registry.processes.insert(
                process.pid,
                BuildProcessInfo {
                    pid: process.pid,
                    executable: process.executable,
                    command_line: process.command_line,
                    role: process.role,
                },
            );
        }
    }

    pub fn unregister_process(&self, pid: u32) {
        if let Ok(mut registry) = self.0.lock() {
            registry.processes.remove(&pid);
        }
    }

    fn request_cancel(&self) -> Vec<BuildProcessInfo> {
        let processes = {
            let mut registry = match self.0.lock() {
                Ok(registry) => registry,
                Err(_) => return Vec::new(),
            };
            if !registry.active {
                return Vec::new();
            }
            registry.cancel_requested = true;
            registry.processes.values().cloned().collect::<Vec<_>>()
        };

        for process in &processes {
            let _ = stop_process_tree(process.pid);
        }

        processes
    }
}

impl Default for BuildProcessState {
    fn default() -> Self {
        Self::new()
    }
}

fn stop_process_tree(pid: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .map_err(|e| format!("Failed to stop process {}: {}", pid, e))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{} {}", stdout.trim(), stderr.trim()).to_ascii_lowercase();
        if combined.contains("not found")
            || combined.contains("there is no running instance")
            || combined.contains("not recognized as an internal")
        {
            return Ok(());
        }

        return Err(format!(
            "Failed to stop process {}: {}",
            pid,
            combined.trim()
        ));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = pid;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BuildWorkspaceRecoveryStatus {
    Ok,
    LockedWithMatches,
    LockedWithoutMatches,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildWorkspaceRecoveryProcess {
    pid: u32,
    executable: String,
    command_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildWorkspaceRecoveryResponse {
    status: BuildWorkspaceRecoveryStatus,
    message: String,
    locked_path: Option<String>,
    processes: Vec<BuildWorkspaceRecoveryProcess>,
}

// ============================================================================
// Commands
// ============================================================================

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn get_app_release_metadata() -> AppReleaseMetadata {
    current_app_release_metadata()
}

#[tauri::command]
fn get_system_info() -> SystemInfo {
    SystemInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

#[tauri::command]
async fn cancel_download(app_handle: tauri::AppHandle) -> Result<(), String> {
    if let Some(flag) = app_handle.try_state::<DownloadCancelFlag>() {
        flag.cancel();
        Ok(())
    } else {
        Err("Cancel flag state not found".to_string())
    }
}

#[tauri::command]
async fn cancel_build(app_handle: tauri::AppHandle) -> Result<(), String> {
    if let Some(flag) = app_handle.try_state::<DownloadCancelFlag>() {
        flag.cancel();
    }

    if let Some(state) = app_handle.try_state::<BuildProcessState>() {
        state.request_cancel();
        Ok(())
    } else {
        Err("Build process state not found".to_string())
    }
}

#[tauri::command]
async fn get_simple_delivery_defaults() -> Result<SimpleDeliveryDefaults, String> {
    resolve_simple_delivery_defaults()
}

#[tauri::command]
async fn get_lightweight_host_status(
    app_handle: tauri::AppHandle,
) -> Result<LightweightHostStatus, String> {
    let state = app_handle.state::<LightweightHostState>();
    Ok(state.status())
}

#[tauri::command]
async fn start_lightweight_host(
    app_handle: tauri::AppHandle,
) -> Result<LightweightHostStatus, String> {
    let defaults = resolve_simple_delivery_defaults()?;
    let state = app_handle.state::<LightweightHostState>();
    ensure_lightweight_host_running(
        state.inner(),
        Path::new(&defaults.publish_path),
        &defaults.runtime_url,
    )
    .await
}

#[tauri::command]
async fn stop_lightweight_host(app_handle: tauri::AppHandle) -> Result<(), String> {
    let state = app_handle.state::<LightweightHostState>();
    stop_embedded_lightweight_host(state.inner()).await
}

#[tauri::command]
async fn check_for_app_update() -> Result<UpdateCheckResponse, String> {
    let metadata = current_app_release_metadata();
    check_for_update(DEFAULT_UPDATE_ENDPOINT, &metadata).await
}

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
    pub runtime_driver_policy: Option<bitosdt::core::RuntimeDriverPolicy>,
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
    #[serde(default = "bitosdt::policy::empty_group_policy_selection_value")]
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
enum DeliveryMode {
    Simple,
    Advanced,
}

const WDS_EXPORT_ROOT: &str = r"C:\BitOSDT\WDS";
const GROUP_POLICY_PRESETS_SETTING_KEY: &str = "group_policy_presets_json";

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

#[derive(Debug, Clone, Serialize)]
struct WdsExportManifest {
    export_folder: String,
    boot_wim_path: String,
    payload_path: String,
    expected_payload_size_bytes: u64,
    expected_payload_sha256: String,
    expected_payload_file_name: Option<String>,
    runtime_source_kind: String,
    runtime_source_value: String,
    runtime_unc_path: Option<String>,
    runtime_unc_auth_configured: bool,
    runtime_http_url: Option<String>,
    windows_version: String,
    windows_build: String,
    windows_edition: String,
    source_path: String,
    sign_in_readiness: SignInReadiness,
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

#[derive(Debug, Clone, Serialize)]
struct UiImage {
    id: String,
    name: String,
    description: String,
    os_type: String,
    os_version: String,
    os_architecture: String,
    os_language: String,
    license_type: String,
    status: String,
    created_at: String,
    updated_at: String,
    size_bytes: Option<u64>,
    iso_path: Option<String>,
    has_saved_wizard_state: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ImageSaveMode {
    Overwrite,
    Copy,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiImageEditPayload {
    image: UiImage,
    wizard_state: serde_json::Value,
    legacy_defaults_applied: bool,
    legacy_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct FeatureStatus {
    windows_source_selection: bool,
    oobe_configuration: bool,
    domain_join: bool,
    autopilot_integration: bool,
    application_installation: bool,
    windows_update: bool,
    full_iso_output: bool,
    lightweight_iso: bool,
    image_manager: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DriverSyncResponse {
    started: bool,
    synced_sources: u32,
    errors: Vec<String>,
}

#[tauri::command]
async fn start_esd_download(request: DownloadRequest) -> Result<String, String> {
    use bitosdt::download::{EsdDownloader, EsdInfo};

    // Create download directory if it doesn't exist
    let download_path = std::path::Path::new(&request.output_path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    let downloader = EsdDownloader::new(download_path)
        .map_err(|e| format!("Failed to create downloader: {}", e))?;

    // Parse ESD info from URL
    let esd_info = EsdInfo {
        id: "custom".to_string(),
        display_name: "Windows Image".to_string(),
        url: request.url.clone(),
        size_bytes: 0, // Will be determined during download
        sha256: request.expected_hash,
        language: "en-US".to_string(),
        architecture: "x64".to_string(),
        version: "Unknown".to_string(),
        build: "Unknown".to_string(),
    };

    // Download the ESD file
    let downloaded_path = downloader
        .download_esd(&esd_info, |_progress| {
            // Progress callback - in real implementation, emit to frontend via Tauri event
        })
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    Ok(format!("Downloaded to: {:?}", downloaded_path))
}

fn map_network_location(value: &str) -> bitosdt::config::NetworkLocation {
    match value {
        "Home" => bitosdt::config::NetworkLocation::Home,
        "Other" => bitosdt::config::NetworkLocation::Other,
        _ => bitosdt::config::NetworkLocation::Work,
    }
}

fn map_protect_your_pc(value: &str) -> bitosdt::config::ProtectYourPc {
    match value {
        "Custom" => bitosdt::config::ProtectYourPc::Custom,
        "Off" => bitosdt::config::ProtectYourPc::Off,
        _ => bitosdt::config::ProtectYourPc::Recommended,
    }
}

fn map_user_group(value: &str) -> bitosdt::config::UserGroup {
    match value {
        "Users" => bitosdt::config::UserGroup::Users,
        _ => bitosdt::config::UserGroup::Administrators,
    }
}

fn map_deployment_mode(value: &str) -> bitosdt::config::DeploymentMode {
    match value {
        "SelfDeploying" => bitosdt::config::DeploymentMode::SelfDeploying,
        "PreProvisioned" => bitosdt::config::DeploymentMode::PreProvisioned,
        _ => bitosdt::config::DeploymentMode::UserDriven,
    }
}

fn map_custom_installer_type(value: &str) -> bitosdt::tasks::InstallerType {
    match value {
        "Msi" => bitosdt::tasks::InstallerType::Msi,
        "Msix" => bitosdt::tasks::InstallerType::Msix,
        "Msp" => bitosdt::tasks::InstallerType::Msp,
        _ => bitosdt::tasks::InstallerType::Exe,
    }
}

fn map_custom_installer_source_type(value: Option<&str>) -> bitosdt::tasks::InstallerSourceType {
    match value {
        Some("EmbeddedFile") => bitosdt::tasks::InstallerSourceType::EmbeddedFile,
        Some("NetworkDirectory") => bitosdt::tasks::InstallerSourceType::NetworkDirectory,
        _ => bitosdt::tasks::InstallerSourceType::DirectPathOrUrl,
    }
}

fn map_windows_update_reboot(value: &str) -> bitosdt::tasks::RebootBehavior {
    match value {
        "NoReboot" => bitosdt::tasks::RebootBehavior::SuppressReboot,
        "ScheduleReboot" => bitosdt::tasks::RebootBehavior::PromptReboot,
        _ => bitosdt::tasks::RebootBehavior::AutoReboot,
    }
}

fn map_local_payload_kind(value: &str) -> bitosdt::tasks::LocalPayloadKind {
    match value {
        "Directory" => bitosdt::tasks::LocalPayloadKind::Directory,
        _ => bitosdt::tasks::LocalPayloadKind::File,
    }
}

fn default_true() -> bool {
    true
}

fn output_includes_lightweight(output_type: &str) -> bool {
    matches!(output_type, "LightweightISO" | "Both")
}

fn resolve_delivery_mode(request: &ImageBuildRequest) -> DeliveryMode {
    match request.delivery_mode.as_deref() {
        Some("Advanced") => DeliveryMode::Advanced,
        _ => DeliveryMode::Simple,
    }
}

fn validate_driver_paths(paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut resolved = Vec::new();
    for raw_path in paths {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with(r"\\") {
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

fn validate_unc_runtime_credentials_strict(
    unc_path: Option<&String>,
    unc_username: Option<&String>,
    unc_password: Option<&String>,
    path_label: &str,
) -> Result<(Option<String>, Option<String>), String> {
    validate_unc_runtime_credentials(unc_path, unc_username, unc_password, path_label, false)
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

fn validate_full_iso_remote_sources(
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

fn validate_wds_pxe_runtime_source(
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

fn trim_optional_string(value: Option<&String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
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

fn detect_invalid_saved_wds_runtime_warning(wizard_state: &serde_json::Value) -> Option<String> {
    let output = wizard_state.get("output")?;
    if output.get("outputType")?.as_str()? != "WDSPXE" {
        return None;
    }

    let unc = output
        .get("fullIsoUncPath")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let http = output
        .get("fullIsoHttpUrl")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if unc.is_some() && http.is_some() {
        Some(
            "Saved WDS/PXE profile contained both UNC and HTTP runtime paths. BitOSDT kept the UNC path selected in the editor and cleared the HTTP path until the profile is resaved."
                .to_string(),
        )
    } else {
        None
    }
}

fn resolve_lightweight_server_url(
    request: &ImageBuildRequest,
    delivery_mode: DeliveryMode,
) -> Result<String, String> {
    match delivery_mode {
        DeliveryMode::Simple => Ok(default_simple_runtime_url()),
        DeliveryMode::Advanced => request
            .server_url
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Runtime server URL is required for Advanced PXE delivery.".to_string()),
    }
}

fn resolve_lightweight_publish_path(
    request: &ImageBuildRequest,
    delivery_mode: DeliveryMode,
) -> Result<PathBuf, String> {
    match delivery_mode {
        DeliveryMode::Simple => default_simple_publish_path(),
        DeliveryMode::Advanced => request
            .pxe_export_path
            .clone()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                "PXE/WDS export path is required for Advanced PXE delivery.".to_string()
            }),
    }
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
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Err("Computer name can only contain ASCII letters, numbers, and '-'.".to_string());
    }

    Ok(Some(trimmed.to_string()))
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

fn parse_frontend<T: for<'de> Deserialize<'de>>(
    value: serde_json::Value,
    label: &str,
) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("Invalid {} configuration: {}", label, e))
}

fn resolve_language_settings(request: &ImageBuildRequest) -> Result<(String, String), String> {
    let language = request.language.as_deref().unwrap_or("en-US");
    bitosdt::config::resolve_unattend_locale_settings(language).map_err(|e| e.to_string())
}

fn build_unattend_config(
    request: &ImageBuildRequest,
) -> Result<bitosdt::config::UnattendConfig, String> {
    let oobe: FrontendOobeConfig = parse_frontend(request.oobe_config.clone(), "OOBE")?;
    let user_accounts: Vec<FrontendUserAccount> = request
        .user_accounts
        .iter()
        .cloned()
        .map(|v| parse_frontend(v, "user account"))
        .collect::<Result<Vec<_>, _>>()?;
    let domain_join: FrontendDomainJoin =
        parse_frontend(request.domain_join.clone(), "domain join")?;
    let (language, input_locale) = resolve_language_settings(request)?;
    let computer_name = validate_computer_name(oobe.computer_name.as_deref())?;
    let prompt_domain_credentials_at_runtime = domain_join
        .prompt_for_domain_credentials_at_runtime
        .unwrap_or(false);

    let local_admin_count = user_accounts
        .iter()
        .filter(|user| {
            matches!(
                map_user_group(&user.group),
                bitosdt::config::UserGroup::Administrators
            )
        })
        .count() as u32;
    if let Err(error) = bitosdt::build::validate_sign_in_readiness(
        oobe.skip_user_oobe,
        local_admin_count,
        false,
        false,
    ) {
        return Err(error);
    }

    if domain_join.enabled
        && !prompt_domain_credentials_at_runtime
        && (domain_join.domain.trim().is_empty()
            || domain_join.username.trim().is_empty()
            || domain_join.password.trim().is_empty())
    {
        return Err("Domain Join is enabled but required fields are missing".to_string());
    }

    Ok(bitosdt::config::UnattendConfig {
        language,
        input_locale,
        timezone: "Pacific Standard Time".to_string(),
        oobe: bitosdt::config::OobeConfig {
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
            .map(|u| bitosdt::config::UserAccountConfig {
                username: u.username,
                password: u.password,
                display_name: u.display_name,
                group: map_user_group(&u.group),
                password_never_expires: u.password_never_expires,
                require_password_change: u.require_password_change,
            })
            .collect(),
        administrator_password: None,
        computer_name,
        product_key: None,
        domain_join: if domain_join.enabled && !prompt_domain_credentials_at_runtime {
            Some(bitosdt::config::DomainJoinConfig {
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

    let default_domain = domain_join.domain.trim();

    Ok(Some(RuntimeDomainJoinConfig {
        enabled: true,
        prompt_for_credentials_at_runtime: domain_join
            .prompt_for_domain_credentials_at_runtime
            .unwrap_or(false),
        default_domain: if default_domain.is_empty() {
            None
        } else {
            Some(default_domain.to_string())
        },
        default_ou_path: domain_join
            .ou_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
    }))
}

fn build_autopilot_profile(
    request: &ImageBuildRequest,
) -> Result<Option<bitosdt::config::AutopilotProfile>, String> {
    let autopilot: FrontendAutopilot = parse_frontend(request.autopilot.clone(), "autopilot")?;

    if !autopilot.enabled {
        return Ok(None);
    }

    if autopilot.tenant_id.trim().is_empty() {
        return Err("Autopilot is enabled but Tenant ID is missing".to_string());
    }

    let profile = bitosdt::config::AutopilotProfile {
        tenant_id: autopilot.tenant_id.clone(),
        // UI currently does not collect tenant domain; infer a conventional value.
        tenant_domain: format!("{}.onmicrosoft.com", autopilot.tenant_id),
        device_name_template: None,
        deployment_mode: map_deployment_mode(&autopilot.deployment_mode),
        oobe_config: bitosdt::config::AutopilotOobeConfig {
            hide_keyboard: autopilot.skip_device_oobe,
            hide_escape: autopilot.skip_device_oobe,
            hide_privacy: autopilot.skip_user_oobe,
            hide_eula: autopilot.skip_user_oobe,
            enable_white_glove: autopilot.allow_whiteglove,
            user_accept_terms: false,
        },
        group_tag: autopilot.group_tag,
        assigned_user: None,
    };

    Ok(Some(profile))
}

fn build_wds_sign_in_readiness(request: &ImageBuildRequest) -> Result<SignInReadiness, String> {
    let oobe: FrontendOobeConfig = parse_frontend(request.oobe_config.clone(), "OOBE")?;
    let user_accounts: Vec<FrontendUserAccount> = request
        .user_accounts
        .iter()
        .cloned()
        .map(|value| parse_frontend(value, "user account"))
        .collect::<Result<Vec<_>, _>>()?;
    let domain_join: FrontendDomainJoin =
        parse_frontend(request.domain_join.clone(), "domain join")?;
    let autopilot: FrontendAutopilot = parse_frontend(request.autopilot.clone(), "autopilot")?;

    let local_admin_count = user_accounts
        .iter()
        .filter(|user| {
            matches!(
                map_user_group(&user.group),
                bitosdt::config::UserGroup::Administrators
            )
        })
        .count() as u32;

    Ok(assess_sign_in_readiness(
        oobe.skip_user_oobe,
        local_admin_count,
        domain_join.enabled,
        autopilot.enabled,
    ))
}

fn build_task_sequence(
    request: &ImageBuildRequest,
) -> Result<Option<bitosdt::tasks::TaskSequence>, String> {
    let apps: FrontendApps = parse_frontend(request.apps.clone(), "applications")?;
    let windows_update: FrontendWindowsUpdate =
        parse_frontend(request.windows_update.clone(), "windows update")?;
    let shell_layout: ShellLayoutConfig =
        parse_frontend(request.shell_layout.clone(), "shell layout")?;
    let _preview_excluded = windows_update.exclude_preview;
    let copy_destination = apps.copy_destination.clone();
    let enable_custom_scripts = apps.enable_custom_scripts;
    let custom_scripts = apps.custom_scripts.clone();
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

    let has_app_work = !apps.copied_items.is_empty()
        || apps.winget_packages.iter().any(|p| p.enabled)
        || apps.chocolatey_packages.iter().any(|p| p.enabled)
        || apps.custom_installers.iter().any(|p| p.enabled);

    if has_app_work {
        let app_config = bitosdt::tasks::AppInstallConfig {
            winget_packages: apps
                .winget_packages
                .into_iter()
                .map(|p| bitosdt::tasks::WingetPackage {
                    package_id: p.package_id,
                    version: p.version,
                    custom_args: p.custom_args,
                    enabled: p.enabled,
                })
                .collect(),
            chocolatey_packages: apps
                .chocolatey_packages
                .into_iter()
                .map(|p| bitosdt::tasks::ChocolateyPackage {
                    package_name: p.package_name,
                    version: p.version,
                    source: p.source,
                    custom_args: p.custom_args,
                    enabled: p.enabled,
                })
                .collect(),
            custom_installers: apps
                .custom_installers
                .into_iter()
                .map(|p| bitosdt::tasks::CustomInstaller {
                    name: p.name,
                    path: p.path,
                    source_type: map_custom_installer_source_type(p.source_type.as_deref()),
                    source_file_name: p.source_file_name,
                    dependencies: p
                        .dependencies
                        .into_iter()
                        .map(|item| bitosdt::tasks::LocalPayloadItem {
                            source_path: item.source_path,
                            source_kind: map_local_payload_kind(&item.source_kind),
                            display_name: item.display_name,
                        })
                        .collect(),
                    dependency_destination: p.dependency_destination,
                    silent_args: p.silent_args,
                    installer_type: map_custom_installer_type(&p.installer_type),
                    success_codes: vec![0, 3010],
                    enabled: p.enabled,
                })
                .collect(),
            copied_items: apps
                .copied_items
                .into_iter()
                .map(|item| bitosdt::tasks::LocalPayloadItem {
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

        tasks.push(bitosdt::tasks::TaskDefinition {
            id: Uuid::new_v4(),
            name: "Install Applications".to_string(),
            task_type: bitosdt::tasks::TaskType::InstallApps(app_config),
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
        tasks.push(bitosdt::tasks::TaskDefinition {
            id: Uuid::new_v4(),
            name: "Apply Shell Layout".to_string(),
            task_type: bitosdt::tasks::TaskType::CustomScript(bitosdt::tasks::CustomScript {
                name: "Apply Shell Layout".to_string(),
                content: shell_layout_script,
                script_type: bitosdt::tasks::ScriptType::PowerShell,
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
        let update_config = bitosdt::tasks::WindowsUpdateConfig {
            enabled: true,
            include_drivers: windows_update.install_driver_updates,
            include_optional: !windows_update.exclude_optional,
            specific_kbs: vec![],
            timeout_minutes: 120,
            max_cycles: 3,
            reboot_behavior: map_windows_update_reboot(&windows_update.reboot_behavior),
            log_path: "C:\\BitOSDT\\Logs\\windows-update.log".to_string(),
        };

        tasks.push(bitosdt::tasks::TaskDefinition {
            id: Uuid::new_v4(),
            name: "Windows Update".to_string(),
            task_type: bitosdt::tasks::TaskType::WindowsUpdate(update_config),
            order,
            enabled: true,
            continue_on_error: true,
            requires_reboot: false,
        });
        order += 10;
    }

    if enable_custom_scripts {
        for (index, script) in custom_scripts.into_iter().enumerate() {
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

            tasks.push(bitosdt::tasks::TaskDefinition {
                id: Uuid::new_v4(),
                name: name.clone(),
                task_type: bitosdt::tasks::TaskType::CustomScript(bitosdt::tasks::CustomScript {
                    name,
                    content: script.content,
                    script_type: bitosdt::tasks::ScriptType::PowerShell,
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

    Ok(Some(bitosdt::tasks::TaskSequence {
        id: Uuid::new_v4(),
        name: format!(
            "{} {} setup",
            request.windows_version, request.windows_build
        ),
        tasks,
        settings: bitosdt::tasks::TaskSettings {
            scripts_dir: "C:\\BitOSDT\\Tasks".to_string(),
            logs_dir: "C:\\BitOSDT\\Logs".to_string(),
            continue_on_error: true,
            create_completion_marker: true,
        },
    }))
}

fn infer_os_type(name: &str) -> bitosdt::core::OsType {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("server 2025") {
        bitosdt::core::OsType::WindowsServer2025
    } else if lowered.contains("server 2022") {
        bitosdt::core::OsType::WindowsServer2022
    } else if lowered.contains("10") {
        bitosdt::core::OsType::Windows10
    } else if lowered.contains("11") {
        bitosdt::core::OsType::Windows11
    } else {
        bitosdt::core::OsType::Other
    }
}

fn infer_license_type(edition: &str) -> bitosdt::core::LicenseType {
    match edition.to_ascii_lowercase().as_str() {
        "home" => bitosdt::core::LicenseType::Home,
        "enterprise" => bitosdt::core::LicenseType::Enterprise,
        "education" => bitosdt::core::LicenseType::Education,
        "ltsc" => bitosdt::core::LicenseType::Ltsc,
        _ => bitosdt::core::LicenseType::Pro,
    }
}

fn parse_save_mode(request: &ImageBuildRequest) -> ImageSaveMode {
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

fn build_wizard_state_json(request: &ImageBuildRequest) -> serde_json::Value {
    let request_language = resolve_language_settings(request)
        .map(|(language, _)| language)
        .unwrap_or_else(|_| {
            request
                .language
                .clone()
                .unwrap_or_else(|| "en-US".to_string())
        });
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
            "sourceType": if request.source_path.as_ref().is_some_and(|value| !value.trim().is_empty()) { "local" } else { "cloud" },
            "channel": request.windows_channel.clone(),
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
            "promptUncCredentialsAtRuntime": request.prompt_unc_credentials_at_runtime.unwrap_or(false),
        }
    })
}

fn persist_built_image(
    request: &ImageBuildRequest,
    produced_iso_path: &std::path::Path,
) -> Result<(), String> {
    let db = cached_database()?;
    let now = Utc::now();
    let request_language = resolve_language_settings(request)
        .map(|(language, _)| language)
        .unwrap_or_else(|_| {
            request
                .language
                .clone()
                .unwrap_or_else(|| "en-US".to_string())
        });
    let wizard_state_json = Some(build_wizard_state_json(request));
    let mut image = bitosdt::core::Image {
        id: Uuid::new_v4(),
        name: format!(
            "{} {} {}",
            request.windows_version, request.windows_build, request.windows_edition
        ),
        description: Some("Generated from BitOSDT wizard".to_string()),
        os_info: bitosdt::core::OsInfo {
            os_type: infer_os_type(&request.windows_version),
            version: request.windows_build.clone(),
            architecture: bitosdt::core::Architecture::X64,
            language: request_language,
        },
        license: bitosdt::core::LicenseInfo {
            license_type: infer_license_type(&request.windows_edition),
            activation_type: None,
        },
        status: bitosdt::core::ImageStatus::Ready,
        created_at: now,
        updated_at: now,
        built_at: Some(now),
        workspace_path: None,
        wim_path: None,
        iso_path: Some(produced_iso_path.to_path_buf()),
        config: bitosdt::core::DeployConfig {
            os_version: request.windows_build.clone(),
            ..Default::default()
        },
        wizard_state_json,
        size_bytes: std::fs::metadata(produced_iso_path).ok().map(|m| m.len()),
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

fn has_required_winpe_packages(packages_dir: &Path) -> bool {
    if !packages_dir.is_dir() {
        return false;
    }

    std::fs::read_dir(packages_dir)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

fn push_resource_packages_candidates(resource_root: &Path, candidates: &mut Vec<PathBuf>) {
    candidates.push(resource_root.join("Packages"));
    candidates.push(resource_root.join("WinPE-Dependencies").join("Packages"));

    // Tauri may preserve relative path traversal (`../`) in bundled resources using
    // one or more `_up_` path segments. Probe a few levels so packaged MSI builds
    // can still resolve WinPE packages reliably.
    let mut up_root = resource_root.to_path_buf();
    for _ in 0..4 {
        up_root = up_root.join("_up_");
        candidates.push(up_root.join("Packages"));
        candidates.push(up_root.join("WinPE-Dependencies").join("Packages"));
    }
}

fn resolve_winpe_packages_dir_from_candidates(
    resource_dir: Option<&Path>,
    manifest_dir: &Path,
) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Some(resource_root) = resource_dir {
        push_resource_packages_candidates(resource_root, &mut candidates);
    }

    let dev_packages_dir = manifest_dir
        .join("..")
        .join("..")
        .join("WinPE-Dependencies")
        .join("Packages");
    candidates.push(dev_packages_dir);

    for candidate in &candidates {
        if has_required_winpe_packages(candidate) {
            return Ok(candidate.to_path_buf());
        }
    }

    let searched = candidates
        .iter()
        .map(|dir| format!("  - {}", dir.display()))
        .collect::<Vec<_>>()
        .join("\n");

    Err(format!(
        "Required WinPE packages directory was not found at one of:\n{}\nEnsure WinPE-Dependencies/Packages is bundled or present in the development tree.",
        searched
    ))
}

fn resolve_winpe_packages_dir(window: &tauri::Window) -> Result<PathBuf, String> {
    let resource_dir = window.app_handle().path_resolver().resource_dir();
    resolve_winpe_packages_dir_from_candidates(
        resource_dir.as_deref(),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
}

fn resolve_common_boot_drivers_dir(packages_dir: &Path) -> Option<PathBuf> {
    let candidate = packages_dir
        .parent()
        .map(|root| root.join("Drivers").join("Common"))?;

    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn load_runtime_driver_catalog_snapshot() -> Vec<bitosdt::core::DriverPack> {
    open_database()
        .ok()
        .and_then(|db| db.get_all_driverpacks().ok())
        .unwrap_or_default()
}

fn resolve_ui_dir_from_candidates(
    resource_dir: Option<&Path>,
    manifest_dir: &Path,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(resource_root) = resource_dir {
        candidates.push(resource_root.join("UI"));
        candidates.push(resource_root.join("dist"));
        candidates.push(resource_root.join("build"));

        // Handle bundled resource layouts that preserve relative traversal (`../`)
        // using `_up_` path segments.
        let mut up_root = resource_root.to_path_buf();
        for _ in 0..3 {
            up_root = up_root.join("_up_");
            candidates.push(up_root.join("UI"));
            candidates.push(up_root.join("dist"));
            candidates.push(up_root.join("build"));
        }
    }

    let dev_root = manifest_dir.join("..");
    candidates.push(dev_root.join("dist"));
    candidates.push(dev_root.join("build"));

    for candidate in &candidates {
        if candidate.join("index.html").is_file() {
            return Some(candidate.to_path_buf());
        }
    }

    None
}

fn resolve_ui_dir(window: &tauri::Window) -> Option<PathBuf> {
    let resource_dir = window.app_handle().path_resolver().resource_dir();
    resolve_ui_dir_from_candidates(
        resource_dir.as_deref(),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
}

fn build_workspace_path() -> PathBuf {
    bitosdt::core::config::Config::configured_workspace_path().unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        {
            PathBuf::from(r"C:\BitOSDT\Workspace")
        }

        #[cfg(not(target_os = "windows"))]
        {
            PathBuf::from("workspace")
        }
    })
}

fn build_workspace_cleanup_targets(workspace: &Path) -> Vec<PathBuf> {
    vec![
        workspace.join("install.wim"),
        workspace.join("install.normalized.wim"),
        workspace.join("mount"),
        workspace.join("winpe-mount"),
    ]
}

fn remove_workspace_target(path: &Path) -> Result<(), std::io::Error> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn powershell_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn find_workspace_dism_processes(
    workspace: &Path,
    targets: &[PathBuf],
) -> Result<Vec<BuildWorkspaceRecoveryProcess>, String> {
    let workspace_literal = powershell_single_quote(&workspace.to_string_lossy());
    let target_literals = targets
        .iter()
        .map(|path| format!("'{}'", powershell_single_quote(&path.to_string_lossy())))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        r#"$workspace = '{workspace}'.ToLowerInvariant()
$targets = @({targets}) | ForEach-Object {{ $_.ToLowerInvariant() }}
$matches = Get-CimInstance Win32_Process -Filter "Name = 'dism.exe'" -ErrorAction SilentlyContinue |
    Where-Object {{
        $cmd = $_.CommandLine
        if (-not $cmd) {{ return $false }}
        $normalized = $cmd.ToLowerInvariant()
        if ($normalized.Contains($workspace)) {{ return $true }}
        foreach ($target in $targets) {{
            if ($normalized.Contains($target)) {{ return $true }}
        }}
        return $false
    }} |
    ForEach-Object {{
        [PSCustomObject]@{{
            pid = [int]$_.ProcessId
            executable = $_.Name
            command_line = $_.CommandLine
        }}
    }}
if ($null -eq $matches) {{
    '[]'
}} elseif ($matches -is [System.Array]) {{
    $matches | ConvertTo-Json -Compress
}} else {{
    @($matches) | ConvertTo-Json -Compress
}}"#,
        workspace = workspace_literal,
        targets = target_literals
    );

    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("Failed to inspect DISM processes: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to inspect DISM processes: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Ok(Vec::new());
    }

    let json_value: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|e| format!("Invalid process query JSON: {}", e))?;
    match json_value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<BuildWorkspaceRecoveryProcess>, _>>()
            .map_err(|e| format!("Invalid process entry: {}", e)),
        serde_json::Value::Object(_) => serde_json::from_value(json_value)
            .map(|item| vec![item])
            .map_err(|e| format!("Invalid process entry: {}", e)),
        _ => Ok(Vec::new()),
    }
}

#[cfg(not(target_os = "windows"))]
fn find_workspace_dism_processes(
    _workspace: &Path,
    _targets: &[PathBuf],
) -> Result<Vec<BuildWorkspaceRecoveryProcess>, String> {
    Ok(Vec::new())
}

fn inspect_build_workspace_recovery(
    workspace: &Path,
) -> Result<Option<BuildWorkspaceRecoveryResponse>, String> {
    let targets = build_workspace_cleanup_targets(workspace);
    for target in &targets {
        match remove_workspace_target(target) {
            Ok(()) => {}
            Err(error) => {
                let matches = find_workspace_dism_processes(workspace, &targets)?;
                return Ok(Some(build_workspace_recovery_response(
                    target,
                    error.to_string(),
                    matches,
                )));
            }
        }
    }

    Ok(None)
}

fn build_workspace_recovery_response(
    locked_path: &Path,
    error_message: String,
    processes: Vec<BuildWorkspaceRecoveryProcess>,
) -> BuildWorkspaceRecoveryResponse {
    let status = if processes.is_empty() {
        BuildWorkspaceRecoveryStatus::LockedWithoutMatches
    } else {
        BuildWorkspaceRecoveryStatus::LockedWithMatches
    };
    let message = if processes.is_empty() {
        format!(
            "BitOSDT could not clean {} before starting the build: {}",
            locked_path.display(),
            error_message
        )
    } else {
        format!(
            "A previous BitOSDT build is still holding {} open: {}",
            locked_path.display(),
            error_message
        )
    };

    BuildWorkspaceRecoveryResponse {
        status,
        message,
        locked_path: Some(locked_path.display().to_string()),
        processes,
    }
}

#[cfg(target_os = "windows")]
fn cleanup_stale_wim_mounts() -> Result<(), String> {
    let args = vec!["/Cleanup-Wim".to_string()];
    let output = run_dism(&args, None).map_err(|e| format!("Failed to run DISM cleanup: {}", e))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(format!(
        "DISM /Cleanup-Wim failed (stdout={}, stderr={})",
        if stdout.is_empty() {
            "<empty>"
        } else {
            &stdout
        },
        if stderr.is_empty() {
            "<empty>"
        } else {
            &stderr
        }
    ))
}

#[cfg(not(target_os = "windows"))]
fn cleanup_stale_wim_mounts() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
async fn check_build_workspace_recovery() -> Result<BuildWorkspaceRecoveryResponse, String> {
    let workspace = build_workspace_path();
    Ok(
        inspect_build_workspace_recovery(&workspace)?.unwrap_or(BuildWorkspaceRecoveryResponse {
            status: BuildWorkspaceRecoveryStatus::Ok,
            message: "BitOSDT workspace is ready for a new build.".to_string(),
            locked_path: None,
            processes: Vec::new(),
        }),
    )
}

#[tauri::command]
async fn recover_build_workspace() -> Result<BuildWorkspaceRecoveryResponse, String> {
    let workspace = build_workspace_path();
    let initial =
        inspect_build_workspace_recovery(&workspace)?.unwrap_or(BuildWorkspaceRecoveryResponse {
            status: BuildWorkspaceRecoveryStatus::Ok,
            message: "BitOSDT workspace is already ready for a new build.".to_string(),
            locked_path: None,
            processes: Vec::new(),
        });

    if initial.processes.is_empty() {
        return Ok(initial);
    }

    for process in &initial.processes {
        let _ = stop_process_tree(process.pid);
    }

    let cleanup_wim_error = cleanup_stale_wim_mounts().err();
    match inspect_build_workspace_recovery(&workspace)? {
        Some(mut follow_up) => {
            if let Some(cleanup_error) = cleanup_wim_error {
                follow_up.message = format!("{} {}", follow_up.message, cleanup_error);
            }
            Ok(follow_up)
        }
        None => Ok(BuildWorkspaceRecoveryResponse {
            status: BuildWorkspaceRecoveryStatus::Ok,
            message: "BitOSDT stopped the matched DISM processes and cleaned the stale workspace artifacts.".to_string(),
            locked_path: None,
            processes: Vec::new(),
        }),
    }
}

fn maybe_run_runtime_driver_cli() -> Result<bool, String> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return Ok(false);
    };

    if command != "runtime-drivers" {
        return Ok(false);
    }

    let mut config_path: Option<PathBuf> = None;
    let mut windows_path: Option<PathBuf> = None;
    let mut prepare_only = false;
    let mut server_url: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--config requires a file path".to_string())?;
                config_path = Some(PathBuf::from(value));
            }
            "--windows-path" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--windows-path requires a path".to_string())?;
                windows_path = Some(PathBuf::from(value));
            }
            "--prepare-only" => {
                prepare_only = true;
            }
            "--server-url" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--server-url requires a value".to_string())?;
                server_url = Some(value);
            }
            other => {
                return Err(format!("Unknown runtime-drivers argument: {}", other));
            }
        }
    }

    let config_path =
        config_path.ok_or_else(|| "runtime-drivers requires --config <path>".to_string())?;
    let payload = std::fs::read_to_string(&config_path).map_err(|e| {
        format!(
            "Failed to read runtime driver config {}: {}",
            config_path.display(),
            e
        )
    })?;
    let runtime_config: bitosdt::core::RuntimeDriverConfig = serde_json::from_str(&payload)
        .map_err(|e| {
            format!(
                "Failed to parse runtime driver config {}: {}",
                config_path.display(),
                e
            )
        })?;

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to initialize tokio runtime: {}", e))?;

    let result = runtime.block_on(async {
        let mut runtime_config = runtime_config.clone();
        if server_url.is_some() {
            runtime_config
                .runtime_driver_context
                .cache_download_base_url = server_url
                .as_deref()
                .map(|base| format!("{}/BitOSDT/DriverCache", base.trim_end_matches('/')));
        }
        if prepare_only || windows_path.is_none() {
            bitosdt::deploy::prepare_runtime_drivers(&runtime_config, None)
                .await
                .map_err(|e| e.to_string())
        } else {
            bitosdt::deploy::prepare_runtime_drivers(
                &runtime_config,
                Some(windows_path.as_deref().unwrap_or(Path::new(r"W:\"))),
            )
            .await
            .map_err(|e| e.to_string())
        }
    });

    match result {
        Ok(manifest) => {
            if let Some(driverpack) = manifest.matched_driverpack.as_ref() {
                println!("Runtime driver match: {}", driverpack.name);
            } else {
                println!("Runtime driver match: none");
            }
            println!("Prepared: {}", manifest.prepared);
            println!("Installed count: {}", manifest.installed_count);
            for warning in manifest.warnings {
                eprintln!("Warning: {}", warning);
            }
            Ok(true)
        }
        Err(err) => Err(format!("Runtime driver command failed: {}", err)),
    }
}

fn should_force_winpe_fullscreen() -> bool {
    if let Ok(value) = std::env::var("BITOSDT_WINPE_FULLSCREEN") {
        let normalized = value.trim().to_ascii_lowercase();
        if !normalized.is_empty() && !matches!(normalized.as_str(), "0" | "false" | "no" | "off") {
            return true;
        }
    }

    std::env::var("SystemDrive")
        .map(|drive| drive.trim().eq_ignore_ascii_case("X:"))
        .unwrap_or(false)
}

fn enforce_winpe_window_mode(window: &tauri::Window) {
    if let Ok(Some(monitor)) = window.current_monitor() {
        let _ = window.set_position(Position::Physical(*monitor.position()));
        let _ = window.set_size(Size::Physical(*monitor.size()));
    }

    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let _ = window.maximize();
    let _ = window.set_fullscreen(true);
}

fn apply_winpe_window_mode(window: &tauri::Window) {
    if !should_force_winpe_fullscreen() {
        return;
    }

    let _ = window.set_decorations(false);
    enforce_winpe_window_mode(window);

    let window = window.clone();
    tauri::async_runtime::spawn(async move {
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            enforce_winpe_window_mode(&window);
        }
    });
}

#[tauri::command]
async fn build_image(request: ImageBuildRequest, window: tauri::Window) -> Result<String, String> {
    let app_handle = window.app_handle();
    let build_state = app_handle.state::<BuildProcessState>();
    let download_cancel = app_handle.state::<DownloadCancelFlag>();
    download_cancel.reset();
    build_state.begin_build()?;

    let result = async {
        if request.output_type == "WDSPXE" {
            return build_wds_pxe(&request, &window).await;
        }

        let current_executable = std::env::current_exe().ok();
        let winpe_packages_dir = resolve_winpe_packages_dir(&window).ok();
        let common_boot_driver_dir = winpe_packages_dir
            .as_deref()
            .and_then(resolve_common_boot_drivers_dir);
        let context = SharedImageBuildContext {
            ui_dir: resolve_ui_dir(&window),
            winpe_packages_dir: winpe_packages_dir.clone(),
            common_boot_driver_dir,
            runtime_driver_catalog: load_runtime_driver_catalog_snapshot(),
            native_runtime_executable: current_executable.clone(),
            gui_executable: current_executable,
            simple_publish_path: default_simple_publish_path().ok(),
            simple_runtime_url: Some(default_simple_runtime_url()),
            winpe_assets_dir: None,
            persist_built_image: true,
        };

        let shared_request: SharedImageBuildRequest =
            serde_json::from_value(serde_json::to_value(&request).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Failed to convert build request: {}", e))?;

        build_image_with_context(&shared_request, &context, |progress| {
            let _ = window.emit(
                "build-progress",
                BuildProgress {
                    step: progress.step,
                    progress: progress.progress,
                    message: progress.message,
                },
            );
        })
        .await
    }
    .await;

    let normalized_result = match result {
        Ok(value) => {
            if build_state.is_cancel_requested() {
                Err("Build cancelled by user.".to_string())
            } else {
                Ok(value)
            }
        }
        Err(error) => {
            if build_state.is_cancel_requested() || error.contains("Operation cancelled by user") {
                Err("Build cancelled by user.".to_string())
            } else {
                Err(error)
            }
        }
    };

    build_state.finish_build();
    download_cancel.reset();
    normalized_result
}

async fn build_lightweight_iso(
    request: &ImageBuildRequest,
    window: &tauri::Window,
) -> Result<String, String> {
    use bitosdt::build::{stage_lightweight_media_tree, LightweightBuilder, LightweightConfig};

    let emit_progress = |step: &str, progress: u32, message: &str| {
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: step.to_string(),
                progress,
                message: message.to_string(),
            },
        );
    };

    emit_progress("init", 0, "Starting Lightweight ISO build...");
    let delivery_mode = resolve_delivery_mode(request);
    let driver_paths = validate_driver_paths(&request.driver_paths)?;
    let winpe_packages_dir = resolve_winpe_packages_dir(window)?;
    let common_boot_driver_dir = resolve_common_boot_drivers_dir(&winpe_packages_dir);
    let ui_dir = resolve_ui_dir(window);
    if ui_dir.is_none() {
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: "warning".to_string(),
                progress: 0,
                message: "Web UI assets were not found on disk. Continuing without copying X:\\BitOSDT\\UI."
                    .to_string(),
            },
        );
    }

    let workspace = bitosdt::core::config::Config::configured_workspace_path()
        .unwrap_or_else(|_| PathBuf::from("workspace"));
    let download_dir = bitosdt::core::config::Config::configured_download_path()
        .unwrap_or_else(|_| PathBuf::from("downloads"));
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Failed to create workspace: {}", e))?;
    std::fs::create_dir_all(&download_dir)
        .map_err(|e| format!("Failed to create download directory: {}", e))?;
    let publish_path = resolve_lightweight_publish_path(request, delivery_mode)?;
    let runtime_server_url = resolve_lightweight_server_url(request, delivery_mode)?;

    let output_path = if request.output_type == "Both" {
        let path = PathBuf::from(&request.output_path);
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().unwrap_or_default().to_string_lossy();
        parent.join(format!("{}-Lightweight.{}", stem, ext))
    } else {
        PathBuf::from(&request.output_path)
    };

    let current_executable = std::env::current_exe().ok();
    let include_gui_requested = request
        .include_gui
        .unwrap_or(delivery_mode == DeliveryMode::Simple);
    let gui_executable = if include_gui_requested {
        current_executable.clone()
    } else {
        None
    };

    let config = LightweightConfig {
        output_path: output_path.clone(),
        volume_label: request.volume_label.clone(),
        server_url: runtime_server_url.clone(),
        include_gui: include_gui_requested && gui_executable.is_some(),
        gui_executable,
        native_executable: current_executable.clone(),
        winpe_assets_dir: None,
        common_boot_driver_dir,
        driver_cache_dir: Some(download_dir.join("drivers")),
        runtime_driver_policy: request.runtime_driver_policy.clone().unwrap_or_default(),
        runtime_driver_catalog: load_runtime_driver_catalog_snapshot(),
        os_version: request.windows_build.clone(),
        driver_paths: driver_paths.clone(),
        winpe_packages_dir: Some(winpe_packages_dir),
        ui_dir,
        ..Default::default()
    };

    let mut builder = LightweightBuilder::new(workspace.clone())
        .map_err(|e| format!("Failed to create builder: {}", e))?;

    let window_clone = window.clone();
    builder
        .build(&config, move |p, msg| {
            let _ = window_clone.emit(
                "build-progress",
                BuildProgress {
                    step: "build".to_string(),
                    progress: p as u32,
                    message: msg,
                },
            );
        })
        .map_err(|e| format!("Build failed: {}", e))?;

    emit_progress("publish", 90, "Staging PXE/lightweight publish files...");
    let media_dir = workspace.join("winpe").join("media");
    let runtime_executable = if delivery_mode == DeliveryMode::Simple {
        current_executable.as_deref()
    } else {
        None
    };
    let manifest_json = if delivery_mode == DeliveryMode::Simple {
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
    emit_progress(
        "publish",
        95,
        &format!(
            "PXE/lightweight files staged at {} ({} files).",
            publish_result.destination.display(),
            publish_result.copied_files
        ),
    );

    if delivery_mode == DeliveryMode::Simple {
        emit_progress(
            "publish",
            99,
            &format!(
                "Simple delivery staging is ready at {}. Start the embedded lightweight host from BitOSDT when you want to serve it at {}.",
                publish_path.display(),
                runtime_server_url
            ),
        );
    }

    persist_built_image(request, &output_path)?;

    Ok(output_path.to_string_lossy().to_string())
}

async fn build_wds_pxe(
    request: &ImageBuildRequest,
    window: &tauri::Window,
) -> Result<String, String> {
    use bitosdt::build::{
        build_full_iso as build_full_iso_core, DiskSelectionPolicy, FullIsoBuildConfig,
    };
    use bitosdt::core::resolve_adk_paths;

    let emit_progress = |step: &str, progress: u32, message: &str| {
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: step.to_string(),
                progress,
                message: message.to_string(),
            },
        );
    };

    let wds_started_at = Instant::now();
    emit_progress("init", 0, "Starting WDS/PXE export build...");
    let winpe_packages_dir = resolve_winpe_packages_dir(window)?;
    let common_boot_driver_dir = resolve_common_boot_drivers_dir(&winpe_packages_dir);
    let ui_dir = resolve_ui_dir(window);
    if ui_dir.is_none() {
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: "warning".to_string(),
                progress: 0,
                message: "Web UI assets were not found on disk. Continuing without copying X:\\BitOSDT\\UI."
                    .to_string(),
            },
        );
    }

    let download_dir = bitosdt::core::config::Config::configured_download_path()
        .unwrap_or_else(|_| PathBuf::from("C:\\BitOSDT\\Downloads"));
    let workspace = bitosdt::core::config::Config::configured_workspace_path()
        .unwrap_or_else(|_| PathBuf::from("C:\\BitOSDT\\Workspace"));

    std::fs::create_dir_all(&download_dir)
        .map_err(|e| format!("Failed to create download directory: {}", e))?;
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Failed to create workspace: {}", e))?;
    let resolved_adk = resolve_adk_paths(None, "amd64");
    let shared_request: SharedImageBuildRequest =
        serde_json::from_value(serde_json::to_value(request).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Failed to convert WDS build request: {}", e))?;

    emit_progress("source", 5, "Locating Windows source...");

    let resolved_source =
        prepare_full_build_source(&shared_request, &workspace, &download_dir, |progress| {
            let _ = window.emit(
                "build-progress",
                BuildProgress {
                    step: progress.step,
                    progress: progress.progress,
                    message: progress.message,
                },
            );
        })
        .await?;
    let source_path = resolved_source.source_path.clone();

    emit_progress(
        "source",
        20,
        &format!(
            "Using source: {:?}",
            source_path.file_name().unwrap_or_default()
        ),
    );

    emit_progress("config", 20, "Generating WDS/PXE configuration files...");

    let unattend_config = build_unattend_config(request)?;
    let autopilot_profile = build_autopilot_profile(request)?;
    let task_sequence = build_task_sequence(request)?;
    let runtime_domain_join = build_runtime_domain_join_config(request)?;
    let sign_in_readiness = build_wds_sign_in_readiness(request)?;
    if sign_in_readiness.level == SignInReadinessLevel::Warning {
        emit_progress(
            "warning",
            20,
            &format!("WDS/PXE sign-in warning: {}", sign_in_readiness.summary),
        );
    }
    let (unc_image_path, http_image_url, unc_auth_username, unc_auth_password) =
        validate_wds_pxe_runtime_source(
            request.full_iso_unc_path.as_ref(),
            request.full_iso_unc_username.as_ref(),
            request.full_iso_unc_password.as_ref(),
            request.full_iso_http_url.as_ref(),
            request.prompt_unc_credentials_at_runtime.unwrap_or(false),
        )?;
    let native_executable = std::env::current_exe().ok();
    let temp_iso_path = workspace.join("wds-pxe-export-temp.iso");
    let full_iso_config = FullIsoBuildConfig {
        source_path,
        output_path: temp_iso_path.clone(),
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
        workspace: Some(workspace.clone()),
        download_dir: Some(download_dir.clone()),
        adk_paths: resolved_adk,
        winpe_assets_dir: None,
        winpe_packages_dir: Some(winpe_packages_dir),
        ui_dir,
        native_executable,
        common_boot_driver_dir,
        runtime_driver_catalog: load_runtime_driver_catalog_snapshot(),
        runtime_driver_cache_source: Some(download_dir.join("drivers")),
        driver_paths: validate_driver_paths_with_network(&request.driver_paths, true)?,
        apply_drivers_to_offline_windows: request.apply_to_offline_windows.unwrap_or(false),
        runtime_driver_policy: request.runtime_driver_policy.clone().unwrap_or_default(),
        unc_image_path: unc_image_path.clone(),
        unc_auth_username: unc_auth_username.clone(),
        unc_auth_password: unc_auth_password.clone(),
        http_image_url: http_image_url.clone(),
        prompt_unc_credentials_at_runtime: request.prompt_unc_credentials_at_runtime,
    };

    let full_iso_started_at = Instant::now();
    let build_result = build_full_iso_core(&full_iso_config, |progress| {
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: progress.step,
                progress: progress.progress,
                message: progress.message,
            },
        );
    })
    .map_err(|e| format!("WDS/PXE build failed: {}", e))?;
    emit_progress(
        "timing",
        94,
        &format!(
            "Full ISO staging pipeline finished in {:.1} seconds.",
            full_iso_started_at.elapsed().as_secs_f64()
        ),
    );

    let export_started_at = Instant::now();
    emit_progress("export", 95, "Exporting WDS/PXE artifacts...");
    let exported_boot_wim = export_wds_pxe_bundle(
        request,
        &build_result,
        unc_image_path,
        unc_auth_username,
        http_image_url,
        sign_in_readiness,
    )?;
    emit_progress(
        "export",
        98,
        &format!(
            "WDS/PXE artifact export completed in {:.1} seconds.",
            export_started_at.elapsed().as_secs_f64()
        ),
    );

    let _ = std::fs::remove_file(&temp_iso_path);

    emit_progress(
        "complete",
        100,
        &format!(
            "WDS/PXE export complete in {:.1} seconds. boot.wim and payload are available at {}",
            wds_started_at.elapsed().as_secs_f64(),
            WDS_EXPORT_ROOT
        ),
    );

    persist_built_image(request, &exported_boot_wim)?;
    Ok(WDS_EXPORT_ROOT.to_string())
}

async fn build_full_iso(
    request: &ImageBuildRequest,
    window: &tauri::Window,
) -> Result<String, String> {
    use bitosdt::build::{
        build_full_iso as build_full_iso_core, DiskSelectionPolicy, FullIsoBuildConfig,
    };
    use bitosdt::core::resolve_adk_paths;

    let emit_progress = |step: &str, progress: u32, message: &str| {
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: step.to_string(),
                progress,
                message: message.to_string(),
            },
        );
    };

    emit_progress("init", 0, "Starting full ISO build...");
    let winpe_packages_dir = resolve_winpe_packages_dir(window)?;
    let common_boot_driver_dir = resolve_common_boot_drivers_dir(&winpe_packages_dir);
    let ui_dir = resolve_ui_dir(window);
    if ui_dir.is_none() {
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: "warning".to_string(),
                progress: 0,
                message: "Web UI assets were not found on disk. Continuing without copying X:\\BitOSDT\\UI."
                    .to_string(),
            },
        );
    }

    // Use configured download path for caching, separate workspace for build artifacts
    let download_dir = bitosdt::core::config::Config::configured_download_path()
        .unwrap_or_else(|_| PathBuf::from("C:\\BitOSDT\\Downloads"));
    let workspace = bitosdt::core::config::Config::configured_workspace_path()
        .unwrap_or_else(|_| PathBuf::from("C:\\BitOSDT\\Workspace"));

    std::fs::create_dir_all(&download_dir)
        .map_err(|e| format!("Failed to create download directory: {}", e))?;
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Failed to create workspace: {}", e))?;
    let resolved_adk = resolve_adk_paths(None, "amd64");

    emit_progress("source", 5, "Locating Windows source...");

    // Determine source path. The shared builder handles ISO extraction and ESD->WIM conversion.
    let source_path = if let Some(ref source) = request.source_path {
        let source_path = PathBuf::from(source);
        if !source_path.exists() {
            return Err(format!("Source file not found: {}", source));
        }
        source_path
    } else if let Some(ref download_url) = request.download_url {
        // Download from Microsoft CDN to download cache directory
        emit_progress(
            "download",
            5,
            "Downloading Windows image from Microsoft CDN... [0.0% - 0 B/s - ETA: Calculating...]",
        );

        use bitosdt::download::{EsdDownloader, EsdInfo};
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        // Initialize downloader with download cache directory
        let downloader = EsdDownloader::new_with_adk(download_dir.clone(), resolved_adk.clone())
            .map_err(|e| format!("Failed to create downloader: {}", e))?;

        emit_progress("download", 5, "Fetching file size...");
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
            language: "en-us".to_string(),
            architecture: "amd64".to_string(),
            version: request.windows_version.clone(),
            build: request.windows_build.clone(),
        };

        let window_clone = window.clone();
        let last_emit = Arc::new(Mutex::new(Instant::now()));
        let downloaded_path = downloader
            .download_esd(&esd_info, move |progress| {
                // Rate limit: only emit every 100ms for smoother updates
                let mut last = last_emit.lock().unwrap();
                let now = Instant::now();
                let should_emit = now.duration_since(*last).as_millis() >= 100
                    || progress.percent >= 99.9
                    || progress.percent < 0.1;

                if should_emit {
                    *last = now;
                    let percent = progress.percent as u32;
                    let speed = progress.format_speed();
                    let eta = progress.format_eta();
                    let _ = window_clone.emit(
                        "build-progress",
                        BuildProgress {
                            step: "download".to_string(),
                            progress: 5 + (percent * 10 / 100), // Scale to 5-15%
                            message: format!(
                                "Downloading: {:.1}% - {} - ETA: {}",
                                progress.percent, speed, eta
                            ),
                        },
                    );
                }
            })
            .await
            .map_err(|e| format!("Download failed: {}", e))?;

        emit_progress("source", 15, "Download complete.");
        downloaded_path
    } else {
        // Check workspace for existing source
        let workspace_iso = workspace.join("install.iso");
        let workspace_esd = workspace.join("install.esd");
        let workspace_wim = workspace.join("install.wim");

        if workspace_iso.exists() {
            workspace_iso
        } else if workspace_esd.exists() {
            workspace_esd
        } else if workspace_wim.exists() {
            workspace_wim
        } else {
            return Err(
                "Windows source not found. Select an ISO/ESD/WIM file or choose cloud download."
                    .to_string(),
            );
        }
    };

    emit_progress(
        "source",
        15,
        &format!(
            "Using source: {:?}",
            source_path.file_name().unwrap_or_default()
        ),
    );

    emit_progress("config", 20, "Generating configuration files...");

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
    let native_executable = std::env::current_exe().ok();
    let full_iso_config = FullIsoBuildConfig {
        source_path,
        output_path: output_path.clone(),
        volume_label: request.volume_label.clone(),
        windows_version: request.windows_version.clone(),
        windows_build: request.windows_build.clone(),
        windows_edition: request.windows_edition.clone(),
        language: request
            .language
            .clone()
            .unwrap_or_else(|| "en-US".to_string()),
        architecture: "amd64".to_string(),
        wim_index: 1,
        target_disk: request.target_disk,
        disk_selection_policy: DiskSelectionPolicy::ConfigFirstSafeFallback,
        unattend: unattend_config,
        autopilot: autopilot_profile,
        task_sequence,
        runtime_domain_join,
        workspace: Some(workspace),
        download_dir: Some(download_dir.clone()),
        adk_paths: resolved_adk,
        winpe_assets_dir: None,
        winpe_packages_dir: Some(winpe_packages_dir),
        ui_dir,
        native_executable,
        common_boot_driver_dir,
        runtime_driver_catalog: load_runtime_driver_catalog_snapshot(),
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
        let _ = window.emit(
            "build-progress",
            BuildProgress {
                step: progress.step,
                progress: progress.progress,
                message: progress.message,
            },
        );
    })
    .map_err(|e| format!("Full ISO build failed: {}", e))?;

    persist_built_image(request, &output_path)?;
    Ok(output_path.to_string_lossy().to_string())
}

#[tauri::command]
fn generate_unattend_xml(config_json: String, output_path: String) -> Result<String, String> {
    use bitosdt::config::{UnattendConfig, UnattendGenerator};

    let config: UnattendConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let xml =
        UnattendGenerator::generate(&config).map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &xml).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(xml)
}

#[tauri::command]
fn generate_autopilot_json(config_json: String, output_path: String) -> Result<String, String> {
    use bitosdt::config::{AutopilotGenerator, AutopilotProfile};

    let profile: AutopilotProfile =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let json = AutopilotGenerator::generate_configuration(&profile)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &json).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(json)
}

#[tauri::command]
fn generate_app_install_script(config_json: String, output_path: String) -> Result<String, String> {
    use bitosdt::tasks::{AppInstallConfig, AppInstaller};

    let config: AppInstallConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = AppInstaller::generate_install_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[tauri::command]
fn generate_winget_script(packages_json: String, output_path: String) -> Result<String, String> {
    use bitosdt::tasks::{AppInstaller, WingetPackage};

    let packages: Vec<WingetPackage> =
        serde_json::from_str(&packages_json).map_err(|e| format!("Invalid packages: {}", e))?;

    let script = AppInstaller::generate_winget_only_script(&packages);

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[tauri::command]
fn generate_windows_update_script(
    config_json: String,
    output_path: String,
) -> Result<String, String> {
    use bitosdt::tasks::{WindowsUpdateConfig, WindowsUpdateGenerator};

    let config: WindowsUpdateConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = WindowsUpdateGenerator::generate_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[tauri::command]
fn generate_domain_join_script(config_json: String, output_path: String) -> Result<String, String> {
    use bitosdt::tasks::{DomainJoinConfig, DomainJoinGenerator};

    let config: DomainJoinConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = DomainJoinGenerator::generate_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[tauri::command]
fn generate_user_creation_script(
    config_json: String,
    output_path: String,
) -> Result<String, String> {
    use bitosdt::tasks::{UserCreatorGenerator, UsersConfig};

    let config: UsersConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = UserCreatorGenerator::generate_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[tauri::command]
fn generate_osdcloud_startnet(
    output_path: String,
    use_start_osdcloud: bool,
) -> Result<String, String> {
    use bitosdt::build::StartnetGenerator;

    let content = StartnetGenerator::generate_osdcloud(use_start_osdcloud);

    std::fs::write(&output_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(content)
}

#[tauri::command]
fn generate_bitosdt_startnet(output_path: String, exe_path: String) -> Result<String, String> {
    use bitosdt::build::StartnetGenerator;

    let content = StartnetGenerator::generate_bitosdt_gui(&exe_path);

    std::fs::write(&output_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(content)
}

#[tauri::command]
fn generate_network_startnet(output_path: String, server_url: String) -> Result<String, String> {
    use bitosdt::build::StartnetGenerator;

    let content = StartnetGenerator::generate_network_boot(&server_url);

    std::fs::write(&output_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(content)
}

#[tauri::command]
fn create_iso(request: IsoRequest) -> Result<String, String> {
    use bitosdt::build::IsoCreator;
    use std::path::PathBuf;

    IsoCreator::create_iso(
        &PathBuf::from(&request.source_dir),
        &PathBuf::from(&request.output_path),
        &request.volume_label,
    )
    .map_err(|e| format!("ISO creation failed: {}", e))?;

    Ok(request.output_path)
}

// ============================================================================
// OS Catalog Commands
// ============================================================================

use bitosdt::catalog::{OsCatalogSyncService, OsCatalogSyncStatus, OsVersionEntry};

fn load_config() -> Result<bitosdt::core::Config, String> {
    bitosdt::core::Config::load().map_err(|e| format!("Failed to load config: {}", e))
}

fn open_database() -> Result<bitosdt::core::Database, String> {
    let config = load_config()?;
    bitosdt::core::Database::new(&config.database_path)
        .map_err(|e| format!("Failed to open database: {}", e))
}

fn cached_database() -> Result<std::sync::MutexGuard<'static, bitosdt::core::Database>, String> {
    static DB: OnceLock<Result<Mutex<bitosdt::core::Database>, String>> = OnceLock::new();

    let db = DB.get_or_init(|| {
        let config = load_config()?;
        let database = bitosdt::core::Database::new(&config.database_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        Ok(Mutex::new(database))
    });

    match db {
        Ok(database) => database.lock().map_err(|error| error.to_string()),
        Err(error) => Err(error.clone()),
    }
}

fn policy_bootstrap_cache() -> &'static Mutex<Option<bitosdt::policy::PolicyEditorBootstrap>> {
    static CACHE: OnceLock<Mutex<Option<bitosdt::policy::PolicyEditorBootstrap>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn load_saved_policy_presets_from_database() -> Result<Vec<bitosdt::policy::PolicyPreset>, String> {
    let db = cached_database()?;
    let raw = db
        .get_setting(GROUP_POLICY_PRESETS_SETTING_KEY)
        .map_err(|e| {
            format!(
                "Failed to get setting {}: {}",
                GROUP_POLICY_PRESETS_SETTING_KEY, e
            )
        })?;
    bitosdt::policy::load_saved_policy_presets_from_json(raw.as_deref())
}

fn store_saved_policy_presets_in_database(
    presets: &[bitosdt::policy::PolicyPreset],
) -> Result<(), String> {
    let raw = bitosdt::policy::serialize_saved_policy_presets(presets)?;
    let db = cached_database()?;
    db.set_setting(GROUP_POLICY_PRESETS_SETTING_KEY, &raw, "json")
        .map_err(|e| {
            format!(
                "Failed to set setting {}: {}",
                GROUP_POLICY_PRESETS_SETTING_KEY, e
            )
        })
}

fn load_policy_editor_bootstrap_cached(
    force_refresh: bool,
) -> Result<bitosdt::policy::PolicyEditorBootstrap, String> {
    let cache = policy_bootstrap_cache();
    if !force_refresh {
        if let Some(cached) = cache
            .lock()
            .expect("policy bootstrap cache poisoned")
            .clone()
        {
            return Ok(cached);
        }
    }

    let mut bootstrap = bitosdt::policy::load_policy_editor_bootstrap()?;
    bootstrap.saved_presets = load_saved_policy_presets_from_database()?;
    *cache.lock().expect("policy bootstrap cache poisoned") = Some(bootstrap.clone());
    Ok(bootstrap)
}

#[tauri::command]
fn get_policy_editor_bootstrap(
    force_refresh: Option<bool>,
) -> Result<bitosdt::policy::PolicyEditorBootstrap, String> {
    load_policy_editor_bootstrap_cached(force_refresh.unwrap_or(false))
}

#[tauri::command]
fn save_policy_preset(
    name: String,
    selection: bitosdt::policy::GroupPolicySelection,
) -> Result<bitosdt::policy::PolicyPreset, String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("Preset name is required.".to_string());
    }

    let mut presets = load_saved_policy_presets_from_database()?;
    let preset = bitosdt::policy::PolicyPreset {
        id: format!("custom-{}", Uuid::new_v4()),
        name: trimmed_name.to_string(),
        built_in: false,
        selected_policy_ids: selection.selected_policy_ids,
        custom_registry_entries: selection.custom_registry_entries,
    };
    presets.retain(|existing| existing.id != preset.id);
    presets.push(preset.clone());
    presets.sort_by(|left, right| left.name.cmp(&right.name));
    store_saved_policy_presets_in_database(&presets)?;

    let mut bootstrap = load_policy_editor_bootstrap_cached(false)?;
    bootstrap.saved_presets = presets;
    *policy_bootstrap_cache()
        .lock()
        .expect("policy bootstrap cache poisoned") = Some(bootstrap);

    Ok(preset)
}

#[tauri::command]
fn delete_policy_preset(preset_id: String) -> Result<(), String> {
    let mut presets = load_saved_policy_presets_from_database()?;
    presets.retain(|preset| preset.id != preset_id);
    store_saved_policy_presets_in_database(&presets)?;

    let mut bootstrap = load_policy_editor_bootstrap_cached(false)?;
    bootstrap.saved_presets = presets;
    *policy_bootstrap_cache()
        .lock()
        .expect("policy bootstrap cache poisoned") = Some(bootstrap);

    Ok(())
}

#[tauri::command]
async fn get_os_versions(
    language: Option<String>,
    release: Option<String>,
    os: Option<String>,
    arch: Option<String>,
) -> Result<Vec<OsVersionEntry>, String> {
    let db = cached_database()?;

    let versions = db
        .get_os_versions_filtered(
            os.as_deref(),
            release.as_deref(),
            arch.as_deref(),
            language.as_deref(),
        )
        .map_err(|e| format!("Failed to get OS versions: {}", e))?;

    Ok(versions)
}

#[tauri::command]
async fn sync_os_catalog() -> Result<OsCatalogSyncStatus, String> {
    use bitosdt::catalog::os_sync::fetch_os_catalog;

    // Step 1: Fetch data asynchronously (no DB needed, fully Send-safe)
    let entries = match fetch_os_catalog().await {
        Ok(entries) => entries,
        Err(e) => {
            // Create sync service just to get failure status
            if let Ok(db) = open_database() {
                if let Ok(sync_service) = OsCatalogSyncService::new(db) {
                    return Ok(sync_service.failure_status(e.to_string()));
                }
            }
            return Err(format!("Fetch failed: {}", e));
        }
    };

    // Step 2: Save to database synchronously (no async/await after this)
    let db = open_database()?;

    let sync_service = OsCatalogSyncService::new(db)
        .map_err(|e| format!("Failed to create sync service: {}", e))?;

    sync_service
        .save_entries(entries)
        .map_err(|e| format!("Save failed: {}", e))
}

#[tauri::command]
fn get_last_catalog_sync() -> Result<Option<String>, String> {
    let db = cached_database()?;
    db.get_last_catalog_sync_time()
        .map_err(|e| format!("Failed to get last catalog sync: {}", e))
}

fn to_ui_image(image: &bitosdt::core::Image) -> UiImage {
    let status = match image.status {
        bitosdt::core::ImageStatus::Draft => "Draft",
        bitosdt::core::ImageStatus::Ready => "Ready",
        bitosdt::core::ImageStatus::Building => "Building",
        bitosdt::core::ImageStatus::Failed => "Error",
    };

    UiImage {
        id: image.id.to_string(),
        name: image.name.clone(),
        description: image.description.clone().unwrap_or_default(),
        os_type: image.os_info.os_type.display_name().to_string(),
        os_version: image.os_info.version.clone(),
        os_architecture: image.os_info.architecture.as_str().to_string(),
        os_language: image.os_info.language.clone(),
        license_type: format!("{:?}", image.license.license_type),
        status: status.to_string(),
        created_at: image.created_at.to_rfc3339(),
        updated_at: image.updated_at.to_rfc3339(),
        size_bytes: image.size_bytes,
        iso_path: image
            .iso_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        has_saved_wizard_state: image.wizard_state_json.is_some(),
    }
}

fn derive_legacy_wizard_state(image: &bitosdt::core::Image) -> serde_json::Value {
    json!({
        "currentStep": 0,
        "windowsVersion": {
            "name": image.os_info.os_type.display_name(),
            "build": image.os_info.version,
            "edition": format!("{:?}", image.license.license_type),
            "language": image.os_info.language,
            "sourceType": "cloud"
        },
        "oobeConfig": {
            "skipMachineOobe": true,
            "skipUserOobe": true,
            "hideEula": true,
            "hideWirelessSetup": true,
            "hideLocalAccountScreen": false,
            "hideOnlineAccountScreens": true,
            "networkLocation": "Work",
            "protectYourPc": "Recommended",
            "computerName": ""
        },
        "userAccounts": [],
        "domainJoin": {
            "enabled": false,
            "domain": "",
            "username": "",
            "password": "",
            "ouPath": "",
            "promptForDomainCredentialsAtRuntime": false
        },
        "autopilot": {
            "enabled": false,
            "tenantId": "",
            "deploymentMode": "UserDriven",
            "skipUserOobe": true,
            "skipDeviceOobe": true,
            "allowWhiteglove": false,
            "groupTag": ""
        },
        "apps": {
            "wingetPackages": [],
            "chocolateyPackages": [],
            "customInstallers": [],
            "copiedItems": [],
            "copyDestination": "",
            "enableCustomScripts": false,
            "customScripts": [],
            "autoInstallChocolatey": true,
            "continueOnError": true
        },
        "windowsUpdate": {
            "enabled": true,
            "installSecurityUpdates": true,
            "installCriticalUpdates": true,
            "installDriverUpdates": false,
            "excludePreview": true,
            "excludeOptional": true,
            "rebootBehavior": "NoReboot"
        },
        "groupPolicies": bitosdt::policy::empty_group_policy_selection_value(),
        "output": {
            "outputType": "FullISO",
            "outputPath": image.iso_path.as_ref().map(|value| value.to_string_lossy().to_string()).unwrap_or_default(),
            "volumeLabel": "BITOSDT",
            "deliveryMode": "Simple",
            "wdsRuntimeSource": "UNC",
            "serverUrl": "http://deploy.local:8080",
            "pxeExportPath": "",
            "driverPaths": [],
            "applyDriversToOfflineWindows": false,
            "includeGui": true,
            "fullIsoUncPath": "",
            "fullIsoUncUsername": "",
            "fullIsoUncPassword": "",
            "fullIsoHttpUrl": "",
            "promptUncCredentialsAtRuntime": false
        }
    })
}

#[tauri::command]
fn list_images() -> Result<Vec<UiImage>, String> {
    let db = cached_database()?;
    let images = db
        .list_images()
        .map_err(|e| format!("Failed to list images: {}", e))?;
    Ok(images.iter().map(to_ui_image).collect())
}

#[tauri::command]
fn get_image_edit_payload(image_id: String) -> Result<UiImageEditPayload, String> {
    let db = cached_database()?;
    let id = Uuid::parse_str(&image_id).map_err(|e| format!("Invalid image id: {}", e))?;
    let image = db
        .get_image(id)
        .map_err(|e| format!("Failed to load image: {}", e))?
        .ok_or_else(|| "Image not found".to_string())?;

    let (wizard_state, legacy_defaults_applied, legacy_warning) = if let Some(saved) =
        image.wizard_state_json.clone()
    {
        let invalid_wds_warning = detect_invalid_saved_wds_runtime_warning(&saved);
        (saved, false, invalid_wds_warning)
    } else {
        (
                derive_legacy_wizard_state(&image),
                true,
                Some(
                    "Legacy profile defaults were applied because this image was created before full wizard-state persistence."
                        .to_string(),
                ),
            )
    };

    Ok(UiImageEditPayload {
        image: to_ui_image(&image),
        wizard_state,
        legacy_defaults_applied,
        legacy_warning,
    })
}

#[tauri::command]
fn delete_image(image_id: String) -> Result<(), String> {
    let db = cached_database()?;
    let id = Uuid::parse_str(&image_id).map_err(|e| format!("Invalid image id: {}", e))?;
    let deleted = db
        .delete_image(id)
        .map_err(|e| format!("Failed to delete image: {}", e))?;
    if !deleted {
        return Err("Image not found".to_string());
    }
    Ok(())
}

#[tauri::command]
fn duplicate_image(image_id: String) -> Result<UiImage, String> {
    let db = cached_database()?;
    let id = Uuid::parse_str(&image_id).map_err(|e| format!("Invalid image id: {}", e))?;
    let mut image = db
        .get_image(id)
        .map_err(|e| format!("Failed to load image: {}", e))?
        .ok_or_else(|| "Image not found".to_string())?;

    let now = Utc::now();
    image.id = Uuid::new_v4();
    image.name = format!("{} (Copy)", image.name);
    image.status = bitosdt::core::ImageStatus::Draft;
    image.created_at = now;
    image.updated_at = now;
    image.built_at = None;

    db.create_image(&image)
        .map_err(|e| format!("Failed to duplicate image: {}", e))?;

    Ok(to_ui_image(&image))
}

#[tauri::command]
fn list_usb_targets() -> Result<Vec<usb::UsbTarget>, String> {
    usb::list_usb_targets()
}

#[tauri::command]
fn write_iso_to_usb(request: usb::WriteIsoToUsbRequest) -> Result<String, String> {
    usb::write_iso_to_usb(request)
}

#[tauri::command]
fn write_provisioning_bundle_to_usb(
    request: usb::WriteProvisioningBundleRequest,
) -> Result<String, String> {
    usb::write_provisioning_bundle_to_usb(request)
}

#[tauri::command]
fn get_feature_status() -> FeatureStatus {
    FeatureStatus {
        windows_source_selection: true,
        oobe_configuration: true,
        domain_join: true,
        autopilot_integration: true,
        application_installation: true,
        windows_update: true,
        full_iso_output: true,
        lightweight_iso: true,
        image_manager: true,
    }
}

// ============================================================================
// OOBE Profile Commands
// ============================================================================

#[tauri::command]
fn create_oobe_profile(
    request: oobe_profiles::OobeProfileRequest,
) -> Result<oobe_profiles::OobeProfileSummary, String> {
    oobe_profiles::create_oobe_profile(request)
}

#[tauri::command]
fn list_oobe_profiles() -> Result<Vec<oobe_profiles::OobeProfileSummary>, String> {
    oobe_profiles::list_oobe_profiles()
}

#[tauri::command]
fn get_oobe_profile(name: String) -> Result<oobe_profiles::OobeProfileDetail, String> {
    oobe_profiles::get_oobe_profile(name)
}

#[tauri::command]
fn rename_oobe_profile(
    name: String,
    new_name: String,
) -> Result<oobe_profiles::OobeProfileSummary, String> {
    oobe_profiles::rename_oobe_profile(name, new_name)
}

#[tauri::command]
fn duplicate_oobe_profile(
    name: String,
    new_name: String,
) -> Result<oobe_profiles::OobeProfileSummary, String> {
    oobe_profiles::duplicate_oobe_profile(name, new_name)
}

#[tauri::command]
fn delete_oobe_profile(name: String) -> Result<(), String> {
    oobe_profiles::delete_oobe_profile(name)
}

#[tauri::command]
fn export_oobe_profile_zip(name: String, output_zip_path: String) -> Result<String, String> {
    oobe_profiles::export_oobe_profile_zip(name, output_zip_path)
}

#[tauri::command]
fn import_oobe_profile_zip(zip_path: String) -> Result<oobe_profiles::OobeProfileSummary, String> {
    oobe_profiles::import_oobe_profile_zip(zip_path)
}

#[tauri::command]
fn preflight_oobe_profile(name: String) -> Result<oobe_profiles::OobeProfilePreflight, String> {
    oobe_profiles::preflight_oobe_profile(name)
}

#[tauri::command]
fn generate_oobe_ppkg(request: ppkg::PpkgRequest) -> Result<ppkg::PpkgResponse, String> {
    ppkg::generate_oobe_ppkg(request)
}

#[tauri::command]
fn get_ppkg_capability_status(
    builder_path: Option<String>,
) -> Result<ppkg::PpkgCapabilityStatus, String> {
    Ok(ppkg::get_ppkg_capability_status(builder_path))
}

// ============================================================================
// Settings Commands
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub value_type: String,
}

#[derive(Debug, Clone, Serialize)]
struct CachePathSummary {
    key: String,
    label: String,
    path: String,
    exists: bool,
    file_count: u64,
    directory_count: u64,
    total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
struct CacheClearSummary {
    removable_paths: Vec<CachePathSummary>,
    preserved_paths: Vec<CachePathSummary>,
    os_catalog_entries: u32,
    driver_catalog_entries: u32,
    driver_cache_records: u32,
}

#[derive(Debug, Clone, Serialize)]
struct CacheClearResult {
    summary: CacheClearSummary,
    deleted_files: u64,
    deleted_directories: u64,
    deleted_bytes: u64,
    warnings: Vec<String>,
}

fn summarize_path_contents(path: &Path, label: &str, key: &str) -> CachePathSummary {
    fn walk(path: &Path) -> (u64, u64, u64) {
        let mut file_count = 0u64;
        let mut directory_count = 0u64;
        let mut total_bytes = 0u64;

        let Ok(entries) = std::fs::read_dir(path) else {
            return (file_count, directory_count, total_bytes);
        };

        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                directory_count += 1;
                let (nested_files, nested_dirs, nested_bytes) = walk(&entry.path());
                file_count += nested_files;
                directory_count += nested_dirs;
                total_bytes += nested_bytes;
            } else {
                file_count += 1;
                total_bytes += metadata.len();
            }
        }

        (file_count, directory_count, total_bytes)
    }

    let exists = path.exists();
    let (file_count, directory_count, total_bytes) = if exists {
        match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => (1, 0, metadata.len()),
            Ok(_) => walk(path),
            Err(_) => (0, 0, 0),
        }
    } else {
        (0, 0, 0)
    };

    CachePathSummary {
        key: key.to_string(),
        label: label.to_string(),
        path: path.to_string_lossy().to_string(),
        exists,
        file_count,
        directory_count,
        total_bytes,
    }
}

fn build_cache_clear_summary() -> Result<CacheClearSummary, String> {
    let config = load_config()?;
    let db = cached_database()?;

    Ok(CacheClearSummary {
        removable_paths: vec![summarize_path_contents(
            &config.settings.download_path,
            "Downloaded images and cache",
            "download_cache",
        )],
        preserved_paths: vec![
            summarize_path_contents(
                &config.settings.workspace_path,
                "Workspace builds and exported output",
                "workspace",
            ),
            summarize_path_contents(
                &config.database_path,
                "Saved image profiles and app settings database",
                "database",
            ),
        ],
        os_catalog_entries: db.count_os_versions().unwrap_or(0),
        driver_catalog_entries: db.count_driverpacks().unwrap_or(0),
        driver_cache_records: db.count_driver_cache_records().unwrap_or(0),
    })
}

fn clear_directory_contents(path: &Path) -> Result<(u64, u64), String> {
    let mut deleted_files = 0u64;
    let mut deleted_directories = 0u64;

    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create cache directory {}: {}", path.display(), e))?;
        return Ok((0, 0));
    }

    for entry in std::fs::read_dir(path)
        .map_err(|e| format!("Failed to read cache directory {}: {}", path.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to enumerate cache entry: {}", e))?;
        let entry_path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to inspect {}: {}", entry_path.display(), e))?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(&entry_path).map_err(|e| {
                format!("Failed to remove directory {}: {}", entry_path.display(), e)
            })?;
            deleted_directories += 1;
        } else {
            std::fs::remove_file(&entry_path)
                .map_err(|e| format!("Failed to remove file {}: {}", entry_path.display(), e))?;
            deleted_files += 1;
        }
    }

    std::fs::create_dir_all(path).map_err(|e| {
        format!(
            "Failed to recreate cache directory {}: {}",
            path.display(),
            e
        )
    })?;
    Ok((deleted_files, deleted_directories))
}

#[tauri::command]
fn get_cache_clear_summary() -> Result<CacheClearSummary, String> {
    build_cache_clear_summary()
}

#[tauri::command]
fn clear_download_cache() -> Result<CacheClearResult, String> {
    let summary = build_cache_clear_summary()?;
    let config = load_config()?;
    let db = cached_database()?;

    let mut warnings = Vec::new();
    let (deleted_files, deleted_directories) =
        clear_directory_contents(&config.settings.download_path)?;

    if let Err(error) = db.clear_os_versions() {
        warnings.push(format!("Failed to clear synced OS catalog rows: {}", error));
    }
    if let Err(error) = db.clear_driverpacks() {
        warnings.push(format!(
            "Failed to clear synced driver catalog rows: {}",
            error
        ));
    }
    if let Err(error) = db.clear_driver_cache_records() {
        warnings.push(format!("Failed to clear driver cache records: {}", error));
    }

    Ok(CacheClearResult {
        deleted_files,
        deleted_directories,
        deleted_bytes: summary
            .removable_paths
            .iter()
            .map(|entry| entry.total_bytes)
            .sum(),
        summary,
        warnings,
    })
}

#[tauri::command]
fn get_settings() -> Result<Vec<Setting>, String> {
    let mut config = load_config()?;
    let db = cached_database()?;

    let mut config_changed = false;
    for key in [
        "theme",
        "language",
        "download_path",
        "workspace_path",
        "adk_path",
    ] {
        if let Some(value) = db
            .get_setting(key)
            .map_err(|e| format!("Failed to get setting {}: {}", key, e))?
        {
            if !value.trim().is_empty() {
                if apply_setting_to_config(&mut config, key, &value) {
                    config_changed = true;
                }
            }
        }
    }

    if config_changed {
        config
            .save()
            .map_err(|e| format!("Failed to save config: {}", e))?;
    }

    let auto_sync_catalogs = db
        .get_setting("auto_sync_catalogs")
        .map_err(|e| format!("Failed to get setting auto_sync_catalogs: {}", e))?
        .unwrap_or_else(|| "true".to_string());

    let sync_frequency_hours = db
        .get_setting("sync_frequency_hours")
        .map_err(|e| format!("Failed to get setting sync_frequency_hours: {}", e))?
        .unwrap_or_else(|| "24".to_string());

    Ok(vec![
        Setting {
            key: "theme".to_string(),
            value: config.settings.theme.clone(),
            value_type: "string".to_string(),
        },
        Setting {
            key: "language".to_string(),
            value: config.settings.default_language.clone(),
            value_type: "string".to_string(),
        },
        Setting {
            key: "download_path".to_string(),
            value: config.settings.download_path.to_string_lossy().to_string(),
            value_type: "string".to_string(),
        },
        Setting {
            key: "workspace_path".to_string(),
            value: config.settings.workspace_path.to_string_lossy().to_string(),
            value_type: "string".to_string(),
        },
        Setting {
            key: "adk_path".to_string(),
            value: config
                .settings
                .adk_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            value_type: "string".to_string(),
        },
        Setting {
            key: "auto_sync_catalogs".to_string(),
            value: auto_sync_catalogs,
            value_type: "bool".to_string(),
        },
        Setting {
            key: "sync_frequency_hours".to_string(),
            value: sync_frequency_hours,
            value_type: "int".to_string(),
        },
        Setting {
            key: "suppress_credential_warning".to_string(),
            value: config.settings.suppress_credential_warning.to_string(),
            value_type: "bool".to_string(),
        },
    ])
}

fn apply_setting_to_config(config: &mut bitosdt::core::Config, key: &str, value: &str) -> bool {
    match key {
        "theme" => {
            config.settings.theme = value.to_string();
            true
        }
        "language" => {
            config.settings.default_language = value.to_string();
            true
        }
        "download_path" => {
            config.settings.download_path = std::path::PathBuf::from(value);
            true
        }
        "workspace_path" => {
            config.settings.workspace_path = std::path::PathBuf::from(value);
            true
        }
        "adk_path" => {
            let trimmed = value.trim();
            config.settings.adk_path = if trimmed.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(trimmed))
            };
            true
        }
        "suppress_credential_warning" => {
            config.settings.suppress_credential_warning = value == "true";
            true
        }
        _ => false,
    }
}

#[tauri::command]
fn set_setting(key: String, value: String, value_type: String) -> Result<(), String> {
    let mut config = load_config()?;
    let db = cached_database()?;

    if apply_setting_to_config(&mut config, &key, &value) {
        config
            .save()
            .map_err(|e| format!("Failed to save config: {}", e))?;
    }

    db.set_setting(&key, &value, &value_type)
        .map_err(|e| format!("Failed to set setting: {}", e))?;

    if key == GROUP_POLICY_PRESETS_SETTING_KEY {
        let mut bootstrap = load_policy_editor_bootstrap_cached(false)?;
        bootstrap.saved_presets = load_saved_policy_presets_from_database()?;
        *policy_bootstrap_cache()
            .lock()
            .expect("policy bootstrap cache poisoned") = Some(bootstrap);
    }

    Ok(())
}

#[tauri::command]
fn get_credential_warning_suppressed() -> Result<bool, String> {
    let config = load_config()?;
    Ok(config.settings.suppress_credential_warning)
}

#[tauri::command]
fn set_credential_warning_suppressed(suppressed: bool) -> Result<(), String> {
    let mut config = load_config()?;
    config.settings.suppress_credential_warning = suppressed;
    config
        .save()
        .map_err(|e| format!("Failed to save credential warning setting: {}", e))?;
    let db = cached_database()?;
    db.set_setting(
        "suppress_credential_warning",
        &suppressed.to_string(),
        "bool",
    )
    .map_err(|e| format!("Failed to persist credential warning setting: {}", e))?;
    Ok(())
}

fn write_wds_export_readme(
    export_dir: &Path,
    manifest: &WdsExportManifest,
) -> Result<PathBuf, String> {
    let sign_in_level = match manifest.sign_in_readiness.level {
        SignInReadinessLevel::Ready => "ready",
        SignInReadinessLevel::Warning => "warning",
        SignInReadinessLevel::Blocked => "blocked",
    };
    let sign_in_details = if manifest.sign_in_readiness.details.is_empty() {
        "- No additional sign-in notes were generated.\n".to_string()
    } else {
        manifest
            .sign_in_readiness
            .details
            .iter()
            .map(|detail| format!("- {}\n", detail))
            .collect::<String>()
    };
    let content = format!(
        concat!(
            "BitOSDT WDS/PXE Export\n",
            "======================\n\n",
            "Build: {windows_version} {windows_build} {windows_edition}\n",
            "Export Folder: {export_folder}\n",
            "Boot WIM: {boot_wim}\n",
            "Payload: {payload}\n",
            "Expected Payload Size: {expected_payload_size_bytes} bytes\n",
            "Expected Payload SHA-256: {expected_payload_sha256}\n",
            "Runtime Source: {runtime_source_kind}\n",
            "Final Runtime Path: {runtime_source_value}\n",
            "UNC Authentication: {runtime_unc_auth}\n\n",
            "First Sign-In Readiness: {sign_in_level}\n",
            "First Sign-In Summary: {sign_in_summary}\n",
            "First Sign-In Notes:\n",
            "{sign_in_details}\n",
            "Workflow:\n",
            "1. Import boot.wim into Windows Deployment Services.\n",
            "2. Copy the payload file to your SMB or HTTP hosting location.\n",
            "3. Ensure the hosted file path exactly matches the final runtime path above.\n",
            "4. PXE boot with WDS and let BitOSDT deploy from the remote image.\n\n",
            "Notes:\n",
            "- BitOSDT exports artifacts locally only.\n",
            "- BitOSDT does not publish the payload to WDS, SMB, or HTTP for you.\n",
            "- PXE deployment now validates the hosted payload size and SHA-256 before applying Windows.\n",
            "- If validation fails, host the exported prepared WIM from C:\\BitOSDT\\WDS at the configured runtime path.\n",
            "- Dedicated SMB or HTTP hosting is recommended over relying on WDS REMINST layout.\n"
        ),
        windows_version = manifest.windows_version,
        windows_build = manifest.windows_build,
        windows_edition = manifest.windows_edition,
        export_folder = manifest.export_folder,
        boot_wim = manifest.boot_wim_path,
        payload = manifest.payload_path,
        expected_payload_size_bytes = manifest.expected_payload_size_bytes,
        expected_payload_sha256 = manifest.expected_payload_sha256,
        runtime_source_kind = manifest.runtime_source_kind,
        runtime_source_value = manifest.runtime_source_value,
        runtime_unc_auth = if manifest.runtime_source_kind == "UNC" {
            if manifest.runtime_unc_auth_configured {
                "configured"
            } else {
                "not configured"
            }
        } else {
            "n/a"
        },
        sign_in_level = sign_in_level,
        sign_in_summary = manifest.sign_in_readiness.summary,
        sign_in_details = sign_in_details,
    );
    let readme_path = export_dir.join("README-WDS-PXE.txt");
    std::fs::write(&readme_path, content)
        .map_err(|e| format!("Failed to write WDS/PXE README: {}", e))?;
    Ok(readme_path)
}

fn export_wds_pxe_bundle(
    request: &ImageBuildRequest,
    build_result: &bitosdt::build::FullIsoBuildResult,
    unc_image_path: Option<String>,
    unc_auth_username: Option<String>,
    http_image_url: Option<String>,
    sign_in_readiness: SignInReadiness,
) -> Result<PathBuf, String> {
    export_wds_pxe_bundle_to_dir(
        request,
        build_result,
        unc_image_path,
        unc_auth_username,
        http_image_url,
        sign_in_readiness,
        &PathBuf::from(WDS_EXPORT_ROOT),
    )
}

fn export_wds_pxe_bundle_to_dir(
    request: &ImageBuildRequest,
    build_result: &bitosdt::build::FullIsoBuildResult,
    unc_image_path: Option<String>,
    unc_auth_username: Option<String>,
    http_image_url: Option<String>,
    sign_in_readiness: SignInReadiness,
    export_dir: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("Failed to create WDS export directory: {}", e))?;

    let boot_wim_source = build_result
        .winpe_dir
        .join("media")
        .join("sources")
        .join("boot.wim");
    if !boot_wim_source.exists() {
        return Err(format!(
            "WDS export failed because boot.wim was not found at {}",
            boot_wim_source.display()
        ));
    }

    let payload_file_name = build_result
        .prepared_wim_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("install.wim");

    let boot_wim_export = export_dir.join("boot.wim");
    let payload_export = export_dir.join(payload_file_name);

    std::fs::copy(&boot_wim_source, &boot_wim_export)
        .map_err(|e| format!("Failed to export boot.wim: {}", e))?;
    std::fs::copy(&build_result.prepared_wim_path, &payload_export)
        .map_err(|e| format!("Failed to export Windows payload: {}", e))?;

    let (runtime_source_kind, runtime_source_value) =
        match (unc_image_path.as_ref(), http_image_url.as_ref()) {
            (Some(path), None) => ("UNC".to_string(), path.clone()),
            (None, Some(url)) => ("HTTP".to_string(), url.clone()),
            (Some(path), Some(_)) => ("UNC".to_string(), path.clone()),
            (None, None) => ("UNC".to_string(), "<not configured>".to_string()),
        };

    let manifest = WdsExportManifest {
        export_folder: export_dir.to_string_lossy().to_string(),
        boot_wim_path: boot_wim_export.to_string_lossy().to_string(),
        payload_path: payload_export.to_string_lossy().to_string(),
        expected_payload_size_bytes: build_result.payload_provenance.size_bytes,
        expected_payload_sha256: build_result.payload_provenance.sha256.clone(),
        expected_payload_file_name: build_result.payload_provenance.file_name.clone(),
        runtime_source_kind,
        runtime_source_value,
        runtime_unc_path: unc_image_path,
        runtime_unc_auth_configured: unc_auth_username.is_some(),
        runtime_http_url: http_image_url,
        windows_version: request.windows_version.clone(),
        windows_build: request.windows_build.clone(),
        windows_edition: request.windows_edition.clone(),
        source_path: build_result.source_path.to_string_lossy().to_string(),
        sign_in_readiness,
    };

    write_wds_export_readme(&export_dir, &manifest)?;

    let manifest_path = export_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize WDS/PXE manifest: {}", e))?;
    std::fs::write(&manifest_path, manifest_json)
        .map_err(|e| format!("Failed to write WDS/PXE manifest: {}", e))?;

    Ok(boot_wim_export)
}

#[tauri::command]
fn sync_driver_catalog() -> Result<DriverSyncResponse, String> {
    use bitosdt::catalog::CatalogSyncService;
    let db = open_database()?;

    let sync_service =
        CatalogSyncService::new(db).map_err(|e| format!("Failed to create sync service: {}", e))?;
    let results = tauri::async_runtime::block_on(sync_service.sync_all())
        .map_err(|e| format!("Driver catalog sync failed: {}", e))?;

    let synced_sources = results.iter().filter(|r| r.last_sync_success).count() as u32;
    let errors = results
        .into_iter()
        .filter_map(|r| {
            if r.last_sync_success {
                None
            } else {
                Some(format!(
                    "{}: {}",
                    r.manufacturer,
                    r.error_message
                        .unwrap_or_else(|| "Unknown error".to_string())
                ))
            }
        })
        .collect();

    Ok(DriverSyncResponse {
        started: true,
        synced_sources,
        errors,
    })
}

// ============================================================================
// File Dialog Commands
// ============================================================================

#[tauri::command]
async fn show_save_dialog(
    default_path: Option<String>,
    title: Option<String>,
) -> Result<Option<String>, String> {
    use tauri::api::dialog::FileDialogBuilder;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut dialog = FileDialogBuilder::new();

    if let Some(title_str) = title {
        dialog = dialog.set_title(&title_str);
    }

    if let Some(path) = default_path {
        dialog = dialog.set_file_name(&path);
    }

    dialog
        .add_filter("ISO Image", &["iso"])
        .add_filter("All Files", &["*"])
        .save_file(move |path| {
            let _ = tx.send(path.map(|p| p.to_string_lossy().to_string()));
        });

    rx.recv()
        .map_err(|e| format!("Failed to receive dialog result: {}", e))
}

#[tauri::command]
async fn show_save_dialog_with_filters(
    default_path: Option<String>,
    title: Option<String>,
    filters: Option<Vec<(String, Vec<String>)>>,
) -> Result<Option<String>, String> {
    use tauri::api::dialog::FileDialogBuilder;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut dialog = FileDialogBuilder::new();

    if let Some(title_str) = title {
        dialog = dialog.set_title(&title_str);
    }

    if let Some(path) = default_path {
        dialog = dialog.set_file_name(&path);
    }

    if let Some(filter_list) = filters {
        for (name, extensions) in filter_list {
            let ext_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&name, &ext_refs);
        }
    } else {
        dialog = dialog
            .add_filter("ZIP Archive", &["zip"])
            .add_filter("All Files", &["*"]);
    }

    dialog.save_file(move |path| {
        let _ = tx.send(path.map(|p| p.to_string_lossy().to_string()));
    });

    rx.recv()
        .map_err(|e| format!("Failed to receive dialog result: {}", e))
}

#[tauri::command]
async fn show_open_dialog(
    title: Option<String>,
    filters: Option<Vec<(String, Vec<String>)>>,
) -> Result<Option<String>, String> {
    use tauri::api::dialog::FileDialogBuilder;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut dialog = FileDialogBuilder::new();

    if let Some(title_str) = title {
        dialog = dialog.set_title(&title_str);
    }

    // Apply filters or use defaults for Windows images
    if let Some(filter_list) = filters {
        for (name, extensions) in filter_list {
            let ext_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
            dialog = dialog.add_filter(&name, &ext_refs);
        }
    } else {
        dialog = dialog
            .add_filter("Windows Images", &["iso", "esd", "wim"])
            .add_filter("ISO Image", &["iso"])
            .add_filter("ESD Image", &["esd"])
            .add_filter("WIM Image", &["wim"])
            .add_filter("All Files", &["*"]);
    }

    dialog.pick_file(move |path| {
        let _ = tx.send(path.map(|p| p.to_string_lossy().to_string()));
    });

    rx.recv()
        .map_err(|e| format!("Failed to receive dialog result: {}", e))
}

#[tauri::command]
async fn show_folder_dialog(title: Option<String>) -> Result<Option<String>, String> {
    use tauri::api::dialog::FileDialogBuilder;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut dialog = FileDialogBuilder::new();

    if let Some(title_str) = title {
        dialog = dialog.set_title(&title_str);
    }

    dialog.pick_folder(move |path| {
        let _ = tx.send(path.map(|p| p.to_string_lossy().to_string()));
    });

    rx.recv()
        .map_err(|e| format!("Failed to receive dialog result: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn make_test_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bitosdt-main-{}-{}", prefix, Uuid::new_v4()))
    }

    fn write_dummy_chrome(packages_dir: &Path) {
        fs::create_dir_all(packages_dir.join("chrome")).expect("create chrome dir");
        fs::write(
            packages_dir.join("chrome").join("chrome.exe"),
            b"dummy chrome",
        )
        .expect("write chrome executable");
    }

    fn write_dummy_ui(ui_dir: &Path) {
        fs::create_dir_all(ui_dir).expect("create ui dir");
        fs::write(ui_dir.join("index.html"), "<!doctype html><html></html>")
            .expect("write ui index");
    }

    #[test]
    fn build_process_state_tracks_and_cancels_registered_processes() {
        let state = BuildProcessState::new();
        state.begin_build().expect("begin build");
        state.register_process(TrackedBuildProcess {
            pid: 1234,
            executable: "dism.exe".to_string(),
            command_line: "dism.exe /Mount-Wim".to_string(),
            role: "prepare-mount".to_string(),
        });

        let processes = state.request_cancel();
        assert!(state.is_cancel_requested());
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].pid, 1234);

        state.finish_build();
        assert!(!state.is_active());
        assert!(!state.is_cancel_requested());
    }

    #[test]
    fn build_process_state_unregister_removes_process() {
        let state = BuildProcessState::new();
        state.begin_build().expect("begin build");
        state.register_process(TrackedBuildProcess {
            pid: 4321,
            executable: "oscdimg.exe".to_string(),
            command_line: "oscdimg.exe -m".to_string(),
            role: "iso-create".to_string(),
        });

        state.unregister_process(4321);
        assert!(state.request_cancel().is_empty());
    }

    #[test]
    fn build_workspace_recovery_response_marks_locked_with_matches() {
        let path = Path::new(r"C:\BitOSDT\Workspace\install.wim");
        let response = build_workspace_recovery_response(
            path,
            "The process cannot access the file because it is being used by another process. (os error 32)".to_string(),
            vec![BuildWorkspaceRecoveryProcess {
                pid: 1200,
                executable: "dism.exe".to_string(),
                command_line: "dism.exe /Export-Image /DestinationImageFile:C:\\BitOSDT\\Workspace\\install.wim".to_string(),
            }],
        );

        assert!(matches!(
            response.status,
            BuildWorkspaceRecoveryStatus::LockedWithMatches
        ));
        assert_eq!(
            response.locked_path.as_deref(),
            Some(r"C:\BitOSDT\Workspace\install.wim")
        );
        assert_eq!(response.processes.len(), 1);
    }

    #[test]
    fn build_workspace_recovery_response_marks_locked_without_matches() {
        let path = Path::new(r"C:\BitOSDT\Workspace\install.wim");
        let response = build_workspace_recovery_response(
            path,
            "Access is denied. (os error 5)".to_string(),
            Vec::new(),
        );

        assert!(matches!(
            response.status,
            BuildWorkspaceRecoveryStatus::LockedWithoutMatches
        ));
        assert!(response.processes.is_empty());
    }

    #[test]
    fn inspect_build_workspace_recovery_returns_none_for_clean_workspace() {
        let root = make_test_root("workspace-recovery-clean");
        fs::create_dir_all(&root).expect("create workspace root");

        let result = inspect_build_workspace_recovery(&root).expect("inspect clean workspace");
        assert!(result.is_none());

        let _ = fs::remove_dir_all(root);
    }

    fn base_oobe_config(computer_name: Option<&str>) -> serde_json::Value {
        json!({
            "skipMachineOobe": false,
            "skipUserOobe": false,
            "hideEula": true,
            "hideWirelessSetup": true,
            "hideLocalAccountScreen": false,
            "hideOnlineAccountScreens": true,
            "networkLocation": "Work",
            "protectYourPc": "Recommended",
            "computerName": computer_name.unwrap_or("")
        })
    }

    fn base_apps_config() -> serde_json::Value {
        json!({
            "wingetPackages": [],
            "chocolateyPackages": [],
            "customInstallers": [],
            "autoInstallChocolatey": true,
            "continueOnError": true,
            "enableCustomScripts": false,
            "customScripts": []
        })
    }

    fn base_windows_update_config() -> serde_json::Value {
        json!({
            "enabled": false,
            "installSecurityUpdates": false,
            "installCriticalUpdates": false,
            "installDriverUpdates": false,
            "excludePreview": true,
            "excludeOptional": true,
            "rebootBehavior": "NoReboot"
        })
    }

    fn make_request(output_type: &str) -> ImageBuildRequest {
        ImageBuildRequest {
            windows_version: "Windows 11".to_string(),
            windows_build: "23H2".to_string(),
            windows_edition: "Pro".to_string(),
            windows_channel: Some("Retail".to_string()),
            language: Some("en-us".to_string()),
            output_type: output_type.to_string(),
            output_path: "C:\\BitOSDT\\test.iso".to_string(),
            volume_label: "BITOSDT".to_string(),
            source_path: Some("C:\\BitOSDT\\install.wim".to_string()),
            download_url: None,
            target_disk: None,
            delivery_mode: Some("Simple".to_string()),
            server_url: Some("http://deploy.local:8080".to_string()),
            driver_paths: vec![],
            boot_driver_unc_path: None,
            apply_to_offline_windows: Some(false),
            runtime_driver_policy: None,
            pxe_export_path: Some("C:\\BitOSDT\\PXE".to_string()),
            full_iso_unc_path: Some("\\\\wds\\reminst\\images\\install.wim".to_string()),
            full_iso_unc_username: Some("CONTOSO\\deploy".to_string()),
            full_iso_unc_password: Some("Secret123!".to_string()),
            full_iso_http_url: Some("http://deploy.local/install.wim".to_string()),
            prompt_unc_credentials_at_runtime: None,
            include_gui: Some(true),
            existing_image_id: None,
            save_mode: None,
            oobe_config: base_oobe_config(None),
            user_accounts: vec![],
            domain_join: json!({
                "enabled": false,
                "domain": "",
                "username": "",
                "password": "",
                "ouPath": "",
                "promptForDomainCredentialsAtRuntime": false
            }),
            autopilot: json!({
                "enabled": false,
                "tenantId": "",
                "deploymentMode": "UserDriven",
                "skipUserOobe": true,
                "skipDeviceOobe": true,
                "allowWhiteglove": false,
                "groupTag": ""
            }),
            apps: base_apps_config(),
            windows_update: base_windows_update_config(),
            group_policies: bitosdt::policy::empty_group_policy_selection_value(),
            shell_layout: empty_shell_layout_value(),
        }
    }

    #[test]
    fn map_custom_source_type_defaults_to_direct_path() {
        assert_eq!(
            map_custom_installer_source_type(None),
            bitosdt::tasks::InstallerSourceType::DirectPathOrUrl
        );
        assert_eq!(
            map_custom_installer_source_type(Some("Unknown")),
            bitosdt::tasks::InstallerSourceType::DirectPathOrUrl
        );
    }

    #[test]
    fn map_custom_source_type_maps_new_values() {
        assert_eq!(
            map_custom_installer_source_type(Some("EmbeddedFile")),
            bitosdt::tasks::InstallerSourceType::EmbeddedFile
        );
        assert_eq!(
            map_custom_installer_source_type(Some("NetworkDirectory")),
            bitosdt::tasks::InstallerSourceType::NetworkDirectory
        );
    }

    #[test]
    fn validate_computer_name_accepts_auto_or_valid_explicit_values() {
        assert_eq!(validate_computer_name(None).unwrap(), None);
        assert_eq!(validate_computer_name(Some("")).unwrap(), None);
        assert_eq!(validate_computer_name(Some("   ")).unwrap(), None);
        assert_eq!(validate_computer_name(Some("*")).unwrap(), None);
        assert_eq!(
            validate_computer_name(Some("ENG-LAP-01")).unwrap(),
            Some("ENG-LAP-01".to_string())
        );
    }

    #[test]
    fn validate_computer_name_rejects_invalid_values() {
        assert!(validate_computer_name(Some("-bad")).is_err());
        assert!(validate_computer_name(Some("bad-")).is_err());
        assert!(validate_computer_name(Some("bad_name")).is_err());
        assert!(validate_computer_name(Some("name.with.dot")).is_err());
        assert!(validate_computer_name(Some("1234567890123456")).is_err());
    }

    #[test]
    fn build_unattend_config_maps_computer_name() {
        let mut request = make_request("FullISO");
        request.oobe_config = base_oobe_config(Some("ENG-WS-01"));

        let config = build_unattend_config(&request).expect("build unattend config");
        assert_eq!(config.computer_name.as_deref(), Some("ENG-WS-01"));
    }

    #[test]
    fn build_unattend_config_accepts_french_language() {
        let mut request = make_request("FullISO");
        request.language = Some("fr-fr".to_string());

        let config = build_unattend_config(&request).expect("build unattend config");
        assert_eq!(config.language, "fr-FR");
        assert_eq!(config.input_locale, "fr-FR");
    }

    #[test]
    fn build_unattend_config_rejects_invalid_language() {
        let mut request = make_request("FullISO");
        request.language = Some("fr--fr".to_string());

        let err = build_unattend_config(&request).expect_err("expected invalid language error");
        assert!(err.contains("Invalid language"));
        assert!(err.contains("BCP-47"));
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

        let config = build_unattend_config(&request).expect("local admin should satisfy skip");
        assert!(config.oobe.skip_user_oobe);
        assert_eq!(config.users.len(), 1);
    }

    #[test]
    fn build_unattend_config_allows_runtime_prompted_domain_join_with_blank_fields() {
        let mut request = make_request("FullISO");
        request.domain_join = json!({
            "enabled": true,
            "domain": "",
            "username": "",
            "password": "",
            "ouPath": "",
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
    fn build_task_sequence_appends_custom_scripts_after_built_in_tasks() {
        let mut request = make_request("FullISO");
        request.apps = json!({
            "wingetPackages": [
                {
                    "packageId": "Google.Chrome",
                    "enabled": true
                }
            ],
            "chocolateyPackages": [],
            "customInstallers": [],
            "autoInstallChocolatey": true,
            "continueOnError": true,
            "enableCustomScripts": true,
            "customScripts": [
                {
                    "name": "Script Alpha",
                    "content": "Write-Host 'alpha'",
                    "enabled": true,
                    "continueOnError": false
                },
                {
                    "name": "Script Beta",
                    "content": "Write-Host 'beta'",
                    "enabled": true,
                    "continueOnError": true
                }
            ]
        });
        request.windows_update = json!({
            "enabled": true,
            "installSecurityUpdates": true,
            "installCriticalUpdates": false,
            "installDriverUpdates": false,
            "excludePreview": true,
            "excludeOptional": true,
            "rebootBehavior": "NoReboot"
        });

        let sequence = build_task_sequence(&request)
            .expect("build task sequence")
            .expect("expected tasks");

        assert_eq!(sequence.tasks.len(), 4);
        assert_eq!(sequence.tasks[0].name, "Install Applications");
        assert_eq!(sequence.tasks[0].order, 10);
        assert_eq!(sequence.tasks[1].name, "Windows Update");
        assert_eq!(sequence.tasks[1].order, 20);
        assert_eq!(sequence.tasks[2].name, "Script Alpha");
        assert_eq!(sequence.tasks[2].order, 30);
        assert_eq!(sequence.tasks[3].name, "Script Beta");
        assert_eq!(sequence.tasks[3].order, 40);

        match &sequence.tasks[2].task_type {
            bitosdt::tasks::TaskType::CustomScript(script) => {
                assert_eq!(script.script_type, bitosdt::tasks::ScriptType::PowerShell);
                assert!(script.run_as_admin);
                assert_eq!(script.timeout_seconds, 0);
                assert!(!script.continue_on_error);
                assert_eq!(script.content, "Write-Host 'alpha'");
            }
            _ => panic!("expected custom script task"),
        }

        assert!(!sequence.tasks[2].continue_on_error);
        assert!(sequence.tasks[3].continue_on_error);
    }

    #[test]
    fn validate_lightweight_restrictions_rejects_explicit_computer_name() {
        let oobe: FrontendOobeConfig =
            parse_frontend(base_oobe_config(Some("ENG-WS-01")), "OOBE").expect("parse oobe");
        let apps: FrontendApps =
            parse_frontend(base_apps_config(), "applications").expect("parse apps");

        let err = validate_lightweight_restrictions("LightweightISO", &oobe, &apps)
            .expect_err("explicit computer name should be blocked");
        assert_eq!(
            err,
            "Computer name customization is supported only for Full ISO in this release."
        );
    }

    #[test]
    fn validate_lightweight_restrictions_rejects_enabled_custom_scripts() {
        let oobe: FrontendOobeConfig =
            parse_frontend(base_oobe_config(None), "OOBE").expect("parse oobe");
        let apps: FrontendApps = parse_frontend(
            json!({
                "wingetPackages": [],
                "chocolateyPackages": [],
                "customInstallers": [],
                "autoInstallChocolatey": true,
                "continueOnError": true,
                "enableCustomScripts": true,
                "customScripts": [
                    {
                        "name": "Script One",
                        "content": "Write-Host 'Hello'",
                        "enabled": true,
                        "continueOnError": true
                    }
                ]
            }),
            "applications",
        )
        .expect("parse apps");

        let err = validate_lightweight_restrictions("Both", &oobe, &apps)
            .expect_err("custom scripts should be blocked for lightweight output");
        assert_eq!(
            err,
            "Custom post-install scripts are supported only for Full ISO in this release."
        );
    }

    #[test]
    fn resolve_delivery_mode_defaults_to_simple() {
        let request = make_request("LightweightISO");
        assert_eq!(resolve_delivery_mode(&request), DeliveryMode::Simple);
    }

    #[test]
    fn resolve_delivery_mode_maps_advanced() {
        let mut request = make_request("LightweightISO");
        request.delivery_mode = Some("Advanced".to_string());
        assert_eq!(resolve_delivery_mode(&request), DeliveryMode::Advanced);
    }

    #[test]
    fn parse_save_mode_defaults_to_copy() {
        let request = make_request("FullISO");
        assert_eq!(parse_save_mode(&request), ImageSaveMode::Copy);
    }

    #[test]
    fn parse_save_mode_maps_overwrite_case_insensitive() {
        let mut request = make_request("FullISO");
        request.save_mode = Some("OvErWrItE".to_string());
        assert_eq!(parse_save_mode(&request), ImageSaveMode::Overwrite);
    }

    #[test]
    fn validate_driver_paths_rejects_network_paths() {
        let err = validate_driver_paths(&[r"\\server\share\drivers".to_string()])
            .expect_err("network driver path should fail");
        assert!(err.contains("must be a local path"));
    }

    #[test]
    fn validate_driver_paths_accepts_existing_local_path() {
        let root = make_test_root("drivers-local");
        let driver_dir = root.join("Drivers");
        fs::create_dir_all(&driver_dir).expect("create driver dir");

        let paths =
            validate_driver_paths(&[driver_dir.to_string_lossy().to_string()]).expect("paths");
        assert_eq!(paths, vec![driver_dir.clone()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validate_full_iso_remote_sources_accepts_unc_and_http() {
        let unc = Some(r"\\wds\reminst\images\install.wim".to_string());
        let unc_username = Some(r"CONTOSO\deploy".to_string());
        let unc_password = Some("Secret123!".to_string());
        let http = Some("http://deploy.local/install.wim".to_string());
        let (resolved_unc, resolved_http, resolved_username, resolved_password) =
            validate_full_iso_remote_sources(
                unc.as_ref(),
                unc_username.as_ref(),
                unc_password.as_ref(),
                http.as_ref(),
                false,
            )
            .expect("sources");
        assert_eq!(
            resolved_unc.as_deref(),
            Some(r"\\wds\reminst\images\install.wim")
        );
        assert_eq!(resolved_username.as_deref(), Some(r"CONTOSO\deploy"));
        assert_eq!(resolved_password.as_deref(), Some("Secret123!"));
        assert_eq!(
            resolved_http.as_deref(),
            Some("http://deploy.local/install.wim")
        );
    }

    #[test]
    fn validate_full_iso_remote_sources_rejects_invalid_unc() {
        let unc = Some(r"C:\images\install.wim".to_string());
        let err = validate_full_iso_remote_sources(unc.as_ref(), None, None, None, false)
            .expect_err("local path should not be accepted");
        assert!(err.contains("must start with \\\\"));
    }

    #[test]
    fn validate_full_iso_remote_sources_rejects_host_only_unc_file_path() {
        let unc = Some(r"\\host\install.wim".to_string());
        let err = validate_full_iso_remote_sources(unc.as_ref(), None, None, None, false)
            .expect_err("missing share should fail");
        assert!(err.contains("full UNC file path"));
    }

    #[test]
    fn validate_full_iso_remote_sources_rejects_empty_unc_share_segment() {
        let unc = Some(r"\\host\\install.wim".to_string());
        let err = validate_full_iso_remote_sources(unc.as_ref(), None, None, None, false)
            .expect_err("empty share segment should fail");
        assert!(err.contains("full UNC file path"));
    }

    #[test]
    fn validate_full_iso_remote_sources_rejects_invalid_http_url() {
        let http = Some("ftp://deploy.local/install.wim".to_string());
        let err = validate_full_iso_remote_sources(None, None, None, http.as_ref(), false)
            .expect_err("invalid schema should fail");
        assert!(err.contains("must start with http:// or https://"));
    }

    #[test]
    fn validate_full_iso_remote_sources_requires_unc_credentials_pair() {
        let unc = Some(r"\\wds\reminst\images\install.wim".to_string());
        let err = validate_full_iso_remote_sources(
            unc.as_ref(),
            Some(&"CONTOSO\\deploy".to_string()),
            None,
            None,
            false,
        )
        .expect_err("missing password should fail");
        assert!(err.contains("requires both a username and password"));

        let err = validate_full_iso_remote_sources(
            None,
            Some(&"CONTOSO\\deploy".to_string()),
            Some(&"Secret123!".to_string()),
            None,
            false,
        )
        .expect_err("credentials without path should fail");
        assert!(err.contains("UNC credentials require a UNC runtime path"));
    }

    #[test]
    fn validate_wds_pxe_runtime_source_requires_exactly_one_path() {
        let unc = Some(r"\\server\share\install.wim".to_string());
        let http = Some("http://deploy.local/install.wim".to_string());

        let err = validate_wds_pxe_runtime_source(None, None, None, None, false)
            .expect_err("missing runtime path should fail");
        assert!(err.contains("exactly one"));

        let err = validate_wds_pxe_runtime_source(unc.as_ref(), None, None, http.as_ref(), false)
            .expect_err("multiple runtime paths should fail");
        assert!(err.contains("exactly one"));

        let (resolved_unc, resolved_http, resolved_username, resolved_password) =
            validate_wds_pxe_runtime_source(
                unc.as_ref(),
                Some(&"CONTOSO\\deploy".to_string()),
                Some(&"Secret123!".to_string()),
                None,
                false,
            )
            .expect("unc runtime path");
        assert_eq!(resolved_unc.as_deref(), Some(r"\\server\share\install.wim"));
        assert!(resolved_http.is_none());
        assert_eq!(resolved_username.as_deref(), Some(r"CONTOSO\deploy"));
        assert_eq!(resolved_password.as_deref(), Some("Secret123!"));
    }

    #[test]
    fn validate_wds_pxe_runtime_source_rejects_host_only_unc_file_path() {
        let unc = Some(r"\\host\install.wim".to_string());
        let err = validate_wds_pxe_runtime_source(
            unc.as_ref(),
            Some(&"CONTOSO\\deploy".to_string()),
            Some(&"Secret123!".to_string()),
            None,
            false,
        )
        .expect_err("missing share should fail");
        assert!(err.contains("full UNC file path"));
    }

    #[test]
    fn build_wizard_state_json_preserves_wds_output_type_and_runtime_source() {
        let mut request = make_request("WDSPXE");
        request.output_path = WDS_EXPORT_ROOT.to_string();
        request.full_iso_unc_path = Some(r"\\server\share\install.wim".to_string());
        request.full_iso_unc_username = Some(r"CONTOSO\deploy".to_string());
        request.full_iso_unc_password = Some("Secret123!".to_string());
        request.full_iso_http_url = None;

        let wizard_state = build_wizard_state_json(&request);
        assert_eq!(wizard_state["output"]["outputType"], "WDSPXE");
        assert_eq!(wizard_state["output"]["outputPath"], WDS_EXPORT_ROOT);
        assert_eq!(
            wizard_state["output"]["fullIsoUncPath"],
            r"\\server\share\install.wim"
        );
        assert_eq!(
            wizard_state["output"]["fullIsoUncUsername"],
            r"CONTOSO\deploy"
        );
        assert_eq!(wizard_state["output"]["fullIsoUncPassword"], "Secret123!");
        assert_eq!(
            wizard_state["output"]["fullIsoHttpUrl"],
            serde_json::Value::Null
        );
        assert_eq!(wizard_state["output"]["wdsRuntimeSource"], "UNC");
    }

    #[test]
    fn build_wizard_state_json_clears_inactive_wds_runtime_source() {
        let mut request = make_request("WDSPXE");
        request.output_path = WDS_EXPORT_ROOT.to_string();
        request.full_iso_unc_path = Some(r"\\server\share\install.wim".to_string());
        request.full_iso_http_url = Some("http://deploy.local/install.wim".to_string());

        let wizard_state = build_wizard_state_json(&request);
        assert_eq!(
            wizard_state["output"]["fullIsoUncPath"],
            r"\\server\share\install.wim"
        );
        assert_eq!(
            wizard_state["output"]["fullIsoHttpUrl"],
            serde_json::Value::Null
        );
        assert_eq!(
            wizard_state["output"]["fullIsoUncUsername"],
            request.full_iso_unc_username.as_deref().unwrap()
        );
        assert_eq!(
            wizard_state["output"]["fullIsoUncPassword"],
            request.full_iso_unc_password.as_deref().unwrap()
        );
        assert_eq!(wizard_state["output"]["wdsRuntimeSource"], "UNC");
    }

    #[test]
    fn build_wizard_state_json_clears_unc_credentials_without_unc_path() {
        let mut request = make_request("FullISO");
        request.full_iso_unc_path = None;
        request.full_iso_http_url = Some("http://deploy.local/install.wim".to_string());

        let wizard_state = build_wizard_state_json(&request);
        assert_eq!(
            wizard_state["output"]["fullIsoUncUsername"],
            serde_json::Value::Null
        );
        assert_eq!(
            wizard_state["output"]["fullIsoUncPassword"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn detect_invalid_saved_wds_runtime_warning_flags_dual_source_state() {
        let warning = detect_invalid_saved_wds_runtime_warning(&json!({
            "output": {
                "outputType": "WDSPXE",
                "fullIsoUncPath": r"\\server\share\install.wim",
                "fullIsoHttpUrl": "http://deploy.local/install.wim"
            }
        }))
        .expect("warning");

        assert!(warning.contains("both UNC and HTTP runtime paths"));
    }

    #[test]
    fn export_wds_pxe_bundle_writes_boot_payload_and_metadata() {
        let root = make_test_root("wds-export");
        let export_dir = root.join("WDS");
        let winpe_sources = root.join("winpe").join("media").join("sources");
        fs::create_dir_all(&winpe_sources).expect("create winpe sources");
        fs::write(winpe_sources.join("boot.wim"), b"boot").expect("write boot.wim");

        let prepared_wim = root.join("prepared").join("install.wim");
        fs::create_dir_all(prepared_wim.parent().expect("prepared parent"))
            .expect("create prepared parent");
        fs::write(&prepared_wim, b"payload").expect("write install.wim");

        let request = make_request("WDSPXE");
        let build_result = bitosdt::build::FullIsoBuildResult {
            output_path: root.join("temp.iso"),
            prepared_wim_path: prepared_wim.clone(),
            payload_provenance: bitosdt::build::full_iso_builder::PayloadProvenance {
                size_bytes: 7,
                sha256: "abc123".to_string(),
                file_name: Some("install.wim".to_string()),
            },
            source_path: root.join("source.iso"),
            workspace: root.join("workspace"),
            winpe_dir: root.join("winpe"),
        };

        let boot_wim = export_wds_pxe_bundle_to_dir(
            &request,
            &build_result,
            Some(r"\\server\share\install.wim".to_string()),
            Some(r"CONTOSO\deploy".to_string()),
            None,
            assess_sign_in_readiness(false, 1, false, false),
            &export_dir,
        )
        .expect("export wds bundle");

        assert_eq!(boot_wim, export_dir.join("boot.wim"));
        assert!(export_dir.join("boot.wim").exists());
        assert!(export_dir.join("install.wim").exists());
        assert!(export_dir.join("README-WDS-PXE.txt").exists());
        assert!(export_dir.join("manifest.json").exists());

        let readme =
            fs::read_to_string(export_dir.join("README-WDS-PXE.txt")).expect("read readme");
        assert!(readme.contains("Import boot.wim into Windows Deployment Services."));
        assert!(readme.contains("Expected Payload Size: 7 bytes"));
        assert!(readme.contains("Expected Payload SHA-256: abc123"));
        assert!(readme.contains("Runtime Source: UNC"));
        assert!(readme.contains(r"\\server\share\install.wim"));
        assert!(readme.contains("UNC Authentication: configured"));
        assert!(readme.contains("First Sign-In Readiness: ready"));
        assert!(readme.contains("A local administrator sign-in path is configured"));
        assert!(readme.contains("host the exported prepared WIM from C:\\BitOSDT\\WDS"));

        let manifest = fs::read_to_string(export_dir.join("manifest.json")).expect("read manifest");
        assert!(manifest.contains("\"expected_payload_size_bytes\": 7"));
        assert!(manifest.contains("\"expected_payload_sha256\": \"abc123\""));
        assert!(manifest.contains("\"expected_payload_file_name\": \"install.wim\""));
        assert!(manifest.contains("\"runtime_source_kind\": \"UNC\""));
        assert!(manifest.contains("\"runtime_unc_auth_configured\": true"));
        assert!(manifest.contains(r"\\server\\share\\install.wim"));
        assert!(manifest.contains("\"sign_in_readiness\""));
        assert!(manifest.contains("boot.wim"));
        assert!(manifest.contains("install.wim"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_lightweight_publish_path_uses_simple_workspace_default() {
        let request = make_request("LightweightISO");
        let path = resolve_lightweight_publish_path(&request, DeliveryMode::Simple)
            .expect("resolve publish path");
        assert!(path.ends_with("pxe-simple"));
    }

    #[test]
    fn resolve_winpe_packages_prefers_resource_path() {
        let root = make_test_root("resource");
        let manifest_dir = root.join("project").join("cargo").join("src-tauri");
        let resource_dir = root.join("resources");
        let resource_packages = resource_dir.join("Packages");
        let dev_packages = root
            .join("project")
            .join("WinPE-Dependencies")
            .join("Packages");

        fs::create_dir_all(&manifest_dir).expect("create manifest dir");
        write_dummy_chrome(&resource_packages);
        write_dummy_chrome(&dev_packages);

        let resolved =
            resolve_winpe_packages_dir_from_candidates(Some(&resource_dir), &manifest_dir)
                .expect("resolve packages");
        let resolved_canonical = fs::canonicalize(&resolved).expect("canonicalize resolved");
        let expected_canonical =
            fs::canonicalize(&resource_packages).expect("canonicalize expected");
        assert_eq!(resolved_canonical, expected_canonical);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_winpe_packages_supports_up_resource_layout() {
        let root = make_test_root("resource-up");
        let manifest_dir = root.join("project").join("cargo").join("src-tauri");
        let resource_dir = root.join("resources");
        let resource_packages = resource_dir
            .join("_up_")
            .join("_up_")
            .join("WinPE-Dependencies")
            .join("Packages");

        fs::create_dir_all(&manifest_dir).expect("create manifest dir");
        write_dummy_chrome(&resource_packages);

        let resolved =
            resolve_winpe_packages_dir_from_candidates(Some(&resource_dir), &manifest_dir)
                .expect("resolve packages");
        let resolved_canonical = fs::canonicalize(&resolved).expect("canonicalize resolved");
        let expected_canonical =
            fs::canonicalize(&resource_packages).expect("canonicalize expected");
        assert_eq!(resolved_canonical, expected_canonical);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_winpe_packages_falls_back_to_dev_path() {
        let root = make_test_root("dev-fallback");
        let manifest_dir = root.join("project").join("cargo").join("src-tauri");
        let dev_packages = root
            .join("project")
            .join("WinPE-Dependencies")
            .join("Packages");
        fs::create_dir_all(&manifest_dir).expect("create manifest dir");
        write_dummy_chrome(&dev_packages);

        let resolved =
            resolve_winpe_packages_dir_from_candidates(None, &manifest_dir).expect("resolve dev");
        let resolved_canonical = fs::canonicalize(&resolved).expect("canonicalize resolved");
        let expected_canonical = fs::canonicalize(&dev_packages).expect("canonicalize expected");
        assert_eq!(resolved_canonical, expected_canonical);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_winpe_packages_reports_missing_directory() {
        let root = make_test_root("missing");
        let manifest_dir = root.join("project").join("cargo").join("src-tauri");
        fs::create_dir_all(&manifest_dir).expect("create manifest dir");

        let err = resolve_winpe_packages_dir_from_candidates(None, &manifest_dir)
            .expect_err("missing packages should return an error");
        assert!(err.contains("Required WinPE packages directory was not found"));
        assert!(err.contains("WinPE-Dependencies/Packages"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_ui_dir_prefers_bundled_ui_directory() {
        let root = make_test_root("ui-resource");
        let manifest_dir = root.join("project").join("cargo").join("src-tauri");
        let resource_dir = root.join("resources");
        let resource_ui = resource_dir.join("UI");
        let dev_dist = root.join("project").join("cargo").join("dist");

        fs::create_dir_all(&manifest_dir).expect("create manifest dir");
        write_dummy_ui(&resource_ui);
        write_dummy_ui(&dev_dist);

        let resolved =
            resolve_ui_dir_from_candidates(Some(&resource_dir), &manifest_dir).expect("resolve ui");
        let resolved_canonical = fs::canonicalize(&resolved).expect("canonicalize resolved");
        let expected_canonical = fs::canonicalize(&resource_ui).expect("canonicalize expected");
        assert_eq!(resolved_canonical, expected_canonical);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_ui_dir_supports_up_resource_layout() {
        let root = make_test_root("ui-resource-up");
        let manifest_dir = root.join("project").join("cargo").join("src-tauri");
        let resource_dir = root.join("resources");
        let up_dist = resource_dir.join("_up_").join("_up_").join("dist");

        fs::create_dir_all(&manifest_dir).expect("create manifest dir");
        write_dummy_ui(&up_dist);

        let resolved =
            resolve_ui_dir_from_candidates(Some(&resource_dir), &manifest_dir).expect("resolve ui");
        let resolved_canonical = fs::canonicalize(&resolved).expect("canonicalize resolved");
        let expected_canonical = fs::canonicalize(&up_dist).expect("canonicalize expected");
        assert_eq!(resolved_canonical, expected_canonical);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_ui_dir_returns_none_when_missing() {
        let root = make_test_root("ui-missing");
        let manifest_dir = root.join("project").join("cargo").join("src-tauri");
        fs::create_dir_all(&manifest_dir).expect("create manifest dir");

        let resolved = resolve_ui_dir_from_candidates(None, &manifest_dir);
        assert!(resolved.is_none());

        let _ = fs::remove_dir_all(root);
    }
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() {
    match maybe_run_runtime_driver_cli() {
        Ok(true) => return,
        Ok(false) => {}
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    }

    let build_process_state = BuildProcessState::new();
    let hooks_state = build_process_state.0.clone();
    let _ = set_build_runtime_hooks(BuildRuntimeHooks {
        is_cancelled: Arc::new(move || {
            hooks_state
                .lock()
                .map(|registry| registry.cancel_requested)
                .unwrap_or(false)
        }),
        on_started: {
            let state = build_process_state.clone();
            Arc::new(move |process| state.register_process(process))
        },
        on_exited: {
            let state = build_process_state.clone();
            Arc::new(move |pid| state.unregister_process(pid))
        },
    });

    tauri::Builder::default()
        .setup(|app| {
            bitosdt::core::config::Config::ensure_app_dir_exists()
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            if let Some(window) = app.get_window("main") {
                apply_winpe_window_mode(&window);

                #[cfg(debug_assertions)]
                {
                    window.open_devtools();
                }
            }
            Ok(())
        })
        .manage(DownloadCancelFlag::new())
        .manage(build_process_state)
        .manage(LightweightHostState::new())
        .invoke_handler(tauri::generate_handler![
            get_app_version,
            get_app_release_metadata,
            get_system_info,
            check_for_app_update,
            cancel_download,
            cancel_build,
            check_build_workspace_recovery,
            recover_build_workspace,
            get_simple_delivery_defaults,
            get_lightweight_host_status,
            start_lightweight_host,
            stop_lightweight_host,
            start_esd_download,
            build_image,
            generate_unattend_xml,
            generate_autopilot_json,
            generate_app_install_script,
            generate_winget_script,
            generate_windows_update_script,
            generate_domain_join_script,
            generate_user_creation_script,
            generate_osdcloud_startnet,
            generate_bitosdt_startnet,
            generate_network_startnet,
            create_iso,
            get_os_versions,
            sync_os_catalog,
            get_last_catalog_sync,
            list_images,
            get_image_edit_payload,
            delete_image,
            duplicate_image,
            list_usb_targets,
            write_iso_to_usb,
            write_provisioning_bundle_to_usb,
            get_feature_status,
            create_oobe_profile,
            list_oobe_profiles,
            get_oobe_profile,
            rename_oobe_profile,
            duplicate_oobe_profile,
            delete_oobe_profile,
            export_oobe_profile_zip,
            import_oobe_profile_zip,
            preflight_oobe_profile,
            generate_oobe_ppkg,
            get_ppkg_capability_status,
            show_save_dialog,
            show_save_dialog_with_filters,
            show_open_dialog,
            show_folder_dialog,
            get_policy_editor_bootstrap,
            save_policy_preset,
            delete_policy_preset,
            get_settings,
            get_credential_warning_suppressed,
            set_credential_warning_suppressed,
            get_cache_clear_summary,
            clear_download_cache,
            set_setting,
            sync_driver_catalog,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
