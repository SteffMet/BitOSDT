use bitosdt::build::{generate_provisioning_hta, generate_provisioning_kiosk_helper_ps1};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const OOBE_ROOT: &str = r"C:\BitOSDT\Provisioning";
pub const LEGACY_OOBE_ROOT: &str = r"C:\BitOSDT\AutoUnattend";
const PROFILE_MANIFEST_FILE: &str = ".bitosdt-oobe.json";
const AUTOUNATTEND_FILE: &str = "Autounattend.xml";
const DEPLOYMENT_README_FILE: &str = "DEPLOYMENT-README.txt";
const DEFAULT_LANGUAGE: &str = "en-US";
const DEFAULT_INPUT_LOCALE: &str = "0409:00000409";
const DEFAULT_TIMEZONE: &str = "Pacific Standard Time";
const OOBE_MANIFEST_SCHEMA_VERSION: u32 = 5;
const USB_ORCHESTRATOR_SCRIPT: &str = "Start-BitOSDTUsbOrchestrator.ps1";
const USB_RUNONCE_NAME: &str = "BitOSDTUsbOobe";
const USB_BOOTSTRAP_ADMIN_USERNAME: &str = "Administrator";
const PROVISIONING_ORCHESTRATOR_SCRIPT: &str = "Start-BitOSDTOrchestrator.ps1";
const PROVISIONING_UI_HTA_FILE: &str = "Start-BitOSDTProvisioningUi.hta";
const PROVISIONING_UI_PROFILE_FILE: &str = "ProvisioningUiProfile.json";
const PROVISIONING_KIOSK_HELPER_FILE: &str = "Apply-Kiosk.ps1";
const PROVISIONING_WIFI_SCRIPT: &str = "wifi-connect.ps1";
const PROVISIONING_BITLOCKER_SCRIPT: &str = "disable-bitlocker.ps1";
const PROVISIONING_RUNONCE_NAME: &str = "BitOSDTProvisioning";
const PROVISIONING_SCHEDULED_TASK_NAME: &str = "BitOSDTProvisioningUi";
const BITOSDT_RUNTIME_ROOT: &str = r"C:\BitOSDT";
const BITOSDT_RUNTIME_SCRIPTS_DIR: &str = r"C:\BitOSDT\Scripts";
const BITOSDT_RUNTIME_APPS_DIR: &str = r"C:\BitOSDT\Apps";
const BITOSDT_RUNTIME_FILES_DIR: &str = r"C:\BitOSDT\Files";
const BITOSDT_PROVISIONING_UI_STATE_DIR: &str = r"C:\ProgramData\BitOSDT\ProvisioningUi";
const BITOSDT_PROVISIONING_UI_PROFILE_PATH: &str =
    r"C:\ProgramData\BitOSDT\ProvisioningUi\profile.json";
const BITOSDT_PROVISIONING_UI_STATE_PATH: &str =
    r"C:\ProgramData\BitOSDT\ProvisioningUi\ui-state.json";
const BITOSDT_PROVISIONING_UI_STATUS_PATH: &str =
    r"C:\ProgramData\BitOSDT\ProvisioningUi\task-status.json";
const BITOSDT_PROVISIONING_UI_COMMAND_PATH: &str =
    r"C:\ProgramData\BitOSDT\ProvisioningUi\command.json";
const BITOSDT_PROVISIONING_UI_APP_PROGRESS_PATH: &str =
    r"C:\ProgramData\BitOSDT\ProvisioningUi\app-progress.json";
const BITOSDT_PROVISIONING_UI_HEARTBEAT_PATH: &str =
    r"C:\ProgramData\BitOSDT\ProvisioningUi\ui-heartbeat.json";
const BITOSDT_PROVISIONING_UI_SESSION_LOG_PATH: &str = r"C:\BitOSDT\Logs\provisioning-ui.log";
const BITOSDT_PROVISIONING_UI_SHELL_LOG_PATH: &str = r"C:\BitOSDT\Logs\provisioning-shell.log";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeProfileRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default = "default_trigger_mode")]
    pub trigger_mode: TriggerMode,
    #[serde(default)]
    pub oobe_config: OobeUiConfig,
    #[serde(default)]
    pub domain_join: DomainJoinUiConfig,
    #[serde(default)]
    pub domain_join_mode: DomainJoinMode,
    #[serde(default)]
    pub prompt_for_computer_name: bool,
    #[serde(default)]
    pub default_user: DefaultUserUiConfig,
    #[serde(default)]
    pub wifi: OobeWifiConfig,
    #[serde(default)]
    pub apps: OobeAppsConfig,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_input_locale")]
    pub input_locale: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub enable_debloat: bool,
    #[serde(default)]
    pub debloat_script_content: String,
}

impl Default for OobeProfileRequest {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            overwrite: false,
            trigger_mode: default_trigger_mode(),
            oobe_config: OobeUiConfig::default(),
            domain_join: DomainJoinUiConfig::default(),
            domain_join_mode: DomainJoinMode::default(),
            prompt_for_computer_name: false,
            default_user: DefaultUserUiConfig::default(),
            wifi: OobeWifiConfig::default(),
            apps: OobeAppsConfig::default(),
            language: default_language(),
            input_locale: default_input_locale(),
            timezone: default_timezone(),
            enable_debloat: false,
            debloat_script_content: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeProfileSummary {
    pub name: String,
    pub description: String,
    pub path: String,
    pub updated_at: String,
    pub has_manifest: bool,
    #[serde(default)]
    pub preflight_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeProfilePreflight {
    pub profile_name: String,
    pub profile_path: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeProfileDetail {
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
    pub request: OobeProfileRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OobeProfileManifest {
    schema_version: u32,
    name: String,
    description: String,
    created_at: String,
    updated_at: String,
    request: OobeProfileRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeUiConfig {
    #[serde(default = "default_true")]
    pub skip_machine_oobe: bool,
    #[serde(default = "default_true")]
    pub skip_user_oobe: bool,
    #[serde(default = "default_true")]
    pub hide_eula: bool,
    #[serde(default = "default_true")]
    pub hide_privacy_settings: bool,
    #[serde(default = "default_true")]
    pub hide_wireless_setup: bool,
    #[serde(default)]
    pub hide_local_account_screen: bool,
    #[serde(default = "default_true")]
    pub hide_online_account_screens: bool,
    #[serde(default = "default_network_location")]
    pub network_location: String,
    #[serde(default = "default_protect_your_pc")]
    pub protect_your_pc: String,
    #[serde(default)]
    pub computer_name: Option<String>,
}

impl Default for OobeUiConfig {
    fn default() -> Self {
        Self {
            skip_machine_oobe: true,
            skip_user_oobe: true,
            hide_eula: true,
            hide_privacy_settings: true,
            hide_wireless_setup: true,
            hide_local_account_screen: false,
            hide_online_account_screens: true,
            network_location: default_network_location(),
            protect_your_pc: default_protect_your_pc(),
            computer_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainJoinUiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub ou_path: Option<String>,
}

impl Default for DomainJoinUiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            domain: String::new(),
            username: String::new(),
            password: String::new(),
            ou_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum DomainJoinMode {
    #[default]
    SpecializeXml,
    PostRenameScript,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum TriggerMode {
    SetupUnattend,
    #[default]
    FirstLogonUsbScan,
    ProvisioningPackage,
}

fn default_trigger_mode() -> TriggerMode {
    TriggerMode::FirstLogonUsbScan
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultUserUiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_user_group")]
    pub group: String,
}

impl Default for DefaultUserUiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            username: String::new(),
            password: String::new(),
            group: default_user_group(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeWifiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub ssid: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_wifi_authentication")]
    pub authentication: String,
    #[serde(default = "default_wifi_encryption")]
    pub encryption: String,
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    #[serde(default)]
    pub hidden_network: bool,
    #[serde(default)]
    pub dns_server_1: String,
    #[serde(default)]
    pub dns_server_2: String,
}

impl Default for OobeWifiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ssid: String::new(),
            password: String::new(),
            authentication: default_wifi_authentication(),
            encryption: default_wifi_encryption(),
            auto_connect: true,
            hidden_network: false,
            dns_server_1: String::new(),
            dns_server_2: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeWingetPackage {
    pub package_id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub custom_args: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeChocolateyPackage {
    pub package_name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub custom_args: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OobeLocalPayloadItem {
    pub source_path: String,
    #[serde(default = "default_payload_kind")]
    pub source_kind: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeCustomInstaller {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_file_name: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<OobeLocalPayloadItem>,
    #[serde(default)]
    pub dependency_destination: Option<String>,
    #[serde(default)]
    pub silent_args: String,
    #[serde(default = "default_installer_type")]
    pub installer_type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeCustomScript {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OobeAppsConfig {
    #[serde(default)]
    pub winget_packages: Vec<OobeWingetPackage>,
    #[serde(default)]
    pub chocolatey_packages: Vec<OobeChocolateyPackage>,
    #[serde(default)]
    pub custom_installers: Vec<OobeCustomInstaller>,
    #[serde(default)]
    pub copied_items: Vec<OobeLocalPayloadItem>,
    #[serde(default)]
    pub copy_destination: Option<String>,
    #[serde(default)]
    pub disable_bitlocker: bool,
    #[serde(default)]
    pub reboot_after_disable_bitlocker: bool,
    #[serde(default = "default_true")]
    pub auto_install_chocolatey: bool,
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
    #[serde(default)]
    pub enable_custom_scripts: bool,
    #[serde(default)]
    pub custom_scripts: Vec<OobeCustomScript>,
}

impl Default for OobeAppsConfig {
    fn default() -> Self {
        Self {
            winget_packages: Vec::new(),
            chocolatey_packages: Vec::new(),
            custom_installers: Vec::new(),
            copied_items: Vec::new(),
            copy_destination: None,
            disable_bitlocker: false,
            reboot_after_disable_bitlocker: false,
            auto_install_chocolatey: true,
            continue_on_error: true,
            enable_custom_scripts: false,
            custom_scripts: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_network_location() -> String {
    "Work".to_string()
}

fn default_protect_your_pc() -> String {
    "Recommended".to_string()
}

fn default_user_group() -> String {
    "Administrators".to_string()
}

fn default_wifi_authentication() -> String {
    "Wpa2Psk".to_string()
}

fn default_wifi_encryption() -> String {
    "Aes".to_string()
}

fn default_installer_type() -> String {
    "Exe".to_string()
}

fn default_payload_kind() -> String {
    "File".to_string()
}

fn default_language() -> String {
    DEFAULT_LANGUAGE.to_string()
}

fn default_input_locale() -> String {
    DEFAULT_INPUT_LOCALE.to_string()
}

fn default_timezone() -> String {
    DEFAULT_TIMEZONE.to_string()
}

fn resolve_oobe_locale_settings(request: &OobeProfileRequest) -> Result<(String, String), String> {
    let requested_language = if request.language.trim().is_empty() {
        DEFAULT_LANGUAGE
    } else {
        request.language.trim()
    };

    let (language, derived_input_locale) =
        bitosdt::config::resolve_unattend_locale_settings(requested_language)
            .map_err(|e| format!("Invalid OOBE language: {}", e))?;

    let requested_input_locale = request.input_locale.trim();
    if requested_input_locale.is_empty() {
        return Ok((language, derived_input_locale));
    }

    if requested_input_locale.contains(':') {
        if requested_input_locale != derived_input_locale {
            return Err(format!(
                "Unsupported locale combination: language '{}' cannot be paired with input locale '{}'. Use '{}' or leave input locale blank.",
                language, requested_input_locale, derived_input_locale
            ));
        }
        return Ok((language, requested_input_locale.to_string()));
    }

    let normalized_input_locale = bitosdt::config::normalize_language_tag(requested_input_locale)
        .map_err(|_| {
            format!(
                "Invalid input locale '{}'. Use BCP-47 (for example 'fr-FR') or keyboard ID format like '0409:00000409'.",
                requested_input_locale
            )
        })?;

    if normalized_input_locale != language {
        return Err(format!(
            "Unsupported locale combination: language '{}' cannot be paired with input locale '{}'. Use '{}' or leave input locale blank.",
            language, normalized_input_locale, derived_input_locale
        ));
    }

    Ok((language, normalized_input_locale))
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

fn map_local_payload_kind(value: &str) -> bitosdt::tasks::LocalPayloadKind {
    match value {
        "Directory" => bitosdt::tasks::LocalPayloadKind::Directory,
        _ => bitosdt::tasks::LocalPayloadKind::File,
    }
}

fn map_wifi_authentication(value: &str) -> bitosdt::config::WifiAuthentication {
    match value {
        "Open" => bitosdt::config::WifiAuthentication::Open,
        "Wpa3Sae" => bitosdt::config::WifiAuthentication::Wpa3Sae,
        _ => bitosdt::config::WifiAuthentication::Wpa2Psk,
    }
}

fn map_wifi_encryption(value: &str) -> bitosdt::config::WifiEncryption {
    match value {
        "None" => bitosdt::config::WifiEncryption::None,
        "Tkip" => bitosdt::config::WifiEncryption::Tkip,
        _ => bitosdt::config::WifiEncryption::Aes,
    }
}

fn sanitize_profile_name(name: &str) -> String {
    let trimmed = name.trim();
    let mut sanitized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ' ' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }

    let collapsed = sanitized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    collapsed
        .trim_matches(|ch: char| ch == '.' || ch == '_' || ch.is_whitespace())
        .trim()
        .to_string()
}

fn is_staging_profile_name(name: &str) -> bool {
    name.trim().starts_with(".tmp-")
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

fn should_prompt_for_computer_name(request: &OobeProfileRequest) -> bool {
    validate_computer_name(request.oobe_config.computer_name.as_deref())
        .map(|value| value.is_none())
        .unwrap_or(true)
}

fn provisioning_native_computer_name(request: &OobeProfileRequest) -> Option<String> {
    validate_computer_name(request.oobe_config.computer_name.as_deref())
        .ok()
        .flatten()
}

fn provisioning_has_wifi_dns_overrides(request: &OobeProfileRequest) -> bool {
    !request.wifi.dns_server_1.trim().is_empty() || !request.wifi.dns_server_2.trim().is_empty()
}

fn provisioning_native_wifi_supported(request: &OobeProfileRequest) -> bool {
    request.wifi.enabled
        && !request.wifi.hidden_network
        && !provisioning_has_wifi_dns_overrides(request)
        && matches!(request.wifi.authentication.as_str(), "Open" | "Wpa2Psk")
}

fn provisioning_post_signin_wifi_required(request: &OobeProfileRequest) -> bool {
    request.wifi.enabled && !provisioning_native_wifi_supported(request)
}

fn provisioning_native_domain_join_supported(request: &OobeProfileRequest) -> bool {
    request.domain_join.enabled
        && request.domain_join_mode == DomainJoinMode::SpecializeXml
        && provisioning_native_computer_name(request).is_some()
        && request
            .domain_join
            .ou_path
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
}

fn provisioning_post_signin_domain_join_required(request: &OobeProfileRequest) -> bool {
    request.domain_join.enabled && !provisioning_native_domain_join_supported(request)
}

fn provisioning_post_signin_computer_name_required(request: &OobeProfileRequest) -> bool {
    should_prompt_for_computer_name(request)
}

fn ensure_domain_inputs(domain_join: &DomainJoinUiConfig) -> Result<(), String> {
    if !domain_join.enabled {
        return Ok(());
    }

    if domain_join.domain.trim().is_empty()
        || domain_join.username.trim().is_empty()
        || domain_join.password.trim().is_empty()
    {
        return Err("Domain Join is enabled but required fields are missing.".to_string());
    }

    Ok(())
}

fn ensure_default_user_inputs(user: &DefaultUserUiConfig) -> Result<(), String> {
    if !user.enabled {
        return Ok(());
    }

    if user.username.trim().is_empty() || user.password.trim().is_empty() {
        return Err("Default user is enabled but username or password is missing.".to_string());
    }

    Ok(())
}

fn ensure_wifi_inputs(wifi: &OobeWifiConfig) -> Result<(), String> {
    if !wifi.enabled {
        return Ok(());
    }

    if wifi.ssid.trim().is_empty() {
        return Err("Wi-Fi is enabled but SSID is missing.".to_string());
    }

    if wifi.authentication != "Open" {
        let password_len = wifi.password.chars().count();
        if password_len < 8 || password_len > 63 {
            return Err(
                "Wi-Fi password must be between 8 and 63 characters for secured networks."
                    .to_string(),
            );
        }
    }

    ensure_dns_server_value(&wifi.dns_server_1, "Primary Wi-Fi DNS")?;
    ensure_dns_server_value(&wifi.dns_server_2, "Secondary Wi-Fi DNS")?;

    Ok(())
}

fn ensure_dns_server_value(value: &str, label: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    trimmed
        .parse::<IpAddr>()
        .map(|_| ())
        .map_err(|_| format!("{label} must be a valid IPv4 or IPv6 address."))
}

fn ensure_usb_first_logon_requirements(request: &OobeProfileRequest) -> Result<(), String> {
    if request.trigger_mode != TriggerMode::FirstLogonUsbScan {
        return Ok(());
    }

    if !request.default_user.enabled {
        return Err(
            "USB media mode requires a default local administrator account for recovery sign-in."
                .to_string(),
        );
    }

    if request.default_user.group != "Administrators" {
        return Err(
            "USB media mode requires the default local user to be in the Administrators group."
                .to_string(),
        );
    }

    Ok(())
}

fn oobe_root_path() -> PathBuf {
    PathBuf::from(OOBE_ROOT)
}

fn legacy_oobe_root_path() -> PathBuf {
    PathBuf::from(LEGACY_OOBE_ROOT)
}

fn profile_dir(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

fn find_profile_dir_case_insensitive(root: &Path, name: &str) -> Option<PathBuf> {
    let mut case_insensitive_match = None;
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name == name {
            return Some(path);
        }
        if case_insensitive_match.is_none() && file_name.eq_ignore_ascii_case(name) {
            case_insensitive_match = Some(path);
        }
    }

    case_insensitive_match.or_else(|| {
        let exact = profile_dir(root, name);
        exact.exists().then_some(exact)
    })
}

fn resolve_profile_path_with_roots(
    canonical_root: &Path,
    legacy_root: Option<&Path>,
    profile_name: &str,
) -> Option<PathBuf> {
    find_profile_dir_case_insensitive(canonical_root, profile_name).or_else(|| {
        legacy_root.and_then(|root| find_profile_dir_case_insensitive(root, profile_name))
    })
}

pub(crate) fn resolve_oobe_profile_path(profile_name: &str) -> Option<PathBuf> {
    let sanitized_name = sanitize_profile_name(profile_name);
    if sanitized_name.is_empty() {
        return None;
    }

    resolve_profile_path_with_roots(
        &oobe_root_path(),
        Some(&legacy_oobe_root_path()),
        &sanitized_name,
    )
}

fn profile_exists_with_roots(
    canonical_root: &Path,
    legacy_root: Option<&Path>,
    profile_name: &str,
) -> bool {
    resolve_profile_path_with_roots(canonical_root, legacy_root, profile_name).is_some()
}

fn normalize_manifest(manifest: OobeProfileManifest) -> OobeProfileManifest {
    manifest
}

fn read_manifest(path: &Path) -> Result<OobeProfileManifest, String> {
    let manifest_path = path.join(PROFILE_MANIFEST_FILE);
    let bytes = fs::read(&manifest_path).map_err(|e| {
        format!(
            "Failed to read profile manifest {}: {}",
            manifest_path.display(),
            e
        )
    })?;
    let manifest = serde_json::from_slice::<OobeProfileManifest>(&bytes).map_err(|e| {
        format!(
            "Failed to parse profile manifest {}: {}",
            manifest_path.display(),
            e
        )
    })?;

    let normalized = normalize_manifest(manifest);
    Ok(normalized)
}

fn write_manifest(path: &Path, manifest: &OobeProfileManifest) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(manifest)
        .map_err(|e| format!("Failed to serialize profile manifest: {}", e))?;
    fs::write(path.join(PROFILE_MANIFEST_FILE), content)
        .map_err(|e| format!("Failed to write profile manifest: {}", e))
}

fn copy_payload_directory_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        return Err(format!("Source path does not exist: {}", source.display()));
    }

    fs::create_dir_all(destination).map_err(|e| {
        format!(
            "Failed to create destination directory {}: {}",
            destination.display(),
            e
        )
    })?;

    for entry in
        fs::read_dir(source).map_err(|e| format!("Failed to list {}: {}", source.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to inspect source entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = destination.join(entry.file_name());

        if src_path.is_dir() {
            copy_directory_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

fn ensure_deployment_readme(profile_dir: &Path, profile_name: &str) -> Result<(), String> {
    let readme_path = profile_dir.join(DEPLOYMENT_README_FILE);
    if readme_path.is_file() {
        return Ok(());
    }

    fs::write(&readme_path, build_deployment_readme(profile_name))
        .map_err(|e| format!("Failed to write {}: {}", readme_path.display(), e))
}

fn build_deployment_readme(profile_name: &str) -> String {
    [
        format!("BitOSDT Deployment Package: {}", profile_name),
        "".to_string(),
        "Preferred USB structure:".to_string(),
        r"  <USB Root>\Provisioning\<ProfileName>\Autounattend.xml".to_string(),
        r"  <USB Root>\Provisioning\<ProfileName>\Scripts\*.ps1".to_string(),
        r"  <USB Root>\Provisioning\<ProfileName>\Apps\* (optional)".to_string(),
        r"  <USB Root>\Provisioning\<ProfileName>\Files\* (optional)".to_string(),
        "".to_string(),
        "Legacy USB structure still supported:".to_string(),
        r"  <USB Root>\AutoUnattend\<ProfileName>\Autounattend.xml".to_string(),
        r"  <USB Root>\AutoUnattend\<ProfileName>\Scripts\*.ps1".to_string(),
        r"  <USB Root>\AutoUnattend\<ProfileName>\Apps\* (optional)".to_string(),
        r"  <USB Root>\AutoUnattend\<ProfileName>\Files\* (optional)".to_string(),
        "".to_string(),
        "Trigger timing:".to_string(),
        "  1) During the first temporary Administrator sign-in, BitOSDT runs a single USB bootstrap command.".to_string(),
        r"  2) BitOSDT scans removable drives for Provisioning\<ProfileName> first, then AutoUnattend\<ProfileName> as a legacy fallback.".to_string(),
        r"  3) Start-BitOSDTUsbOrchestrator.ps1 resumes locally from C:\BitOSDT\Scripts across reboots until completion.".to_string(),
        "".to_string(),
        "Operator checklist:".to_string(),
        "  - Keep profile folder name exactly equal to <ProfileName> in the UI.".to_string(),
        "  - Verify Autounattend.xml and Scripts folder exist before deployment.".to_string(),
        "  - Keep the USB inserted until BitOSDT has completed its first sign-in bootstrap.".to_string(),
    ]
    .join("\r\n")
}

fn expected_script_files_for_request(request: &OobeProfileRequest) -> Vec<String> {
    let mut required = Vec::new();
    let uses_first_logon_scripts = request.trigger_mode == TriggerMode::FirstLogonUsbScan;
    let uses_provisioning_scripts = request.trigger_mode == TriggerMode::ProvisioningPackage;
    let uses_script_payloads = uses_first_logon_scripts || uses_provisioning_scripts;

    if uses_first_logon_scripts {
        required.push(USB_ORCHESTRATOR_SCRIPT.to_string());
    }

    if uses_script_payloads && has_app_work(&request.apps) {
        required.push("installapps.ps1".to_string());
    }
    if uses_provisioning_scripts && request.apps.disable_bitlocker {
        required.push(PROVISIONING_BITLOCKER_SCRIPT.to_string());
    }
    if uses_script_payloads && request.enable_debloat {
        required.push("debloat.ps1".to_string());
    }
    if uses_provisioning_scripts {
        required.push(PROVISIONING_ORCHESTRATOR_SCRIPT.to_string());
    }
    if (uses_first_logon_scripts && request.wifi.enabled)
        || (uses_provisioning_scripts && provisioning_post_signin_wifi_required(request))
    {
        required.push(PROVISIONING_WIFI_SCRIPT.to_string());
    }
    if (uses_first_logon_scripts && request.domain_join_mode == DomainJoinMode::PostRenameScript)
        || (uses_provisioning_scripts && provisioning_post_signin_domain_join_required(request))
    {
        if request.domain_join.enabled {
            required.push("domainjoin.ps1".to_string());
        }
    }

    if uses_script_payloads && request.apps.enable_custom_scripts {
        for (index, script) in request.apps.custom_scripts.iter().enumerate() {
            if !script.enabled || script.content.trim().is_empty() {
                continue;
            }
            required.push(build_custom_script_filename(index, &script.name));
        }
    }

    required.sort();
    required.dedup();
    required
}

fn has_legacy_usb_first_logon_commands(xml: &str) -> bool {
    let legacy_markers = [
        "Stage AutoUnattend payload",
        "Prompt for computer name",
        "Join domain",
        "Install applications",
        "Run debloat script",
    ];
    legacy_markers
        .iter()
        .filter(|marker| xml.contains(**marker))
        .count()
        > 1
}

fn preflight_profile_with_root(
    root: &Path,
    profile_name: &str,
) -> Result<OobeProfilePreflight, String> {
    preflight_profile_with_roots(root, None, profile_name)
}

fn preflight_profile_with_roots(
    canonical_root: &Path,
    legacy_root: Option<&Path>,
    profile_name: &str,
) -> Result<OobeProfilePreflight, String> {
    let sanitized_name = sanitize_profile_name(profile_name);
    if sanitized_name.is_empty() {
        return Err("Profile name is required.".to_string());
    }

    let requested_path = profile_dir(canonical_root, &sanitized_name);
    let mut warnings = Vec::new();

    if !profile_exists_with_roots(canonical_root, legacy_root, &sanitized_name) {
        warnings.push(format!(
            "Missing profile directory: {}. Create Provisioning\\{} on the deployment media (legacy AutoUnattend is still supported).",
            requested_path.display(),
            sanitized_name
        ));
    }

    let actual_path = resolve_profile_path_with_roots(canonical_root, legacy_root, &sanitized_name)
        .unwrap_or_else(|| requested_path.clone());
    let mut requested_roots = vec![canonical_root.to_path_buf()];
    if let Some(legacy_root) = legacy_root {
        requested_roots.push(legacy_root.to_path_buf());
    }

    if actual_path != requested_path {
        let actual_folder_name = actual_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if actual_folder_name != sanitized_name
            && actual_folder_name.eq_ignore_ascii_case(&sanitized_name)
        {
            warnings.push(format!(
                "Profile folder case mismatch: expected '{}', found '{}'. Rename the folder to exactly match the profile name.",
                sanitized_name, actual_folder_name
            ));
        }
    }

    if !actual_path.exists() {
        return Ok(OobeProfilePreflight {
            profile_name: sanitized_name,
            profile_path: requested_path.to_string_lossy().to_string(),
            warnings,
        });
    }

    if actual_path != requested_path
        && requested_roots
            .iter()
            .all(|root| actual_path != profile_dir(root, &sanitized_name))
    {
        warnings.push(format!(
            "Profile lookup mismatch: expected path '{}' but found '{}'.",
            requested_path.display(),
            actual_path.display()
        ));
    }

    let manifest = read_manifest(&actual_path).ok();
    if let Some(m) = &manifest {
        if sanitize_profile_name(&m.name) != sanitized_name {
            warnings.push(format!(
                "Manifest/profile mismatch: manifest name '{}' does not match folder '{}'. Re-save or rename profile.",
                m.name, sanitized_name
            ));
        }
        if sanitize_profile_name(&m.request.name) != sanitized_name {
            warnings.push(format!(
                "Request/profile mismatch: request.name '{}' does not match folder '{}'. Re-save profile from the editor.",
                m.request.name, sanitized_name
            ));
        }
    }

    let trigger_mode = manifest
        .as_ref()
        .map(|m| m.request.trigger_mode.clone())
        .unwrap_or(TriggerMode::FirstLogonUsbScan);

    if trigger_mode == TriggerMode::ProvisioningPackage {
        let provisioning_bootstrap = actual_path.join("Apply-BitOSDTProvisioning.ps1");
        if !provisioning_bootstrap.is_file() {
            warnings.push(format!(
                "Missing required file: {}. Re-generate the profile to recreate provisioning bootstrap script.",
                provisioning_bootstrap.display()
            ));
        }
    } else {
        let autounattend = actual_path.join(AUTOUNATTEND_FILE);
        if !autounattend.is_file() {
            warnings.push(format!(
                "Missing required file: {}. Re-generate the profile to recreate {}.",
                autounattend.display(),
                AUTOUNATTEND_FILE
            ));
        }
    }

    let scripts_dir = actual_path.join("Scripts");
    let expected_scripts = manifest
        .as_ref()
        .map(|m| expected_script_files_for_request(&m.request))
        .unwrap_or_default();
    if !expected_scripts.is_empty() && !scripts_dir.is_dir() {
        warnings.push(format!(
            "Missing required directory: {}. Re-generate the profile so scripts are staged correctly.",
            scripts_dir.display()
        ));
    }

    if scripts_dir.is_dir() {
        for script_name in expected_scripts {
            let script_path = scripts_dir.join(&script_name);
            if !script_path.is_file() {
                warnings.push(format!(
                    "Missing required script: {}. Re-generate the profile or restore this script.",
                    script_path.display()
                ));
            }
        }
    }

    let has_inline_ppkg_export = fs::read_dir(&actual_path)
        .ok()
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("ppkg"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if trigger_mode != TriggerMode::ProvisioningPackage && has_inline_ppkg_export {
        let provisioning_bootstrap = actual_path.join("Apply-BitOSDTProvisioning.ps1");
        let provisioning_orchestrator = scripts_dir.join(PROVISIONING_ORCHESTRATOR_SCRIPT);
        let provisioning_hta = scripts_dir.join(PROVISIONING_UI_HTA_FILE);
        let provisioning_profile = scripts_dir.join(PROVISIONING_UI_PROFILE_FILE);
        if !provisioning_bootstrap.is_file()
            || !provisioning_orchestrator.is_file()
            || !provisioning_hta.is_file()
            || !provisioning_profile.is_file()
        {
            warnings.push(
                "Existing inline PPKG export is missing refreshed provisioning sidecar assets. Re-generate the profile or export PPKG again to rebuild the provisioning bootstrap, HTA, and sidecar script set."
                    .to_string(),
            );
        }
    }

    if trigger_mode == TriggerMode::FirstLogonUsbScan {
        if manifest
            .as_ref()
            .map(|m| m.schema_version < OOBE_MANIFEST_SCHEMA_VERSION)
            .unwrap_or(true)
        {
            warnings.push(
                "Legacy USB OOBE profile detected. Re-generate this profile so it uses the single-bootstrap USB orchestrator flow."
                    .to_string(),
            );
        }

        let usb_orchestrator = scripts_dir.join(USB_ORCHESTRATOR_SCRIPT);
        if !usb_orchestrator.is_file() {
            warnings.push(format!(
                "Missing required USB orchestrator: {}. Re-generate the profile before deployment.",
                usb_orchestrator.display()
            ));
        }

        let autounattend_path = actual_path.join(AUTOUNATTEND_FILE);
        if let Ok(xml) = fs::read_to_string(&autounattend_path) {
            if has_legacy_usb_first_logon_commands(&xml) {
                warnings.push(
                    "Legacy USB FirstLogonCommands layout detected in Autounattend.xml. Re-generate this profile before deployment."
                        .to_string(),
                );
            }
        }
    }

    Ok(OobeProfilePreflight {
        profile_name: sanitized_name,
        profile_path: actual_path.to_string_lossy().to_string(),
        warnings,
    })
}

pub fn preflight_oobe_profile(name: String) -> Result<OobeProfilePreflight, String> {
    preflight_profile_with_roots(&oobe_root_path(), Some(&legacy_oobe_root_path()), &name)
}

fn generate_bootstrap_password() -> String {
    format!(
        "BitOSDT!{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn make_profile_source_locator(profile_name: &str) -> String {
    let escaped_name = profile_name.replace('\'', "''");
    let template = concat!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -Command ",
        "\"$ErrorActionPreference='Stop';",
        "$payload={",
        "$profileName='__PROFILE_NAME__';",
        "$source=$null;",
        "$rootNames=@('Provisioning','AutoUnattend');",
        "foreach($drive in (Get-PSDrive -PSProvider FileSystem)){",
        "if($drive.Root -like 'C:*'){continue};",
        "foreach($rootName in $rootNames){",
        "$base=Join-Path $drive.Root $rootName;",
        "if(-not (Test-Path $base)){continue};",
        "$candidate=Join-Path $base $profileName;",
        "if(Test-Path $candidate){$source=$candidate;break}",
        "};",
        "if($source){break}",
        "};",
        "if(-not $source){throw ('Provisioning profile source not found for profile ' + $profileName + '. Remediation: preferred USB structure is <USB>\\Provisioning\\' + $profileName + '\\, legacy fallback is <USB>\\AutoUnattend\\' + $profileName + '\\. Ensure the folder name matches exactly and insert the USB before first logon commands run.');};",
        "$scriptsPath=Join-Path $source 'Scripts';",
        "if(-not (Test-Path $scriptsPath)){throw ('Profile source found but Scripts directory is missing: ' + $scriptsPath + '. Remediation: regenerate/export the profile and copy the full Scripts folder.');};",
        "New-Item -Path 'C:\\BitOSDT\\Scripts' -ItemType Directory -Force | Out-Null;",
        "New-Item -Path 'C:\\BitOSDT\\Apps' -ItemType Directory -Force | Out-Null;",
        "New-Item -Path 'C:\\BitOSDT\\Files' -ItemType Directory -Force | Out-Null;",
        "New-Item -Path 'C:\\ProgramData\\BitOSDT' -ItemType Directory -Force | Out-Null;",
        "Copy-Item -Path (Join-Path $scriptsPath '*') -Destination 'C:\\BitOSDT\\Scripts' -Recurse -Force;",
        "if(Test-Path (Join-Path $source 'Apps')){",
        "Copy-Item -Path (Join-Path $source 'Apps\\*') -Destination 'C:\\BitOSDT\\Apps' -Recurse -Force",
        "};",
        "if(Test-Path (Join-Path $source 'Files')){",
        "Copy-Item -Path (Join-Path $source 'Files\\*') -Destination 'C:\\BitOSDT\\Files' -Recurse -Force",
        "};",
        "$orchestratorPath=Join-Path 'C:\\BitOSDT\\Scripts' 'Start-BitOSDTUsbOrchestrator.ps1';",
        "if(-not (Test-Path -LiteralPath $orchestratorPath)){throw ('USB orchestrator script missing after staging: ' + $orchestratorPath + '. Remediation: regenerate the profile and redeploy the full Scripts folder.');};",
        "& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $orchestratorPath;",
        "if($null -ne $LASTEXITCODE){exit $LASTEXITCODE};",
        "};",
        "$isAdmin=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);",
        "if(-not $isAdmin){",
        "$proc=Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-Command',$payload.ToString()) -Verb RunAs -PassThru -Wait;",
        "if($null -eq $proc){exit 1};",
        "exit $proc.ExitCode",
        "};",
        "& $payload\""
    );

    template.replace("__PROFILE_NAME__", &escaped_name)
}

fn escape_ps_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

fn build_post_rename_domain_join_script(
    domain: &DomainJoinUiConfig,
    reboot_after_join: bool,
) -> String {
    let domain_name = escape_ps_single_quoted(domain.domain.trim());
    let username = escape_ps_single_quoted(domain.username.trim());
    let password = escape_ps_single_quoted(domain.password.trim());
    let ou_path = escape_ps_single_quoted(domain.ou_path.clone().unwrap_or_default().trim());
    let log_path = escape_ps_single_quoted(r"C:\BitOSDT\Logs\domainjoin.log");

    let mut lines = vec![
        "$ErrorActionPreference = 'Stop'".to_string(),
        format!("$LogPath = '{}'", log_path),
        format!("$DomainName = '{}'", domain_name),
        format!("$Username = '{}'", username),
        format!("$Password = '{}'", password),
        "function Write-Log {".to_string(),
        "  param([string]$Message, [string]$Level = 'INFO')".to_string(),
        "  $directory = Split-Path -Path $LogPath -Parent".to_string(),
        "  if (-not (Test-Path -LiteralPath $directory)) {".to_string(),
        "    New-Item -Path $directory -ItemType Directory -Force | Out-Null".to_string(),
        "  }".to_string(),
        "  $line = \"$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') [$Level] $Message\"".to_string(),
        "  $line | Out-File -FilePath $LogPath -Encoding utf8 -Append".to_string(),
        "}".to_string(),
        "function Test-DomainResolvable {".to_string(),
        "  param([Parameter(Mandatory = $true)][string]$Name)".to_string(),
        "  try {".to_string(),
        "    [System.Net.Dns]::GetHostAddresses($Name) | Out-Null".to_string(),
        "    return $true".to_string(),
        "  } catch {".to_string(),
        "    return $false".to_string(),
        "  }".to_string(),
        "}".to_string(),
        "$secure = ConvertTo-SecureString $Password -AsPlainText -Force".to_string(),
        "$cred = New-Object System.Management.Automation.PSCredential($Username, $secure)".to_string(),
        "$params = @{ DomainName = $DomainName; Credential = $cred; Force = $true; ErrorAction = 'Stop' }".to_string(),
        "$currentDomain = $null".to_string(),
        "try {".to_string(),
        "  $currentDomain = (Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).Domain".to_string(),
        "} catch {".to_string(),
        "  Write-Log \"Failed to read current computer domain: $($_.Exception.Message)\" 'WARNING'"
            .to_string(),
        "}".to_string(),
        "if (-not [string]::IsNullOrWhiteSpace($currentDomain) -and $currentDomain -ieq $DomainName) {"
            .to_string(),
        "  Write-Log \"Computer is already joined to $DomainName.\" 'SUCCESS'".to_string(),
        "  exit 0".to_string(),
        "}".to_string(),
    ];

    if !ou_path.is_empty() {
        lines.push(format!("$params['OUPath'] = '{}'", ou_path));
    }

    lines.push("for ($attempt = 1; $attempt -le 10; $attempt++) {".to_string());
    lines.push("  if (Test-DomainResolvable -Name $DomainName) {".to_string());
    lines.push("    break".to_string());
    lines.push("  }".to_string());
    lines.push(
        "  Write-Log \"Domain DNS lookup failed for $DomainName on attempt $attempt of 10. Waiting before retry.\" 'WARNING'"
            .to_string(),
    );
    lines.push("  if ($attempt -eq 10) {".to_string());
    lines.push(
        "    throw \"Domain $DomainName could not be resolved. Check the active network and DNS settings before retrying.\""
            .to_string(),
    );
    lines.push("  }".to_string());
    lines.push("  Start-Sleep -Seconds 6".to_string());
    lines.push("}".to_string());
    lines.push("Write-Log \"Starting domain join for $DomainName using $Username.\"".to_string());
    lines.push("try {".to_string());
    lines.push("  Add-Computer @params".to_string());
    lines.push("  Write-Log \"Domain join completed for $DomainName.\" 'SUCCESS'".to_string());
    lines.push("} catch {".to_string());
    lines.push("  Write-Log \"Domain join failed: $($_.Exception.Message)\" 'ERROR'".to_string());
    lines.push("  throw".to_string());
    lines.push("}".to_string());
    if reboot_after_join {
        lines.push("shutdown /r /t 15 /c 'BitOSDT: Rebooting after domain join'".to_string());
    }

    lines.join("\n")
}

fn map_wifi_authentication_for_netsh(value: &str) -> &'static str {
    match value {
        "Open" => "open",
        "Wpa3Sae" => "WPA3SAE",
        _ => "WPA2PSK",
    }
}

fn map_wifi_encryption_for_netsh(value: &str, authentication: &str) -> &'static str {
    if authentication == "Open" {
        "none"
    } else {
        match value {
            "None" => "none",
            "Tkip" => "TKIP",
            _ => "AES",
        }
    }
}

fn build_wifi_connect_script(wifi: &OobeWifiConfig) -> String {
    let authentication = map_wifi_authentication_for_netsh(&wifi.authentication);
    let encryption = map_wifi_encryption_for_netsh(&wifi.encryption, &wifi.authentication);
    let ssid_xml = escape_xml(wifi.ssid.trim());
    let ssid_hex = wifi
        .ssid
        .as_bytes()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>();
    let key_material = escape_xml(wifi.password.trim());
    let hidden_network = if wifi.hidden_network { "true" } else { "false" };
    let connection_mode = if wifi.auto_connect { "auto" } else { "manual" };
    let dns_servers = [wifi.dns_server_1.trim(), wifi.dns_server_2.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(|value| format!("'{}'", escape_ps_single_quoted(value)))
        .collect::<Vec<_>>()
        .join(", ");

    let shared_key_block = if authentication == "open" {
        String::new()
    } else {
        format!(
            "      <sharedKey>\n        <keyType>passPhrase</keyType>\n        <protected>false</protected>\n        <keyMaterial>{}</keyMaterial>\n      </sharedKey>\n",
            key_material
        )
    };

    let profile_xml = format!(
        r#"<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{ssid_xml}</name>
  <SSIDConfig>
    <SSID>
      <hex>{ssid_hex}</hex>
      <name>{ssid_xml}</name>
    </SSID>
    <nonBroadcast>{hidden_network}</nonBroadcast>
  </SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>{connection_mode}</connectionMode>
  <MSM>
    <security>
      <authEncryption>
        <authentication>{authentication}</authentication>
        <encryption>{encryption}</encryption>
        <useOneX>false</useOneX>
      </authEncryption>
{shared_key_block}    </security>
  </MSM>
</WLANProfile>"#
    );
    let profile_xml = profile_xml.replace('\'', "''");
    let ssid_ps = escape_ps_single_quoted(wifi.ssid.trim());

    vec![
        "$ErrorActionPreference = 'Stop'".to_string(),
        "$ProgressPreference = 'SilentlyContinue'".to_string(),
        format!("$ssid = '{}'", ssid_ps),
        format!("$dnsServers = @({})", dns_servers),
        "$tempProfilePath = Join-Path $env:TEMP 'bitosdt-wifi-profile.xml'".to_string(),
        format!("$profileXml = @'\n{}\n'@", profile_xml),
        "Set-Content -LiteralPath $tempProfilePath -Value $profileXml -Encoding UTF8".to_string(),
        "function Get-BitOSDTWifiAdapter {".to_string(),
        "  Get-NetAdapter -Physical -ErrorAction SilentlyContinue |".to_string(),
        "    Where-Object {".to_string(),
        "      $_.Status -eq 'Up' -and (".to_string(),
        "        $_.InterfaceDescription -match 'Wireless|Wi-?Fi|WLAN|802\\.11' -or".to_string(),
        "        $_.Name -match 'Wireless|Wi-?Fi|WLAN|802\\.11'".to_string(),
        "      )".to_string(),
        "    } |".to_string(),
        "    Sort-Object ifIndex |".to_string(),
        "    Select-Object -First 1".to_string(),
        "}".to_string(),
        "try {".to_string(),
        "  netsh wlan add profile filename=\"$tempProfilePath\" user=all | Out-Null".to_string(),
        "} finally {".to_string(),
        "  Remove-Item -LiteralPath $tempProfilePath -Force -ErrorAction SilentlyContinue"
            .to_string(),
        "}".to_string(),
        "netsh wlan connect name=\"$ssid\" | Out-Null".to_string(),
        "if ($dnsServers.Count -gt 0) {".to_string(),
        "  $wifiAdapter = $null".to_string(),
        "  for ($attempt = 0; $attempt -lt 15; $attempt++) {".to_string(),
        "    $wifiAdapter = Get-BitOSDTWifiAdapter".to_string(),
        "    if ($null -ne $wifiAdapter) {".to_string(),
        "      break".to_string(),
        "    }".to_string(),
        "    Start-Sleep -Seconds 2".to_string(),
        "  }".to_string(),
        "  if ($null -eq $wifiAdapter) {".to_string(),
        "    throw 'Connected to Wi-Fi but could not identify the active wireless adapter for DNS configuration.'"
            .to_string(),
        "  }".to_string(),
        "  Set-DnsClientServerAddress -InterfaceIndex $wifiAdapter.ifIndex -ServerAddresses $dnsServers -ErrorAction Stop".to_string(),
        "}".to_string(),
        "$connected = $false".to_string(),
        "for ($attempt = 0; $attempt -lt 15; $attempt++) {".to_string(),
        "  if (Test-Connection -ComputerName 1.1.1.1 -Count 1 -Quiet -ErrorAction SilentlyContinue) {"
            .to_string(),
        "    $connected = $true".to_string(),
        "    break".to_string(),
        "  }".to_string(),
        "  Start-Sleep -Seconds 4".to_string(),
        "}".to_string(),
        "if (-not $connected) {".to_string(),
        "  throw 'Wi-Fi profile was applied but connectivity check failed (1.1.1.1 unreachable).'"
            .to_string(),
        "}".to_string(),
    ]
    .join("\n")
}

fn build_usb_orchestrator_script(
    request: &OobeProfileRequest,
    has_install_apps_script: bool,
    custom_script_files: &[String],
) -> String {
    let explicit_name = request
        .oobe_config
        .computer_name
        .clone()
        .unwrap_or_default();
    let explicit_name = escape_ps_single_quoted(explicit_name.trim());
    let prompt_for_computer_name = should_prompt_for_computer_name(request);
    let domain_join_enabled =
        request.domain_join.enabled && request.domain_join_mode == DomainJoinMode::PostRenameScript;
    let hide_privacy_settings = request.oobe_config.hide_privacy_settings;
    let wifi_enabled = request.wifi.enabled;
    let custom_script_enabled =
        request.apps.enable_custom_scripts && !custom_script_files.is_empty();
    let run_debloat = request.enable_debloat;

    format!(
        r#"$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$StatePath = 'C:\ProgramData\BitOSDT\usb-oobe-state.json'
$CompletionFlagPath = 'C:\ProgramData\BitOSDT\usb-oobe-complete.flag'
$OrchestratorPath = '{runtime_scripts_dir}\{orchestrator_script}'
$LogPath = 'C:\BitOSDT\Logs\usb-oobe-orchestrator.log'
$RunOncePath = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce'
$RunOnceName = '{runonce_name}'
$WinlogonPath = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
$PromptForComputerName = ${prompt_for_name}
$ExplicitComputerName = '{explicit_name}'
$DomainJoinEnabled = ${domain_join_enabled}
$HidePrivacySettings = {hide_privacy_settings}
$WifiEnabled = ${wifi_enabled}
$InstallAppsEnabled = ${install_apps_enabled}
$DebloatEnabled = ${debloat_enabled}
$CustomScriptsEnabled = ${custom_scripts_enabled}

function Write-Log {{
    param([string]$Message, [string]$Level = 'INFO')
    $directory = Split-Path -Path $LogPath -Parent
    if (-not (Test-Path -LiteralPath $directory)) {{
        New-Item -Path $directory -ItemType Directory -Force | Out-Null
    }}
    $line = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') [$Level] $Message"
    $line | Out-File -FilePath $LogPath -Encoding utf8 -Append
}}

function Ensure-RunOnce {{
    if (-not (Test-Path -LiteralPath $RunOncePath)) {{
        New-Item -Path $RunOncePath -Force | Out-Null
    }}
    $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$OrchestratorPath`""
    New-ItemProperty -Path $RunOncePath -Name $RunOnceName -PropertyType String -Value $command -Force | Out-Null
}}

function Remove-RunOnce {{
    Remove-ItemProperty -Path $RunOncePath -Name $RunOnceName -ErrorAction SilentlyContinue
}}

function Set-PrivacyExperiencePolicy {{
    $policyPath = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\OOBE'
    if (-not (Test-Path -LiteralPath $policyPath)) {{
        New-Item -Path $policyPath -Force | Out-Null
    }}
    if ($HidePrivacySettings) {{
        New-ItemProperty -Path $policyPath -Name 'DisablePrivacyExperience' -PropertyType DWord -Value 1 -Force | Out-Null
        Write-Log 'Configured DisablePrivacyExperience=1 to suppress the privacy settings screen.'
    }} else {{
        Remove-ItemProperty -Path $policyPath -Name 'DisablePrivacyExperience' -ErrorAction SilentlyContinue
        Write-Log 'Privacy settings screen suppression disabled for this profile.'
    }}
}}

function Save-State {{
    param([int]$Phase)
    $directory = Split-Path -Path $StatePath -Parent
    if (-not (Test-Path -LiteralPath $directory)) {{
        New-Item -Path $directory -ItemType Directory -Force | Out-Null
    }}
    $state = [ordered]@{{
        phase = $Phase
        updatedAt = (Get-Date).ToString('o')
    }}
    $state | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $StatePath -Encoding UTF8
}}

function Load-State {{
    if (-not (Test-Path -LiteralPath $StatePath)) {{
        return 0
    }}
    try {{
        $state = Get-Content -LiteralPath $StatePath -Raw | ConvertFrom-Json
        if ($null -eq $state -or $null -eq $state.phase) {{
            return 0
        }}
        return [int]$state.phase
    }} catch {{
        Write-Log "Failed to parse USB state file, defaulting to phase 0: $($_.Exception.Message)" 'WARNING'
        return 0
    }}
}}

function Validate-ComputerName {{
    param([Parameter(Mandatory = $true)][string]$Name)
    $trimmed = $Name.Trim()
    if ([string]::IsNullOrWhiteSpace($trimmed)) {{
        throw 'Computer name is required.'
    }}
    if ($trimmed.Length -gt 15) {{
        throw 'Computer name must be 1-15 characters.'
    }}
    if ($trimmed.StartsWith('-') -or $trimmed.EndsWith('-')) {{
        throw 'Computer name cannot start or end with -.'
    }}
    if ($trimmed -notmatch '^[A-Za-z0-9-]+$') {{
        throw 'Computer name can only contain letters, numbers, and -.'
    }}
    return $trimmed
}}

function Prompt-ComputerName {{
    try {{
        Add-Type -AssemblyName Microsoft.VisualBasic -ErrorAction Stop
        $candidate = [Microsoft.VisualBasic.Interaction]::InputBox(
            'Enter computer name (letters, numbers, and hyphen only, max 15 characters).',
            'BitOSDT Computer Name',
            ''
        )
    }} catch {{
        $candidate = Read-Host 'Enter Computer Name'
    }}
    if ([string]::IsNullOrWhiteSpace($candidate)) {{
        throw 'Computer name prompt was cancelled or blank.'
    }}
    return (Validate-ComputerName -Name $candidate)
}}

function Resolve-ComputerName {{
    if (-not [string]::IsNullOrWhiteSpace($ExplicitComputerName)) {{
        return (Validate-ComputerName -Name $ExplicitComputerName)
    }}
    if ($PromptForComputerName) {{
        return (Prompt-ComputerName)
    }}
    return $env:COMPUTERNAME
}}

function Invoke-BitOSDTScript {{
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [switch]$Optional
    )
    $path = Join-Path '{runtime_scripts_dir}' $Name
    if (-not (Test-Path -LiteralPath $path)) {{
        if ($Optional) {{
            Write-Log "Optional script not present: $path" 'WARNING'
            return
        }}
        throw "Required script not found: $path"
    }}
    Write-Log "Running $path"
    $proc = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $path) -Wait -PassThru -NoNewWindow
    if ($proc.ExitCode -ne 0) {{
        throw "Script failed with exit code $($proc.ExitCode): $Name"
    }}
}}

function Resolve-BuiltinAdministratorName {{
    try {{
        $account = Get-CimInstance Win32_UserAccount -Filter "LocalAccount=True" |
            Where-Object {{ $_.SID -like '*-500' }} |
            Select-Object -First 1
        if ($null -ne $account -and -not [string]::IsNullOrWhiteSpace($account.Name)) {{
            return $account.Name
        }}
    }} catch {{
        Write-Log "Failed to resolve built-in Administrator name from SID: $($_.Exception.Message)" 'WARNING'
    }}
    return '{bootstrap_admin_username}'
}}

function Clear-Autologon {{
    if (-not (Test-Path -LiteralPath $WinlogonPath)) {{
        return
    }}
    foreach ($name in @('DefaultUserName', 'DefaultPassword', 'DefaultDomainName', 'AutoLogonCount', 'ForceAutoLogon')) {{
        Remove-ItemProperty -Path $WinlogonPath -Name $name -ErrorAction SilentlyContinue
    }}
    try {{
        Set-ItemProperty -Path $WinlogonPath -Name 'AutoAdminLogon' -Value '0' -ErrorAction Stop
    }} catch {{
        New-ItemProperty -Path $WinlogonPath -Name 'AutoAdminLogon' -PropertyType String -Value '0' -Force | Out-Null
    }}
}}

function Disable-BuiltinAdministrator {{
    $adminName = Resolve-BuiltinAdministratorName
    & net.exe user $adminName /active:no | Out-Null
    if ($LASTEXITCODE -ne 0) {{
        throw "Failed to disable built-in Administrator account '$adminName'."
    }}
}}

function Cleanup-Bootstrap {{
    param([switch]$Success)
    Remove-RunOnce
    Clear-Autologon
    Disable-BuiltinAdministrator
    Remove-Item -LiteralPath $StatePath -Force -ErrorAction SilentlyContinue
    if ($Success) {{
        'COMPLETE' | Set-Content -LiteralPath $CompletionFlagPath -Encoding ASCII
    }}
}}

try {{
    $phase = Load-State
    Write-Log "Starting USB OOBE orchestrator phase $phase"
    Set-PrivacyExperiencePolicy

    if ($phase -le 0) {{
        $targetName = Resolve-ComputerName
        Set-Content -LiteralPath '{runtime_scripts_dir}\pcname.txt' -Value $targetName -Encoding ASCII
        if ($targetName -ne $env:COMPUTERNAME) {{
            Write-Log "Renaming computer from $env:COMPUTERNAME to $targetName"
            Rename-Computer -NewName $targetName -Force -ErrorAction Stop
            Save-State -Phase 1
            Ensure-RunOnce
            Write-Log 'Phase 0 complete. Rebooting for phase 1.'
            Restart-Computer -Force
            return
        }}
        Write-Log "Computer already named $targetName"
        Save-State -Phase 1
        $phase = 1
    }}

    if ($phase -le 1) {{
        if ($WifiEnabled) {{
            Invoke-BitOSDTScript -Name '{wifi_script}' -Optional
        }}
        $domainJoinTriggered = $false
        if ($DomainJoinEnabled) {{
            Invoke-BitOSDTScript -Name 'domainjoin.ps1'
            $domainJoinTriggered = $true
        }}
        Save-State -Phase 2
        if ($domainJoinTriggered) {{
            Ensure-RunOnce
            Write-Log 'Phase 1 complete. Rebooting for phase 2.'
            Restart-Computer -Force
            return
        }}
        Write-Log 'Phase 1 complete. Continuing without reboot.'
        $phase = 2
    }}

    if ($phase -le 2) {{
        if ($InstallAppsEnabled) {{
            Invoke-BitOSDTScript -Name 'installapps.ps1'
        }}
        if ($DebloatEnabled) {{
            Invoke-BitOSDTScript -Name 'debloat.ps1' -Optional
        }}
        if ($CustomScriptsEnabled) {{
            Get-ChildItem -LiteralPath '{runtime_scripts_dir}' -Filter 'custom-*.ps1' -File -ErrorAction SilentlyContinue |
                Sort-Object Name |
                ForEach-Object {{
                    Invoke-BitOSDTScript -Name $_.Name
                }}
        }}
        Cleanup-Bootstrap -Success
        Write-Log 'USB OOBE orchestration completed successfully.' 'SUCCESS'
        Restart-Computer -Force
        return
    }}

    throw "Unsupported USB OOBE phase value: $phase"
}} catch {{
    $message = $_.Exception.Message
    Write-Log "USB OOBE orchestration failed: $message" 'ERROR'
    try {{
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show(
            "BitOSDT USB OOBE failed: $message`nSee $LogPath for details.",
            'BitOSDT USB OOBE',
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Error
        ) | Out-Null
    }} catch {{
        Write-Log "Unable to display failure dialog: $($_.Exception.Message)" 'WARNING'
    }}
    try {{
        Cleanup-Bootstrap
    }} catch {{
        Write-Log "Cleanup after failure also failed: $($_.Exception.Message)" 'WARNING'
    }}
    Restart-Computer -Force
}}
"#,
        orchestrator_script = USB_ORCHESTRATOR_SCRIPT,
        runonce_name = USB_RUNONCE_NAME,
        prompt_for_name = if prompt_for_computer_name {
            "true"
        } else {
            "false"
        },
        explicit_name = explicit_name,
        domain_join_enabled = if domain_join_enabled { "true" } else { "false" },
        hide_privacy_settings = if hide_privacy_settings {
            "$true"
        } else {
            "$false"
        },
        wifi_enabled = if wifi_enabled { "true" } else { "false" },
        install_apps_enabled = if has_install_apps_script {
            "true"
        } else {
            "false"
        },
        debloat_enabled = if run_debloat { "true" } else { "false" },
        custom_scripts_enabled = if custom_script_enabled {
            "true"
        } else {
            "false"
        },
        wifi_script = PROVISIONING_WIFI_SCRIPT,
        bootstrap_admin_username = USB_BOOTSTRAP_ADMIN_USERNAME,
        runtime_scripts_dir = BITOSDT_RUNTIME_SCRIPTS_DIR
    )
}

fn provisioning_app_item_count(apps: &OobeAppsConfig) -> usize {
    apps.copied_items.len()
        + apps
            .winget_packages
            .iter()
            .filter(|pkg| pkg.enabled)
            .count()
        + apps
            .chocolatey_packages
            .iter()
            .filter(|pkg| pkg.enabled)
            .count()
        + apps
            .custom_installers
            .iter()
            .filter(|installer| installer.enabled)
            .count()
        + apps
            .custom_installers
            .iter()
            .filter(|installer| installer.enabled)
            .map(|installer| installer.dependencies.len())
            .sum::<usize>()
}

struct ProvisioningRegionalSettings {
    language: String,
    input_locale: String,
    timezone: String,
}

fn resolve_provisioning_regional_settings(
    request: &OobeProfileRequest,
) -> Result<ProvisioningRegionalSettings, String> {
    let (language, input_locale) = resolve_oobe_locale_settings(request)?;
    let timezone = if request.timezone.trim().is_empty() {
        DEFAULT_TIMEZONE.to_string()
    } else {
        request.timezone.trim().to_string()
    };

    Ok(ProvisioningRegionalSettings {
        language,
        input_locale,
        timezone,
    })
}

fn build_provisioning_ui_profile_snapshot(
    request: &OobeProfileRequest,
    regional_settings: &ProvisioningRegionalSettings,
    has_install_apps_script: bool,
    custom_script_files: &[String],
) -> Result<String, String> {
    let explicit_name = if provisioning_post_signin_computer_name_required(request) {
        String::new()
    } else {
        provisioning_native_computer_name(request).unwrap_or_default()
    };
    let wifi_dns_servers = [
        request.wifi.dns_server_1.trim(),
        request.wifi.dns_server_2.trim(),
    ]
    .into_iter()
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>();
    let snapshot = serde_json::json!({
        "schemaVersion": 1,
        "name": request.name,
        "description": request.description,
        "language": regional_settings.language,
        "inputLocale": regional_settings.input_locale,
        "timezone": regional_settings.timezone,
        "skipMachineOobe": request.oobe_config.skip_machine_oobe,
        "skipUserOobe": request.oobe_config.skip_user_oobe,
        "hideEula": request.oobe_config.hide_eula,
        "hidePrivacySettings": request.oobe_config.hide_privacy_settings,
        "hideWirelessSetup": request.oobe_config.hide_wireless_setup,
        "hideOnlineAccountScreens": request.oobe_config.hide_online_account_screens,
        "defaultUserEnabled": request.default_user.enabled,
        "defaultUserNativeApplied": request.default_user.enabled,
        "promptForComputerName": provisioning_post_signin_computer_name_required(request),
        "explicitComputerName": explicit_name,
        "computerNameNativeApplied": provisioning_native_computer_name(request).is_some(),
        "wifiEnabled": provisioning_post_signin_wifi_required(request),
        "wifiNativeProfileApplied": provisioning_native_wifi_supported(request),
        "wifiSsid": request.wifi.ssid,
        "wifiDnsServers": wifi_dns_servers,
        "domainJoinEnabled": provisioning_post_signin_domain_join_required(request),
        "domainJoinNativeApplied": provisioning_native_domain_join_supported(request),
        "domainName": request.domain_join.domain,
        "disableBitLocker": request.apps.disable_bitlocker,
        "rebootAfterDisableBitLocker": request.apps.reboot_after_disable_bitlocker,
        "appItemCount": if has_install_apps_script {
            provisioning_app_item_count(&request.apps)
        } else {
            0
        },
        "debloatEnabled": request.enable_debloat,
        "customScriptCount": custom_script_files.len(),
        "postSignInTasksPending": provisioning_post_signin_computer_name_required(request)
            || provisioning_post_signin_wifi_required(request)
            || provisioning_post_signin_domain_join_required(request)
            || request.apps.disable_bitlocker
            || has_install_apps_script
            || request.enable_debloat
            || !custom_script_files.is_empty(),
        "copiedItemsPending": !request.apps.copied_items.is_empty()
    });

    serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("Failed to serialize provisioning UI snapshot: {}", e))
}

fn build_provisioning_orchestrator_script(
    request: &OobeProfileRequest,
    regional_settings: &ProvisioningRegionalSettings,
    has_install_apps_script: bool,
    custom_script_files: &[String],
) -> String {
    let explicit_name = if provisioning_post_signin_computer_name_required(request) {
        String::new()
    } else {
        provisioning_native_computer_name(request).unwrap_or_default()
    };
    let explicit_name = escape_ps_single_quoted(explicit_name.trim());
    let prompt_for_computer_name = provisioning_post_signin_computer_name_required(request);
    let domain_join_enabled = provisioning_post_signin_domain_join_required(request);
    let wifi_enabled = provisioning_post_signin_wifi_required(request);
    let bitlocker_enabled = request.apps.disable_bitlocker;
    let bitlocker_reboot_after_disable = request.apps.reboot_after_disable_bitlocker;
    let custom_script_enabled =
        request.apps.enable_custom_scripts && !custom_script_files.is_empty();
    let run_debloat = request.enable_debloat;
    let regional_language = escape_ps_single_quoted(&regional_settings.language);
    let regional_input_locale = escape_ps_single_quoted(&regional_settings.input_locale);
    let regional_timezone = escape_ps_single_quoted(&regional_settings.timezone);
    let mut script = r#"
param(
    [string]$Action = 'Launch'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$RuntimeScriptsDir = '__RUNTIME_SCRIPTS_DIR__'
$OrchestratorPath = Join-Path $RuntimeScriptsDir '__ORCHESTRATOR_SCRIPT__'
$HtaPath = Join-Path $RuntimeScriptsDir '__HTA_FILE__'
$KioskHelperPath = Join-Path $RuntimeScriptsDir '__KIOSK_HELPER_FILE__'
$ProfileSnapshotPath = Join-Path $RuntimeScriptsDir '__PROFILE_SNAPSHOT_FILE__'
$LogPath = '__LOG_PATH__'
$ShellLogPath = '__SHELL_LOG_PATH__'
$RunOncePath = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce'
$RunOnceName = '__RUNONCE_NAME__'
$ScheduledTaskName = '__SCHEDULED_TASK_NAME__'
$ProfilePath = '__PROFILE_PATH__'
$UiStatePath = '__UI_STATE_PATH__'
$StatusPath = '__STATUS_PATH__'
$CommandPath = '__COMMAND_PATH__'
$AppProgressPath = '__APP_PROGRESS_PATH__'
$HeartbeatPath = '__HEARTBEAT_PATH__'
$PromptForComputerName = __PROMPT_FOR_NAME__
$ExplicitComputerName = '__EXPLICIT_NAME__'
$RegionalLanguage = '__REGIONAL_LANGUAGE__'
$RegionalInputLocale = '__REGIONAL_INPUT_LOCALE__'
$RegionalTimeZone = '__REGIONAL_TIMEZONE__'
$DomainJoinEnabled = __DOMAIN_JOIN_ENABLED__
$WifiEnabled = __WIFI_ENABLED__
$BitLockerEnabled = __BITLOCKER_ENABLED__
$BitLockerRebootAfterDisable = __BITLOCKER_REBOOT_AFTER_DISABLE__
$InstallAppsEnabled = __INSTALL_APPS_ENABLED__
$DebloatEnabled = __DEBLOAT_ENABLED__
$CustomScriptsEnabled = __CUSTOM_SCRIPTS_ENABLED__
$WifiScriptName = '__WIFI_SCRIPT__'

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $directory = Split-Path -Path $LogPath -Parent
    if (-not (Test-Path -LiteralPath $directory)) {
        New-Item -Path $directory -ItemType Directory -Force | Out-Null
    }
    $line = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') [$Level] $Message"
    $line | Out-File -FilePath $LogPath -Encoding utf8 -Append
}

function Ensure-Directory {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -ItemType Directory -Force | Out-Null
    }
}

function Read-JsonFile {
    param(
        [string]$Path,
        $DefaultValue = $null
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        return $DefaultValue
    }
    try {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    } catch {
        Write-Log "Failed to parse JSON at ${Path}: $($_.Exception.Message)" 'WARNING'
        return $DefaultValue
    }
}

function Write-JsonFile {
    param(
        [string]$Path,
        [Parameter(Mandatory = $true)]$Value
    )
    $directory = Split-Path -Path $Path -Parent
    Ensure-Directory -Path $directory
    $json = $Value | ConvertTo-Json -Depth 8
    $lastErrorMessage = ''

    for ($attempt = 1; $attempt -le 12; $attempt++) {
        try {
            Set-Content -LiteralPath $Path -Value $json -Encoding UTF8 -Force -ErrorAction Stop
            return
        } catch {
            $lastErrorMessage = $_.Exception.Message
            if ($attempt -ge 12) {
                break
            }
            Start-Sleep -Milliseconds 150
        }
    }

    throw "Failed to write JSON file $Path after multiple attempts: $lastErrorMessage"
}

function Normalize-RestartChoices {
    param(
        $Value,
        $Steps
    )

    $normalized = [ordered]@{}
    foreach ($step in @($Steps)) {
        $stepId = [string]$step.id
        $restartValue = [bool]$step.defaultRestart

        if ($null -ne $Value) {
            if ($Value -is [System.Collections.IDictionary]) {
                if ($Value.Contains($stepId)) {
                    $restartValue = [bool]$Value[$stepId]
                }
            } elseif ($Value.PSObject.Properties.Name -contains $stepId) {
                $restartValue = [bool]$Value.$stepId
            }
        }

        $normalized[$stepId] = $restartValue
    }

    return $normalized
}

function Get-HeartbeatSummary {
    $heartbeat = Read-JsonFile -Path $HeartbeatPath
    if ($null -eq $heartbeat) {
        return "heartbeat unavailable at $HeartbeatPath"
    }

    $lastTick = if ($heartbeat.PSObject.Properties.Name -contains 'lastTickUtc' -and -not [string]::IsNullOrWhiteSpace($heartbeat.lastTickUtc)) {
        [string]$heartbeat.lastTickUtc
    } else {
        'n/a'
    }
    $lastRender = if ($heartbeat.PSObject.Properties.Name -contains 'lastRenderUtc' -and -not [string]::IsNullOrWhiteSpace($heartbeat.lastRenderUtc)) {
        [string]$heartbeat.lastRenderUtc
    } else {
        'n/a'
    }
    $lastError = if ($heartbeat.PSObject.Properties.Name -contains 'lastError' -and -not [string]::IsNullOrWhiteSpace($heartbeat.lastError)) {
        [string]$heartbeat.lastError
    } else {
        'none'
    }
    $inTick = if ($heartbeat.PSObject.Properties.Name -contains 'inTick') {
        [bool]$heartbeat.inTick
    } else {
        $false
    }

    return "heartbeat lastTick=$lastTick lastRender=$lastRender inTick=$inTick lastError=$lastError"
}

function Get-HeartbeatInfo {
    $heartbeat = Read-JsonFile -Path $HeartbeatPath
    if ($null -eq $heartbeat) {
        return $null
    }

    $lastTickUtc = $null
    if ($heartbeat.PSObject.Properties.Name -contains 'lastTickUtc' -and -not [string]::IsNullOrWhiteSpace($heartbeat.lastTickUtc)) {
        try {
            $lastTickUtc = [DateTime]::Parse([string]$heartbeat.lastTickUtc).ToUniversalTime()
        } catch {
            Write-Log "Invalid heartbeat lastTickUtc value: $($heartbeat.lastTickUtc)" 'WARNING'
        }
    }

    $lastRenderUtc = $null
    if ($heartbeat.PSObject.Properties.Name -contains 'lastRenderUtc' -and -not [string]::IsNullOrWhiteSpace($heartbeat.lastRenderUtc)) {
        try {
            $lastRenderUtc = [DateTime]::Parse([string]$heartbeat.lastRenderUtc).ToUniversalTime()
        } catch {
        }
    }

    $ageSeconds = $null
    if ($null -ne $lastTickUtc) {
        $ageSeconds = [Math]::Round(((Get-Date).ToUniversalTime() - $lastTickUtc).TotalSeconds, 1)
    }

    return [pscustomobject]@{
        lastTickUtc = $lastTickUtc
        lastRenderUtc = $lastRenderUtc
        ageSeconds = $ageSeconds
        inTick = if ($heartbeat.PSObject.Properties.Name -contains 'inTick') { [bool]$heartbeat.inTick } else { $false }
        lastError = if ($heartbeat.PSObject.Properties.Name -contains 'lastError') { [string]$heartbeat.lastError } else { '' }
    }
}

function Test-HeartbeatHealthy {
    param(
        [DateTime]$NotBeforeUtc = [DateTime]::MinValue,
        [int]$FreshWithinSeconds = 20
    )

    $heartbeatInfo = Get-HeartbeatInfo
    if ($null -eq $heartbeatInfo -or $null -eq $heartbeatInfo.lastTickUtc -or $null -eq $heartbeatInfo.ageSeconds) {
        return $false
    }

    if ($NotBeforeUtc -ne [DateTime]::MinValue -and $heartbeatInfo.lastTickUtc.AddSeconds(1) -lt $NotBeforeUtc) {
        return $false
    }

    return [double]$heartbeatInfo.ageSeconds -le $FreshWithinSeconds
}

function Wait-ForFreshHeartbeat {
    param(
        [Parameter(Mandatory = $true)][DateTime]$NotBeforeUtc,
        [int]$TimeoutSeconds = 20,
        [int]$FreshWithinSeconds = 20
    )

    $deadline = (Get-Date).ToUniversalTime().AddSeconds($TimeoutSeconds)
    while ((Get-Date).ToUniversalTime() -lt $deadline) {
        if (Test-HeartbeatHealthy -NotBeforeUtc $NotBeforeUtc -FreshWithinSeconds $FreshWithinSeconds) {
            return $true
        }
        Start-Sleep -Milliseconds 500
    }

    return $false
}

function Find-ProvisioningUiProcess {
    $htaPathNormalized = $HtaPath.ToLowerInvariant()

    try {
        $matchingProcessIds = @(
            Get-CimInstance Win32_Process -Filter "Name = 'mshta.exe'" -ErrorAction SilentlyContinue |
                Where-Object { $_.CommandLine -and $_.CommandLine.ToLowerInvariant().Contains($htaPathNormalized) } |
                Sort-Object ProcessId -Descending |
                Select-Object -ExpandProperty ProcessId
        )
        foreach ($processId in $matchingProcessIds) {
            try {
                return Get-Process -Id $processId -ErrorAction Stop
            } catch {
            }
        }
    } catch {
        Write-Log "Failed to enumerate provisioning mshta processes: $($_.Exception.Message)" 'WARNING'
    }

    try {
        return Get-Process -Name mshta -ErrorAction SilentlyContinue |
            Where-Object { $_.MainWindowTitle -eq 'BitOSDT Provisioning' } |
            Select-Object -First 1
    } catch {
        return $null
    }
}

function Stop-ProvisioningUiProcessSafe {
    param(
        $Process,
        [string]$Reason = ''
    )

    if ($null -eq $Process) {
        return
    }

    try {
        $processId = $Process.Id
        $reasonText = if ([string]::IsNullOrWhiteSpace($Reason)) { 'no reason supplied' } else { $Reason }
        Write-Log ("Stopping provisioning HTA pid={0}. Reason: {1}" -f $processId, $reasonText) 'WARNING'
        Stop-Process -Id $processId -Force -ErrorAction Stop
        Start-Sleep -Milliseconds 400
    } catch {
        Write-Log ("Failed to stop provisioning HTA pid={0}: {1}" -f $Process.Id, $_.Exception.Message) 'WARNING'
    }
}

function Start-ProvisioningUiHost {
    param([int]$Attempt = 1)

    Remove-Item -LiteralPath $HeartbeatPath -Force -ErrorAction SilentlyContinue
    $launchStartedAtUtc = (Get-Date).ToUniversalTime()
    $htaProcess = Start-Process -FilePath 'mshta.exe' -ArgumentList "`"$HtaPath`"" -PassThru
    Write-Log ("Launched provisioning HTA: {0}; pid={1}; attempt={2}; heartbeatPath={3}" -f $HtaPath, $htaProcess.Id, $Attempt, $HeartbeatPath)
    Write-Log "Skipping provisioning kiosk helper on full Windows; the HTA self-manages fullscreen because external chrome stripping can destabilize mshta."

    if (Wait-ForFreshHeartbeat -NotBeforeUtc $launchStartedAtUtc) {
        Write-Log ("Provisioning HTA heartbeat confirmed for pid={0}. {1}" -f $htaProcess.Id, (Get-HeartbeatSummary))
        return $true
    }

    Write-Log ("Provisioning HTA did not publish a fresh heartbeat after launch attempt {0}. {1}" -f $Attempt, (Get-HeartbeatSummary)) 'WARNING'
    Stop-ProvisioningUiProcessSafe -Process $htaProcess -Reason "missing fresh heartbeat after launch attempt $Attempt"
    return $false
}

function Validate-ComputerName {
    param([Parameter(Mandatory = $true)][string]$Name)
    $trimmed = $Name.Trim()
    if ([string]::IsNullOrWhiteSpace($trimmed)) {
        throw 'Computer name is required.'
    }
    if ($trimmed.Length -gt 15) {
        throw 'Computer name must be 1-15 characters.'
    }
    if ($trimmed.StartsWith('-') -or $trimmed.EndsWith('-')) {
        throw 'Computer name cannot start or end with -.'
    }
    if ($trimmed -notmatch '^[A-Za-z0-9-]+$') {
        throw 'Computer name can only contain letters, numbers, and -.'
    }
    return $trimmed
}

function Invoke-BitOSDTScript {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [switch]$Optional
    )
    $path = Join-Path $RuntimeScriptsDir $Name
    if (-not (Test-Path -LiteralPath $path)) {
        if ($Optional) {
            Write-Log "Optional script not present: $path" 'WARNING'
            return
        }
        throw "Required script not found: $path"
    }

    Write-Log "Running $path"
    $proc = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $path) -Wait -PassThru -NoNewWindow
    if ($proc.ExitCode -ne 0) {
        throw "Script failed with exit code $($proc.ExitCode): $Name"
    }
}

function Ensure-Connectivity {
    $connected = $false
    for ($attempt = 0; $attempt -lt 15; $attempt++) {
        if (Test-Connection -ComputerName 1.1.1.1 -Count 1 -Quiet -ErrorAction SilentlyContinue) {
            $connected = $true
            break
        }
        Start-Sleep -Seconds 4
    }
    if (-not $connected) {
        throw 'Network connectivity validation failed (1.1.1.1 unreachable).'
    }
}

function Get-ProvisioningProfile {
    Ensure-Directory -Path '__STATE_DIR__'
    if (-not (Test-Path -LiteralPath $ProfilePath)) {
        if (-not (Test-Path -LiteralPath $ProfileSnapshotPath)) {
            throw "Provisioning UI profile snapshot missing at $ProfileSnapshotPath"
        }
        Copy-Item -LiteralPath $ProfileSnapshotPath -Destination $ProfilePath -Force
        Write-Log "Copied provisioning UI profile snapshot into $ProfilePath"
    }
    $profile = Read-JsonFile -Path $ProfilePath
    if ($null -eq $profile) {
        throw "Provisioning profile could not be loaded from $ProfilePath"
    }
    return $profile
}

function Resolve-RegionalSettings {
    param($Profile)

    $language = $RegionalLanguage
    $inputLocale = $RegionalInputLocale
    $timeZone = $RegionalTimeZone

    if ($null -ne $Profile) {
        if ($Profile.PSObject.Properties.Name -contains 'language' -and -not [string]::IsNullOrWhiteSpace([string]$Profile.language)) {
            $language = [string]$Profile.language
        }
        if ($Profile.PSObject.Properties.Name -contains 'inputLocale' -and -not [string]::IsNullOrWhiteSpace([string]$Profile.inputLocale)) {
            $inputLocale = [string]$Profile.inputLocale
        }
        if ($Profile.PSObject.Properties.Name -contains 'timezone' -and -not [string]::IsNullOrWhiteSpace([string]$Profile.timezone)) {
            $timeZone = [string]$Profile.timezone
        }
    }

    return [pscustomobject]@{
        language = $language
        inputLocale = $inputLocale
        timeZone = $timeZone
    }
}

function Set-UkDateFormat {
    try {
        $intlPath = 'HKCU:\Control Panel\International'
        if (-not (Test-Path -LiteralPath $intlPath)) {
            New-Item -Path $intlPath -Force | Out-Null
        }
        Set-ItemProperty -Path $intlPath -Name 'iDate' -Value '1' -Force
        Set-ItemProperty -Path $intlPath -Name 'sShortDate' -Value 'dd/MM/yyyy' -Force
        Set-ItemProperty -Path $intlPath -Name 'sLongDate' -Value 'dd MMMM yyyy' -Force
        Write-Log 'Applied UK date format override for GMT Standard Time.'
    } catch {
        Write-Log "Failed to apply UK date format override: $($_.Exception.Message)" 'WARNING'
    }
}

function Apply-RegionalSettings {
    param($Profile)

    $settings = Resolve-RegionalSettings -Profile $Profile

    if (-not [string]::IsNullOrWhiteSpace($settings.timeZone)) {
        try {
            Set-TimeZone -Id $settings.timeZone -ErrorAction Stop
            Write-Log "Applied timezone $($settings.timeZone)."
        } catch {
            try {
                $null = & tzutil.exe /s $settings.timeZone 2>&1
                if ($LASTEXITCODE -eq 0) {
                    Write-Log "Applied timezone $($settings.timeZone) via tzutil."
                } else {
                    throw "tzutil exit code $LASTEXITCODE"
                }
            } catch {
                Write-Log "Failed to apply timezone $($settings.timeZone): $($_.Exception.Message)" 'WARNING'
            }
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($settings.language)) {
        try {
            Set-Culture -CultureInfo $settings.language -ErrorAction Stop
            Write-Log "Applied culture $($settings.language)."
        } catch {
            Write-Log "Failed to apply culture $($settings.language): $($_.Exception.Message)" 'WARNING'
        }

        try {
            Set-WinSystemLocale -SystemLocale $settings.language -ErrorAction Stop
            Write-Log "Applied system locale $($settings.language)."
        } catch {
            Write-Log "Failed to apply system locale $($settings.language): $($_.Exception.Message)" 'WARNING'
        }

        try {
            $languageList = New-WinUserLanguageList -Language $settings.language
            if (-not [string]::IsNullOrWhiteSpace($settings.inputLocale) -and $settings.inputLocale.Contains(':')) {
                $languageList[0].InputMethodTips.Clear()
                $null = $languageList[0].InputMethodTips.Add($settings.inputLocale)
            }
            Set-WinUserLanguageList -LanguageList $languageList -Force -ErrorAction Stop
            Write-Log "Applied user language list $($settings.language)."
        } catch {
            Write-Log "Failed to apply user language list $($settings.language): $($_.Exception.Message)" 'WARNING'
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($settings.inputLocale) -and $settings.inputLocale.Contains(':')) {
        try {
            Set-WinDefaultInputMethodOverride -InputTip $settings.inputLocale -ErrorAction Stop
            Write-Log "Applied input locale $($settings.inputLocale)."
        } catch {
            Write-Log "Failed to apply input locale $($settings.inputLocale): $($_.Exception.Message)" 'WARNING'
        }
    }

    if ($settings.timeZone -eq 'GMT Standard Time') {
        Set-UkDateFormat
    }
}

function Get-StepDefinitions {
    param($Profile)

    $steps = @()

    if ($Profile.promptForComputerName) {
        $steps += [pscustomobject]@{ id = 'computerName'; title = 'Computer Name'; defaultRestart = $true }
    }
    if ($WifiEnabled) {
        $steps += [pscustomobject]@{ id = 'wifi'; title = 'Wi-Fi Settings'; defaultRestart = $false }
    }
    if ($DomainJoinEnabled) {
        $steps += [pscustomobject]@{ id = 'domainJoin'; title = 'Domain Join'; defaultRestart = $true }
    }
    if ($BitLockerEnabled) {
        $steps += [pscustomobject]@{ id = 'bitLocker'; title = 'BitLocker'; defaultRestart = $BitLockerRebootAfterDisable }
    }
    if ($InstallAppsEnabled) {
        $steps += [pscustomobject]@{ id = 'apps'; title = 'Applications'; defaultRestart = $false }
    }
    if ($DebloatEnabled -or $CustomScriptsEnabled) {
        $steps += [pscustomobject]@{ id = 'optionalScripts'; title = 'Custom Actions'; defaultRestart = $false }
    }

    return $steps
}

function New-UiState {
    param($Profile, $Steps)

    $restartChoices = [ordered]@{}
    foreach ($step in $Steps) {
        $restartChoices[$step.id] = [bool]$step.defaultRestart
    }

    $initialComputerName = if (-not [string]::IsNullOrWhiteSpace($Profile.explicitComputerName)) {
        $Profile.explicitComputerName
    } else {
        $env:COMPUTERNAME
    }

    return [ordered]@{
        schemaVersion = 1
        currentStepId = if ($Steps.Count -gt 0) { $Steps[0].id } else { 'complete' }
        completedStepIds = @()
        restartChoices = $restartChoices
        computerName = $initialComputerName
        inProgress = $false
        rebootPending = $false
        errorMessage = $null
        lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    }
}

function New-Status {
    param($Steps)

    $tasks = @()
    foreach ($step in $Steps) {
        $tasks += [ordered]@{
            id = $step.id
            title = $step.title
            status = 'pending'
            detail = 'Waiting'
        }
    }

    return [ordered]@{
        schemaVersion = 1
        terminalStatus = if ($Steps.Count -eq 0) { 'complete' } else { 'idle' }
        percentComplete = if ($Steps.Count -eq 0) { 100 } else { 0 }
        bannerMessage = if ($Steps.Count -eq 0) { 'Provisioning package applied successfully. No post-sign-in tasks were enabled for this package.' } else { 'Provisioning package applied. Sign in with an administrator account to continue the remaining post-sign-in tasks.' }
        errorMessage = $null
        tasks = $tasks
        lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    }
}

function Save-UiState {
    param($State)
    $State.lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    Write-JsonFile -Path $UiStatePath -Value $State
}

function Save-Status {
    param($Status)
    $Status.lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    Write-JsonFile -Path $StatusPath -Value $Status
}

function Update-StatusPercent {
    param($Status)
    $total = @($Status.tasks).Count
    if ($total -eq 0) {
        $Status.percentComplete = 100
        return
    }
    $done = @($Status.tasks | Where-Object { $_.status -eq 'complete' -or $_.status -eq 'reboot_pending' }).Count
    $Status.percentComplete = [Math]::Round(($done / $total) * 100)
}

function Set-TaskStatus {
    param(
        $Status,
        [string]$TaskId,
        [string]$TaskStatusValue,
        [string]$Detail
    )
    foreach ($task in $Status.tasks) {
        if ($task.id -eq $TaskId) {
            $task.status = $TaskStatusValue
            if (-not [string]::IsNullOrWhiteSpace($Detail)) {
                $task.detail = $Detail
            }
        } elseif ($TaskStatusValue -eq 'active' -and $task.status -eq 'active') {
            $task.status = 'pending'
        }
    }
    Update-StatusPercent -Status $Status
}

function Ensure-StateFiles {
    $profile = Get-ProvisioningProfile
    $steps = @(Get-StepDefinitions -Profile $profile)
    $state = Read-JsonFile -Path $UiStatePath
    $status = Read-JsonFile -Path $StatusPath

    if ($null -eq $state) {
        $state = New-UiState -Profile $profile -Steps $steps
        Write-Log "Creating provisioning UI state file at $UiStatePath"
        Save-UiState -State $state
    }

    if ($null -eq $status) {
        $status = New-Status -Steps $steps
        Write-Log "Creating provisioning status file at $StatusPath"
        Save-Status -Status $status
    }

    $state.restartChoices = Normalize-RestartChoices -Value $state.restartChoices -Steps $steps
    $state.completedStepIds = @($state.completedStepIds)
    $status.tasks = @($status.tasks)

    return [pscustomobject]@{
        profile = $profile
        steps = $steps
        state = $state
        status = $status
    }
}

function Get-NextStepId {
    param($Steps, [string]$CurrentStepId)
    for ($index = 0; $index -lt $Steps.Count; $index++) {
        if ($Steps[$index].id -eq $CurrentStepId) {
            if ($index + 1 -lt $Steps.Count) {
                return $Steps[$index + 1].id
            }
            return $null
        }
    }
    return $null
}

function Ensure-ScheduledTask {
    try {
        $null = & schtasks.exe /Query /TN $ScheduledTaskName 2>$null
        if ($LASTEXITCODE -eq 0) {
            return
        }
    } catch {
    }

    $taskCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$OrchestratorPath`""
    try {
        $createOutput = & schtasks.exe /Create /TN $ScheduledTaskName /SC ONLOGON /TR $taskCommand /RL HIGHEST /F 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Log "Scheduled task registered: $ScheduledTaskName"
            return
        }

        $detail = (($createOutput | Out-String).Trim())
        if ([string]::IsNullOrWhiteSpace($detail)) {
            $detail = "exit code $LASTEXITCODE"
        } else {
            $detail = "exit code $LASTEXITCODE; $detail"
        }
        Write-Log ("Failed to register scheduled task {0}: {1}" -f $ScheduledTaskName, $detail) 'WARNING'
    } catch {
        Write-Log "Failed to register scheduled task ${ScheduledTaskName}: $($_.Exception.Message)" 'WARNING'
    }
}

function Ensure-RunOnce {
    try {
        if (-not (Test-Path -LiteralPath $RunOncePath)) {
            New-Item -Path $RunOncePath -Force | Out-Null
        }
        $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$OrchestratorPath`""
        New-ItemProperty -Path $RunOncePath -Name $RunOnceName -PropertyType String -Value $command -Force | Out-Null
        Write-Log "RunOnce launcher armed for next admin sign-in. Package apply is complete; remaining provisioning tasks stay pending until sign-in."
    } catch {
        Write-Log "Failed to arm RunOnce launcher ${RunOnceName}: $($_.Exception.Message)" 'WARNING'
    }
}

function Remove-ScheduledTaskSafe {
    try {
        schtasks.exe /Delete /TN $ScheduledTaskName /F | Out-Null
    } catch {
    }
}

function Remove-RunOnceSafe {
    try {
        Remove-ItemProperty -Path $RunOncePath -Name $RunOnceName -ErrorAction SilentlyContinue
    } catch {
    }
}

function Launch-Ui {
    $profile = Get-ProvisioningProfile
    Apply-RegionalSettings -Profile $profile
    $bundle = Ensure-StateFiles
    Ensure-ScheduledTask
    Write-Log ("Evaluating UI launch. terminalStatus={0}; currentStep={1}; {2}" -f $bundle.status.terminalStatus, $bundle.state.currentStepId, (Get-HeartbeatSummary))

    if ($bundle.status.terminalStatus -eq 'complete') {
        Remove-ScheduledTaskSafe
        Remove-RunOnceSafe
        Write-Log 'Provisioning already completed; not relaunching kiosk.'
        return
    }

    if (-not (Test-Path -LiteralPath $HtaPath)) {
        throw "Provisioning HTA missing at $HtaPath"
    }

    $existing = Find-ProvisioningUiProcess

    if ($null -ne $existing) {
        if (Test-HeartbeatHealthy -FreshWithinSeconds 20) {
            Write-Log ("Provisioning HTA already running and heartbeat is healthy. pid={0}; {1}" -f $existing.Id, (Get-HeartbeatSummary))
            return
        }

        Write-Log ("Provisioning HTA process exists but heartbeat is stale or missing. pid={0}; recycling host. {1}" -f $existing.Id, (Get-HeartbeatSummary)) 'WARNING'
        Stop-ProvisioningUiProcessSafe -Process $existing -Reason 'stale or missing heartbeat before launch'
    } elseif (Test-Path -LiteralPath $HeartbeatPath) {
        Write-Log ("Removing stale provisioning heartbeat before relaunch. {0}" -f (Get-HeartbeatSummary)) 'WARNING'
        Remove-Item -LiteralPath $HeartbeatPath -Force -ErrorAction SilentlyContinue
    }

    $uiReady = $false
    for ($attempt = 1; $attempt -le 2; $attempt++) {
        if (Start-ProvisioningUiHost -Attempt $attempt) {
            $uiReady = $true
            break
        }
    }

    if (-not $uiReady) {
        throw "Provisioning HTA did not become responsive after launch. Review $ShellLogPath and $LogPath."
    }
}

function Resolve-TargetComputerName {
    param($Profile, $State, $Command)

    if ($Profile.promptForComputerName) {
        return (Validate-ComputerName -Name $Command.computerName)
    }
    if (-not [string]::IsNullOrWhiteSpace($Profile.explicitComputerName)) {
        return (Validate-ComputerName -Name $Profile.explicitComputerName)
    }
    if (-not [string]::IsNullOrWhiteSpace($State.computerName)) {
        return (Validate-ComputerName -Name $State.computerName)
    }
    return $env:COMPUTERNAME
}

function Invoke-ComputerNameStep {
    param($Profile, $State, $Command)

    $targetName = Resolve-TargetComputerName -Profile $Profile -State $State -Command $Command
    Set-Content -LiteralPath (Join-Path $RuntimeScriptsDir 'pcname.txt') -Value $targetName -Encoding ASCII
    $State.computerName = $targetName

    if ($targetName -eq $env:COMPUTERNAME) {
        return [ordered]@{ detail = "Computer already named $targetName."; restart = $false; rebootPending = $false }
    }

    Write-Log "Renaming computer from $env:COMPUTERNAME to $targetName"
    Rename-Computer -NewName $targetName -Force -ErrorAction Stop

    $restartNow = if ($DomainJoinEnabled) { $true } else { [bool]$Command.restartNow }
    $detail = if ($DomainJoinEnabled) {
        "Computer renamed to $targetName. Restarting now so domain join uses the committed computer name."
    } elseif ($restartNow) {
        "Computer renamed to $targetName. Restarting now."
    } else {
        "Computer renamed to $targetName. Reboot pending."
    }
    return [ordered]@{
        detail = $detail
        restart = $restartNow
        rebootPending = $true
    }
}

function Invoke-WifiStep {
    param($Profile, $Command)
    Invoke-BitOSDTScript -Name $WifiScriptName
    $restartNow = [bool]$Command.restartNow
    $detail = if (-not [string]::IsNullOrWhiteSpace($Profile.wifiSsid)) { "Wi-Fi settings applied for $($Profile.wifiSsid)." } else { 'Wi-Fi settings applied.' }
    return [ordered]@{
        detail = $detail
        restart = $restartNow
        rebootPending = $restartNow
    }
}

function Invoke-DomainJoinStep {
    param($Profile, $Command)
    Ensure-Connectivity
    Invoke-BitOSDTScript -Name 'domainjoin.ps1'
    $restartNow = [bool]$Command.restartNow
    $detail = if (-not [string]::IsNullOrWhiteSpace($Profile.domainName)) { "Joined $($Profile.domainName)." } else { 'Domain join script completed.' }
    return [ordered]@{
        detail = $detail
        restart = $restartNow
        rebootPending = $true
    }
}

function Invoke-BitLockerStep {
    Write-Log 'disable-bitlocker.ps1 pending work detected. Starting BitLocker disable step.'
    Invoke-BitOSDTScript -Name '__BITLOCKER_SCRIPT__'
    Write-Log 'disable-bitlocker.ps1 completed. BitLocker disable step finished.'
    return [ordered]@{
        detail = 'BitLocker disable operation completed for drive C:.'
        restart = $BitLockerRebootAfterDisable
        rebootPending = $BitLockerRebootAfterDisable
    }
}

function Invoke-AppsStep {
    param($Command)
    Write-Log 'installapps.ps1 pending work detected. Starting application and file deployment step.'
    Remove-Item -LiteralPath $AppProgressPath -Force -ErrorAction SilentlyContinue
    Invoke-BitOSDTScript -Name 'installapps.ps1'
    Write-Log 'installapps.ps1 completed. Application and file deployment step finished.'
    $restartNow = [bool]$Command.restartNow
    return [ordered]@{
        detail = 'Application installation and payload deployment finished.'
        restart = $restartNow
        rebootPending = $restartNow
    }
}

function Invoke-OptionalScriptsStep {
    param($Command)
    if ($DebloatEnabled) {
        Invoke-BitOSDTScript -Name 'debloat.ps1' -Optional
    }
    if ($CustomScriptsEnabled) {
        Get-ChildItem -LiteralPath $RuntimeScriptsDir -Filter 'custom-*.ps1' -File -ErrorAction SilentlyContinue |
            Sort-Object Name |
            ForEach-Object {
                Invoke-BitOSDTScript -Name $_.Name
            }
    }
    $restartNow = [bool]$Command.restartNow
    return [ordered]@{
        detail = 'Custom actions completed.'
        restart = $restartNow
        rebootPending = $restartNow
    }
}

function Complete-Provisioning {
    param($Status)
    $Status.terminalStatus = 'complete'
    $Status.bannerMessage = 'Provisioning completed successfully. The package-apply and post-sign-in phases are both complete.'
    $Status.errorMessage = $null
    Update-StatusPercent -Status $Status
    Save-Status -Status $Status
    Remove-ScheduledTaskSafe
    Remove-RunOnceSafe
    Write-Log 'Provisioning orchestration completed successfully.' 'SUCCESS'
}

function Process-Command {
    if (-not (Test-Path -LiteralPath $CommandPath)) {
        return
    }

    $command = Read-JsonFile -Path $CommandPath
    Remove-Item -LiteralPath $CommandPath -Force -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        return
    }

    $bundle = Ensure-StateFiles
    $profile = $bundle.profile
    $steps = @($bundle.steps)
    $state = $bundle.state
    $status = $bundle.status

    if ($state.inProgress) {
        Write-Log 'Ignoring UI command because a provisioning action is already running.' 'WARNING'
        return
    }

    $taskId = if (-not [string]::IsNullOrWhiteSpace($command.stepId)) { [string]$command.stepId } else { [string]$state.currentStepId }
    if ([string]::IsNullOrWhiteSpace($taskId) -or $taskId -eq 'complete') {
        return
    }
    Write-Log ("Processing UI command for step {0}; restartNow={1}; currentStep={2}; {3}" -f $taskId, [bool]$command.restartNow, $state.currentStepId, (Get-HeartbeatSummary))

    if ($taskId -ne 'bitLocker' -and $command.PSObject.Properties.Name -contains 'restartNow') {
        $state.restartChoices[$taskId] = [bool]$command.restartNow
    }
    if ($command.PSObject.Properties.Name -contains 'computerName' -and -not [string]::IsNullOrWhiteSpace($command.computerName)) {
        $state.computerName = [string]$command.computerName
    }

    $state.inProgress = $true
    $state.errorMessage = $null
    $state.rebootPending = $false
    Save-UiState -State $state

    $status.terminalStatus = 'running'
    $status.errorMessage = $null
    $status.bannerMessage = ''
    Set-TaskStatus -Status $status -TaskId $taskId -TaskStatusValue 'active' -Detail 'Running'
    Save-Status -Status $status

    try {
        switch ($taskId) {
            'computerName' { $result = Invoke-ComputerNameStep -Profile $profile -State $state -Command $command }
            'wifi' { $result = Invoke-WifiStep -Profile $profile -Command $command }
            'domainJoin' { $result = Invoke-DomainJoinStep -Profile $profile -Command $command }
            'bitLocker' { $result = Invoke-BitLockerStep }
            'apps' { $result = Invoke-AppsStep -Command $command }
            'optionalScripts' { $result = Invoke-OptionalScriptsStep -Command $command }
            default { throw "Unsupported provisioning step: $taskId" }
        }

        if ($state.completedStepIds -notcontains $taskId) {
            $state.completedStepIds += $taskId
        }
        $state.inProgress = $false
        $state.errorMessage = $null
        $state.rebootPending = [bool]$result.rebootPending

        $taskStatusValue = if ($result.rebootPending) { 'reboot_pending' } else { 'complete' }
        Set-TaskStatus -Status $status -TaskId $taskId -TaskStatusValue $taskStatusValue -Detail $result.detail

        $nextStepId = Get-NextStepId -Steps $steps -CurrentStepId $taskId
        if ([string]::IsNullOrWhiteSpace($nextStepId)) {
            $state.currentStepId = 'complete'
            Save-UiState -State $state
            Write-Log ("Completed step {0}; taskStatus={1}; rebootPending={2}; restartNow={3}; nextStep=complete" -f $taskId, $taskStatusValue, [bool]$result.rebootPending, [bool]$result.restart)
            Complete-Provisioning -Status $status
        } else {
            $state.currentStepId = $nextStepId
            $status.terminalStatus = 'idle'
            $status.bannerMessage = if ($result.restart) { 'Restarting now. BitOSDT resumes automatically after sign-in.' } elseif ($result.rebootPending) { 'Reboot pending. You can continue, and BitOSDT will still resume after restart.' } else { '' }
            Save-UiState -State $state
            Save-Status -Status $status
            Write-Log ("Completed step {0}; taskStatus={1}; rebootPending={2}; restartNow={3}; nextStep={4}" -f $taskId, $taskStatusValue, [bool]$result.rebootPending, [bool]$result.restart, $nextStepId)
        }

        if ($result.rebootPending) {
            Ensure-RunOnce
        }

        if ($result.restart) {
            Write-Log "Restart requested after step $taskId"
            Restart-Computer -Force
        }
    } catch {
        $message = $_.Exception.Message
        $state.inProgress = $false
        $state.errorMessage = $message
        Save-UiState -State $state

        $status.terminalStatus = 'failed'
        $status.errorMessage = $message
        $status.bannerMessage = ''
        Set-TaskStatus -Status $status -TaskId $taskId -TaskStatusValue 'failed' -Detail $message
        Save-Status -Status $status

        Write-Log ("Provisioning orchestration failed during step {0}: {1}; {2}" -f $taskId, $message, (Get-HeartbeatSummary)) 'ERROR'
        throw
    }
}

try {
    switch ($Action) {
        'ProcessCommand' { Process-Command }
        default { Launch-Ui }
    }
} catch {
    Write-Log "Provisioning UI host failed: $($_.Exception.Message)" 'ERROR'
    try {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show(
            "BitOSDT provisioning failed: $($_.Exception.Message)`nSee $LogPath for details.",
            'BitOSDT Provisioning',
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Error
        ) | Out-Null
    } catch {
    }
    throw
}
"#
    .to_string();

    let replacements = [
        ("__RUNTIME_SCRIPTS_DIR__", BITOSDT_RUNTIME_SCRIPTS_DIR),
        ("__ORCHESTRATOR_SCRIPT__", PROVISIONING_ORCHESTRATOR_SCRIPT),
        ("__HTA_FILE__", PROVISIONING_UI_HTA_FILE),
        ("__KIOSK_HELPER_FILE__", PROVISIONING_KIOSK_HELPER_FILE),
        ("__PROFILE_SNAPSHOT_FILE__", PROVISIONING_UI_PROFILE_FILE),
        ("__LOG_PATH__", BITOSDT_PROVISIONING_UI_SESSION_LOG_PATH),
        ("__SHELL_LOG_PATH__", BITOSDT_PROVISIONING_UI_SHELL_LOG_PATH),
        ("__RUNONCE_NAME__", PROVISIONING_RUNONCE_NAME),
        ("__SCHEDULED_TASK_NAME__", PROVISIONING_SCHEDULED_TASK_NAME),
        ("__PROFILE_PATH__", BITOSDT_PROVISIONING_UI_PROFILE_PATH),
        ("__UI_STATE_PATH__", BITOSDT_PROVISIONING_UI_STATE_PATH),
        ("__STATUS_PATH__", BITOSDT_PROVISIONING_UI_STATUS_PATH),
        ("__COMMAND_PATH__", BITOSDT_PROVISIONING_UI_COMMAND_PATH),
        (
            "__APP_PROGRESS_PATH__",
            BITOSDT_PROVISIONING_UI_APP_PROGRESS_PATH,
        ),
        ("__HEARTBEAT_PATH__", BITOSDT_PROVISIONING_UI_HEARTBEAT_PATH),
        ("__STATE_DIR__", BITOSDT_PROVISIONING_UI_STATE_DIR),
        ("__EXPLICIT_NAME__", &explicit_name),
        ("__REGIONAL_LANGUAGE__", &regional_language),
        ("__REGIONAL_INPUT_LOCALE__", &regional_input_locale),
        ("__REGIONAL_TIMEZONE__", &regional_timezone),
        ("__WIFI_SCRIPT__", PROVISIONING_WIFI_SCRIPT),
        (
            "__PROMPT_FOR_NAME__",
            if prompt_for_computer_name {
                "$true"
            } else {
                "$false"
            },
        ),
        (
            "__DOMAIN_JOIN_ENABLED__",
            if domain_join_enabled {
                "$true"
            } else {
                "$false"
            },
        ),
        (
            "__WIFI_ENABLED__",
            if wifi_enabled { "$true" } else { "$false" },
        ),
        (
            "__BITLOCKER_ENABLED__",
            if bitlocker_enabled { "$true" } else { "$false" },
        ),
        (
            "__BITLOCKER_REBOOT_AFTER_DISABLE__",
            if bitlocker_reboot_after_disable {
                "$true"
            } else {
                "$false"
            },
        ),
        (
            "__INSTALL_APPS_ENABLED__",
            if has_install_apps_script {
                "$true"
            } else {
                "$false"
            },
        ),
        (
            "__DEBLOAT_ENABLED__",
            if run_debloat { "$true" } else { "$false" },
        ),
        (
            "__CUSTOM_SCRIPTS_ENABLED__",
            if custom_script_enabled {
                "$true"
            } else {
                "$false"
            },
        ),
        ("__BITLOCKER_SCRIPT__", PROVISIONING_BITLOCKER_SCRIPT),
    ];

    for (token, value) in replacements {
        script = script.replace(token, value);
    }

    script
}

fn default_debloat_script() -> String {
    [
        "$ErrorActionPreference = 'Continue'",
        "Write-Host 'Running BitOSDT debloat script...'",
        "$apps = @(",
        "  'Microsoft.BingNews',",
        "  'Microsoft.BingWeather',",
        "  'Microsoft.GetHelp',",
        "  'Microsoft.Getstarted',",
        "  'Microsoft.Microsoft3DViewer',",
        "  'Microsoft.People',",
        "  'Microsoft.SkypeApp',",
        "  'Microsoft.XboxApp',",
        "  'Microsoft.XboxGameOverlay',",
        "  'Microsoft.XboxGamingOverlay',",
        "  'Microsoft.XboxIdentityProvider',",
        "  'Microsoft.ZuneMusic',",
        "  'Microsoft.ZuneVideo'",
        ")",
        "foreach ($app in $apps) {",
        "  try {",
        "    Get-AppxPackage -Name $app -AllUsers | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue",
        "    Get-AppxProvisionedPackage -Online | Where-Object { $_.DisplayName -eq $app } | Remove-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue | Out-Null",
        "  } catch {",
        "    Write-Host \"Failed to remove ${app}: $_\"",
        "  }",
        "}",
    ]
    .join("\n")
}

fn build_custom_script_filename(order: usize, name: &str) -> String {
    let mut safe = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if safe.is_empty() {
        safe = format!("script{}", order + 1);
    }

    format!(
        "custom-{order:02}-{safe}.ps1",
        order = order + 1,
        safe = safe
    )
}

fn render_plaintext_domain_join_password(
    xml: &str,
    plain_password: &str,
) -> Result<String, String> {
    let component_marker = "Microsoft-Windows-UnattendedJoin";
    let component_start = xml
        .find(component_marker)
        .ok_or_else(|| "Domain join component not found in generated XML.".to_string())?;

    let component_open_start = xml[..component_start]
        .rfind("<component")
        .ok_or_else(|| "Domain join component start tag not found.".to_string())?;
    let component_end = xml[component_start..]
        .find("</component>")
        .map(|offset| component_start + offset + "</component>".len())
        .ok_or_else(|| "Domain join component end tag not found.".to_string())?;

    let component_xml = &xml[component_open_start..component_end];
    let pwd_start_rel = component_xml
        .find("<Password>")
        .ok_or_else(|| "Domain join password tag not found.".to_string())?;
    let pwd_end_rel = component_xml
        .find("</Password>")
        .ok_or_else(|| "Domain join password closing tag not found.".to_string())?;

    let pwd_value_start = component_open_start + pwd_start_rel + "<Password>".len();
    let pwd_value_end = component_open_start + pwd_end_rel;

    let mut updated = String::with_capacity(xml.len() + plain_password.len());
    updated.push_str(&xml[..pwd_value_start]);
    updated.push_str(&escape_xml(plain_password));
    updated.push_str(&xml[pwd_value_end..]);
    Ok(updated)
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn has_app_work(apps: &OobeAppsConfig) -> bool {
    !apps.copied_items.is_empty()
        || apps.winget_packages.iter().any(|pkg| pkg.enabled)
        || apps.chocolatey_packages.iter().any(|pkg| pkg.enabled)
        || apps
            .custom_installers
            .iter()
            .any(|installer| installer.enabled)
}

fn build_disable_bitlocker_script() -> String {
    r#"$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $logPath = 'C:\BitOSDT\Logs\bitlocker-disable.log'
    $directory = Split-Path -Path $logPath -Parent
    if (-not (Test-Path -LiteralPath $directory)) {
        New-Item -Path $directory -ItemType Directory -Force | Out-Null
    }
    $line = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') [$Level] $Message"
    $line | Out-File -FilePath $logPath -Encoding utf8 -Append
}

$manageBde = Get-Command -Name 'manage-bde.exe' -ErrorAction SilentlyContinue
if ($null -eq $manageBde) {
    throw 'manage-bde.exe is not available on this system.'
}

Write-Log 'Checking BitLocker state for drive C:.'
$statusOutput = & $manageBde.Source -status C: 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
    throw "manage-bde -status C: failed with exit code $LASTEXITCODE. Output: $statusOutput"
}

if ($statusOutput -match 'Protection Status:\s+Protection Off') {
    Write-Log 'BitLocker protection is already off for drive C:.' 'SUCCESS'
    return
}

if ($statusOutput -match 'Conversion Status:\s+Decryption in Progress') {
    Write-Log 'BitLocker decryption is already in progress for drive C:.' 'SUCCESS'
    return
}

Write-Log 'Disabling BitLocker for drive C:.'
$disableOutput = & $manageBde.Source -off C: 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
    throw "manage-bde -off C: failed with exit code $LASTEXITCODE. Output: $disableOutput"
}

Write-Log 'manage-bde -off C: completed successfully.' 'SUCCESS'
"#
    .to_string()
}

#[derive(Debug, Clone)]
struct GeneratedScriptPayload {
    app_script_needed: bool,
    custom_script_files: Vec<String>,
}

#[derive(Debug, Clone)]
struct LocalPayloadStagePlan {
    source_path: String,
    entry_name: String,
    source_kind: bitosdt::tasks::LocalPayloadKind,
}

#[derive(Debug, Clone)]
struct EmbeddedInstallerStagePlan {
    source_path: String,
    staged_name: String,
}

fn is_script_payload_trigger(trigger_mode: &TriggerMode) -> bool {
    matches!(
        trigger_mode,
        TriggerMode::FirstLogonUsbScan | TriggerMode::ProvisioningPackage
    )
}

fn normalize_media_installer_runtime_path(path: &str) -> Option<String> {
    let normalized = path.trim().replace('/', "\\");
    let bytes = normalized.as_bytes();
    if bytes.len() < 7 || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }

    let remainder = &normalized[3..];
    let lower = remainder.to_ascii_lowercase();
    if lower == "apps" {
        return Some(BITOSDT_RUNTIME_APPS_DIR.to_string());
    }
    if !lower.starts_with("apps\\") {
        return None;
    }

    Some(format!(r"{}\{}", BITOSDT_RUNTIME_APPS_DIR, &remainder[5..]))
}

fn payload_leaf_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
}

fn unique_sidecar_entry_name(name: &str, used_names: &mut HashSet<String>) -> String {
    let mut candidate = name.to_string();

    while used_names.contains(&candidate.to_ascii_lowercase()) {
        let stem = Path::new(&candidate)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "payload".to_string());
        let ext = Path::new(&candidate)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        candidate = if ext.is_empty() {
            format!("{stem}-copy")
        } else {
            format!("{stem}-copy.{ext}")
        };
    }

    used_names.insert(candidate.to_ascii_lowercase());
    candidate
}

fn build_local_payload_stage_plan(
    request: &OobeProfileRequest,
) -> Result<Vec<LocalPayloadStagePlan>, String> {
    let mut plans = Vec::new();
    let mut used_names = HashSet::new();
    let mut seen_sources = HashSet::new();

    let payloads = request.apps.copied_items.iter().chain(
        request
            .apps
            .custom_installers
            .iter()
            .filter(|installer| installer.enabled)
            .flat_map(|installer| installer.dependencies.iter()),
    );

    for payload in payloads {
        let source_key = payload.source_path.trim();
        if source_key.is_empty() || !seen_sources.insert(source_key.to_ascii_lowercase()) {
            continue;
        }

        let entry_name = payload_leaf_name(source_key)
            .ok_or_else(|| format!("Invalid payload path: {}", source_key))?;
        let entry_name = unique_sidecar_entry_name(&entry_name, &mut used_names);
        plans.push(LocalPayloadStagePlan {
            source_path: source_key.to_string(),
            entry_name,
            source_kind: map_local_payload_kind(&payload.source_kind),
        });
    }

    Ok(plans)
}

fn build_local_payload_runtime_path_map(
    request: &OobeProfileRequest,
) -> Result<HashMap<String, String>, String> {
    let mut runtime_path_map = HashMap::new();
    for plan in build_local_payload_stage_plan(request)? {
        runtime_path_map.insert(
            plan.source_path,
            format!(r"{}\{}", BITOSDT_RUNTIME_FILES_DIR, plan.entry_name),
        );
    }
    Ok(runtime_path_map)
}

fn build_embedded_installer_stage_plan(
    request: &OobeProfileRequest,
) -> Result<Vec<EmbeddedInstallerStagePlan>, String> {
    let mut plans = Vec::new();
    let mut used_names = HashSet::new();

    for installer in request.apps.custom_installers.iter().filter(|i| i.enabled) {
        let source_type = map_custom_installer_source_type(installer.source_type.as_deref());
        if source_type != bitosdt::tasks::InstallerSourceType::EmbeddedFile {
            continue;
        }

        let source = PathBuf::from(installer.path.trim());
        let mut file_name = source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| format!("Invalid embedded installer path: {}", source.display()))?;

        while used_names.contains(&file_name.to_ascii_lowercase()) {
            let stem = Path::new(&file_name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "installer".to_string());
            let ext = Path::new(&file_name)
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            file_name = if ext.is_empty() {
                format!("{stem}-copy")
            } else {
                format!("{stem}-copy.{ext}")
            };
        }
        used_names.insert(file_name.to_ascii_lowercase());

        plans.push(EmbeddedInstallerStagePlan {
            source_path: installer.path.clone(),
            staged_name: file_name,
        });
    }

    Ok(plans)
}

fn build_embedded_installer_runtime_path_map(
    request: &OobeProfileRequest,
) -> Result<HashMap<String, String>, String> {
    let mut runtime_path_map = HashMap::new();
    for plan in build_embedded_installer_stage_plan(request)? {
        runtime_path_map.insert(
            plan.source_path,
            format!(r"{}\{}", BITOSDT_RUNTIME_APPS_DIR, plan.staged_name),
        );
    }
    Ok(runtime_path_map)
}

fn copy_directory_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| {
        format!(
            "Failed to create payload directory {}: {}",
            destination.display(),
            e
        )
    })?;

    for entry in fs::read_dir(source).map_err(|e| {
        format!(
            "Failed to read payload directory {}: {}",
            source.display(),
            e
        )
    })? {
        let entry = entry.map_err(|e| {
            format!(
                "Failed to enumerate payload directory {}: {}",
                source.display(),
                e
            )
        })?;
        let path = entry.path();
        let next_destination = destination.join(entry.file_name());
        if path.is_dir() {
            copy_payload_directory_recursive(&path, &next_destination)?;
        } else if path.is_file() {
            fs::copy(&path, &next_destination).map_err(|e| {
                format!(
                    "Failed to copy payload file {} to {}: {}",
                    path.display(),
                    next_destination.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

fn copy_local_payloads_to_sidecar(
    request: &OobeProfileRequest,
    files_dir: &Path,
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut runtime_path_map = HashMap::new();

    for plan in build_local_payload_stage_plan(request)? {
        let source = PathBuf::from(&plan.source_path);
        let destination = files_dir.join(&plan.entry_name);

        match plan.source_kind {
            bitosdt::tasks::LocalPayloadKind::Directory => {
                if !source.is_dir() {
                    return Err(format!(
                        "Payload directory does not exist: {}",
                        source.display()
                    ));
                }
                copy_payload_directory_recursive(&source, &destination)?;
            }
            bitosdt::tasks::LocalPayloadKind::File => {
                if !source.is_file() {
                    return Err(format!("Payload file does not exist: {}", source.display()));
                }
                fs::copy(&source, &destination).map_err(|e| {
                    format!(
                        "Failed to copy payload {} to {}: {}",
                        source.display(),
                        destination.display(),
                        e
                    )
                })?;
            }
        }

        runtime_path_map.insert(
            plan.source_path,
            format!(r"{}\{}", BITOSDT_RUNTIME_FILES_DIR, plan.entry_name),
        );
    }

    Ok(runtime_path_map)
}

fn collect_app_runtime_path_overrides(
    request: &OobeProfileRequest,
    embedded_runtime_paths: &HashMap<String, String>,
    effective_trigger_mode: &TriggerMode,
) -> HashMap<String, String> {
    let mut overrides = embedded_runtime_paths.clone();
    if !is_script_payload_trigger(effective_trigger_mode) {
        return overrides;
    }

    for installer in request.apps.custom_installers.iter().filter(|i| i.enabled) {
        let source_type = map_custom_installer_source_type(installer.source_type.as_deref());
        if source_type == bitosdt::tasks::InstallerSourceType::NetworkDirectory {
            continue;
        }
        if overrides.contains_key(&installer.path) {
            continue;
        }
        if let Some(path) = normalize_media_installer_runtime_path(&installer.path) {
            overrides.insert(installer.path.clone(), path);
        }
    }

    overrides
}

fn map_app_install_config(
    request: &OobeProfileRequest,
    app_runtime_paths: &HashMap<String, String>,
) -> bitosdt::tasks::AppInstallConfig {
    bitosdt::tasks::AppInstallConfig {
        copied_items: request
            .apps
            .copied_items
            .iter()
            .map(|item| bitosdt::tasks::LocalPayloadItem {
                source_path: app_runtime_paths
                    .get(item.source_path.trim())
                    .cloned()
                    .unwrap_or_else(|| item.source_path.trim().to_string()),
                source_kind: map_local_payload_kind(&item.source_kind),
                display_name: item.display_name.clone(),
            })
            .collect(),
        copy_destination: request.apps.copy_destination.clone(),
        winget_packages: request
            .apps
            .winget_packages
            .iter()
            .map(|p| bitosdt::tasks::WingetPackage {
                package_id: p.package_id.trim().to_string(),
                version: p.version.clone(),
                custom_args: p.custom_args.clone(),
                enabled: p.enabled && !p.package_id.trim().is_empty(),
            })
            .collect(),
        chocolatey_packages: request
            .apps
            .chocolatey_packages
            .iter()
            .map(|p| bitosdt::tasks::ChocolateyPackage {
                package_name: p.package_name.trim().to_string(),
                version: p.version.clone(),
                source: p.source.clone(),
                custom_args: p.custom_args.clone(),
                enabled: p.enabled && !p.package_name.trim().is_empty(),
            })
            .collect(),
        custom_installers: request
            .apps
            .custom_installers
            .iter()
            .map(|p| {
                let path = app_runtime_paths
                    .get(&p.path)
                    .cloned()
                    .unwrap_or_else(|| p.path.trim().to_string());
                bitosdt::tasks::CustomInstaller {
                    name: p.name.trim().to_string(),
                    path,
                    source_type: map_custom_installer_source_type(p.source_type.as_deref()),
                    source_file_name: p.source_file_name.clone(),
                    dependencies: p
                        .dependencies
                        .iter()
                        .map(|item| bitosdt::tasks::LocalPayloadItem {
                            source_path: app_runtime_paths
                                .get(item.source_path.trim())
                                .cloned()
                                .unwrap_or_else(|| item.source_path.trim().to_string()),
                            source_kind: map_local_payload_kind(&item.source_kind),
                            display_name: item.display_name.clone(),
                        })
                        .collect(),
                    dependency_destination: p.dependency_destination.clone(),
                    silent_args: p.silent_args.clone(),
                    installer_type: map_custom_installer_type(&p.installer_type),
                    success_codes: vec![0, 3010],
                    enabled: p.enabled && !p.name.trim().is_empty() && !p.path.trim().is_empty(),
                }
            })
            .collect(),
        auto_install_chocolatey: request.apps.auto_install_chocolatey,
        continue_on_error: request.apps.continue_on_error,
        log_path: "C:\\BitOSDT\\Logs\\app-install.log".to_string(),
        progress_json_path: None,
    }
}

fn copy_embedded_installers(
    request: &OobeProfileRequest,
    apps_dir: &Path,
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut runtime_path_map = HashMap::new();

    for plan in build_embedded_installer_stage_plan(request)? {
        let source = PathBuf::from(plan.source_path.trim());
        if !source.is_file() {
            return Err(format!(
                "Embedded installer file does not exist: {}",
                source.display()
            ));
        }

        let destination = apps_dir.join(&plan.staged_name);
        fs::copy(&source, &destination).map_err(|e| {
            format!(
                "Failed to copy embedded installer {} to {}: {}",
                source.display(),
                destination.display(),
                e
            )
        })?;

        runtime_path_map.insert(
            plan.source_path,
            format!(r"{}\{}", BITOSDT_RUNTIME_APPS_DIR, plan.staged_name),
        );
    }

    Ok(runtime_path_map)
}
fn build_first_logon_commands(
    request: &OobeProfileRequest,
    profile_name: &str,
    _has_install_apps_script: bool,
    _custom_script_files: &[String],
) -> Vec<bitosdt::config::FirstLogonCommand> {
    if request.trigger_mode != TriggerMode::FirstLogonUsbScan {
        return Vec::new();
    }

    vec![bitosdt::config::FirstLogonCommand {
        order: 1,
        description: format!("Bootstrap USB OOBE payload for {}", profile_name),
        command_line: make_profile_source_locator(profile_name),
        require_input: false,
    }]
}

fn write_custom_scripts(
    request: &OobeProfileRequest,
    scripts_dir: &Path,
) -> Result<Vec<String>, String> {
    if !request.apps.enable_custom_scripts {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for (index, script) in request.apps.custom_scripts.iter().enumerate() {
        if !script.enabled || script.content.trim().is_empty() {
            continue;
        }

        let file_name = build_custom_script_filename(index, &script.name);
        let target = scripts_dir.join(&file_name);
        fs::write(&target, &script.content)
            .map_err(|e| format!("Failed to write custom script {}: {}", target.display(), e))?;
        files.push(file_name);
    }

    Ok(files)
}

fn map_domain_join_for_unattend(
    request: &OobeProfileRequest,
) -> Option<bitosdt::config::DomainJoinConfig> {
    if !request.domain_join.enabled || request.domain_join_mode != DomainJoinMode::SpecializeXml {
        return None;
    }

    Some(bitosdt::config::DomainJoinConfig {
        domain: request.domain_join.domain.trim().to_string(),
        username: request.domain_join.username.trim().to_string(),
        password: request.domain_join.password.clone(),
        ou_path: request.domain_join.ou_path.clone(),
        machine_object_ou: request.domain_join.ou_path.clone(),
    })
}

fn map_default_user_for_unattend(
    default_user: &DefaultUserUiConfig,
) -> Vec<bitosdt::config::UserAccountConfig> {
    if !default_user.enabled {
        return Vec::new();
    }

    vec![bitosdt::config::UserAccountConfig {
        username: default_user.username.trim().to_string(),
        password: default_user.password.clone(),
        display_name: Some(default_user.username.trim().to_string()),
        group: map_user_group(default_user.group.as_str()),
        password_never_expires: true,
        require_password_change: false,
    }]
}

fn map_wifi_profile_for_unattend(
    wifi: &OobeWifiConfig,
) -> Option<bitosdt::config::WifiProfileConfig> {
    if !wifi.enabled {
        return None;
    }

    let authentication = map_wifi_authentication(&wifi.authentication);
    let encryption = if authentication == bitosdt::config::WifiAuthentication::Open {
        bitosdt::config::WifiEncryption::None
    } else {
        map_wifi_encryption(&wifi.encryption)
    };
    let password = if authentication == bitosdt::config::WifiAuthentication::Open {
        String::new()
    } else {
        wifi.password.clone()
    };

    Some(bitosdt::config::WifiProfileConfig {
        ssid: wifi.ssid.trim().to_string(),
        password,
        authentication,
        encryption,
        auto_connect: wifi.auto_connect,
        hidden_network: wifi.hidden_network,
    })
}

fn profile_updated_timestamp(path: &Path) -> String {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(chrono::DateTime::<Utc>::from)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339())
}

fn write_setup_unattend_readme(staging_dir: &Path, profile_name: &str) -> Result<(), String> {
    let content = format!(
        "BitOSDT SetupUnattend Trigger Mode\n\nProfile: {profile_name}\n\nExecution timing:\n- Setup phase only (Windows Setup / specialize / oobeSystem passes).\n- No FirstLogon USB scan commands are generated in this mode.\n\nRequired media layout:\n- Place Autounattend.xml in the root of the installation USB/DVD media.\n- Keep Scripts/ and Apps/ folders beside Autounattend.xml at the media root if your unattend references payload scripts.\n\nSetup requirements:\n- Boot from installation media containing this profile output.\n- Ensure removable media remains connected through initial setup phases.\n- If you need post-logon payload execution, use FirstLogonUsbScan trigger mode instead.\n"
    );

    fs::write(staging_dir.join("SETUP-UNATTEND-README.txt"), content)
        .map_err(|e| format!("Failed to write SETUP-UNATTEND-README.txt: {}", e))
}

pub(crate) fn build_provisioning_bootstrap_script(
    expected_package_name: Option<&str>,
    hide_privacy_settings: bool,
) -> String {
    let expected_package_name = escape_ps_single_quoted(expected_package_name.unwrap_or_default());
    [
        "# BitOSDT Provisioning Package bootstrap".to_string(),
        "$ErrorActionPreference = 'Stop'".to_string(),
        "$ProgressPreference = 'SilentlyContinue'".to_string(),
        format!(
            "$HidePrivacySettings = {}",
            if hide_privacy_settings {
                "$true"
            } else {
                "$false"
            }
        ),
        "$logPath = 'C:\\BitOSDT\\Logs\\provisioning-bootstrap.log'".to_string(),
        "$logDir = Split-Path -Path $logPath -Parent".to_string(),
        "if (-not (Test-Path -LiteralPath $logDir)) { New-Item -Path $logDir -ItemType Directory -Force | Out-Null }".to_string(),
        "function Write-Log {".to_string(),
        "  param([string]$Message, [string]$Level = 'INFO')".to_string(),
        "  $line = \"$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') [$Level] $Message\"".to_string(),
        "  $line | Out-File -FilePath $logPath -Encoding utf8 -Append".to_string(),
        "}".to_string(),
        "function Set-PrivacyExperiencePolicy {".to_string(),
        "  $policyPath = 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\OOBE'".to_string(),
        "  if (-not (Test-Path -LiteralPath $policyPath)) { New-Item -Path $policyPath -Force | Out-Null }".to_string(),
        "  if ($HidePrivacySettings) {".to_string(),
        "    New-ItemProperty -Path $policyPath -Name 'DisablePrivacyExperience' -PropertyType DWord -Value 1 -Force | Out-Null".to_string(),
        "    Write-Log 'Configured DisablePrivacyExperience=1 to suppress the privacy settings screen.'".to_string(),
        "  } else {".to_string(),
        "    Remove-ItemProperty -Path $policyPath -Name 'DisablePrivacyExperience' -ErrorAction SilentlyContinue".to_string(),
        "    Write-Log 'Privacy settings screen suppression disabled for this profile.'".to_string(),
        "  }".to_string(),
        "}".to_string(),
        "function Get-StagedItemCount {".to_string(),
        "  param([string]$Path)".to_string(),
        "  if (-not (Test-Path -LiteralPath $Path)) { return 0 }".to_string(),
        "  try { return @(Get-ChildItem -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue).Count } catch { return 0 }".to_string(),
        "}".to_string(),
        "function Resolve-BitOSDTProvisioningMediaRoot {".to_string(),
        format!("  $expectedPackageName = '{}'", expected_package_name),
        "  foreach ($drive in (Get-PSDrive -PSProvider FileSystem)) {".to_string(),
        "    if ($drive.Root -like 'C:*') { continue }".to_string(),
        "    $candidateScripts = Join-Path $drive.Root 'Scripts'".to_string(),
        "    if (-not (Test-Path -LiteralPath $candidateScripts)) { continue }".to_string(),
        "    if ([string]::IsNullOrWhiteSpace($expectedPackageName)) {".to_string(),
        "      $ppkgs = Get-ChildItem -LiteralPath $drive.Root -Filter '*.ppkg' -File -ErrorAction SilentlyContinue".to_string(),
        "      if ($ppkgs) { return $drive.Root.TrimEnd('\\') }".to_string(),
        "      continue".to_string(),
        "    }".to_string(),
        "    $candidatePackage = Join-Path $drive.Root $expectedPackageName".to_string(),
        "    if (Test-Path -LiteralPath $candidatePackage) { return $drive.Root.TrimEnd('\\') }".to_string(),
        "  }".to_string(),
        "  return $null".to_string(),
        "}".to_string(),
        "Write-Log 'BitOSDT provisioning bootstrap started.'".to_string(),
        "Set-PrivacyExperiencePolicy".to_string(),
        format!("$scriptsTarget = '{}'", BITOSDT_RUNTIME_SCRIPTS_DIR),
        format!("$appsTarget = '{}'", BITOSDT_RUNTIME_APPS_DIR),
        format!("$filesTarget = '{}'", BITOSDT_RUNTIME_FILES_DIR),
        format!("$runtimeRoot = '{}'", BITOSDT_RUNTIME_ROOT),
        "$stateDir = 'C:\\ProgramData\\BitOSDT'".to_string(),
        "New-Item -Path $runtimeRoot -ItemType Directory -Force | Out-Null".to_string(),
        "New-Item -Path $scriptsTarget -ItemType Directory -Force | Out-Null".to_string(),
        "New-Item -Path $appsTarget -ItemType Directory -Force | Out-Null".to_string(),
        "New-Item -Path $filesTarget -ItemType Directory -Force | Out-Null".to_string(),
        "New-Item -Path $stateDir -ItemType Directory -Force | Out-Null".to_string(),
        "$mediaRoot = Resolve-BitOSDTProvisioningMediaRoot".to_string(),
        "if (-not [string]::IsNullOrWhiteSpace($mediaRoot)) {".to_string(),
        "  Write-Log (\"Using provisioning sidecar media root: {0}\" -f $mediaRoot)".to_string(),
        "  $scriptsPath = Join-Path $mediaRoot 'Scripts'".to_string(),
        "  if (-not (Test-Path -LiteralPath $scriptsPath)) { throw ('Provisioning sidecar Scripts directory is missing: ' + $scriptsPath + '. Remediation: copy the .ppkg together with sibling Scripts, Apps, and Files folders.') }".to_string(),
        "  Copy-Item -Path (Join-Path $scriptsPath '*') -Destination $scriptsTarget -Recurse -Force".to_string(),
        "  $appsPath = Join-Path $mediaRoot 'Apps'".to_string(),
        "  if (Test-Path -LiteralPath $appsPath) { Copy-Item -Path (Join-Path $appsPath '*') -Destination $appsTarget -Recurse -Force }".to_string(),
        "  $filesPath = Join-Path $mediaRoot 'Files'".to_string(),
        "  if (Test-Path -LiteralPath $filesPath) { Copy-Item -Path (Join-Path $filesPath '*') -Destination $filesTarget -Recurse -Force }".to_string(),
        "} else {".to_string(),
        "  $scriptRoot = $PSScriptRoot".to_string(),
        "  if ([string]::IsNullOrWhiteSpace($scriptRoot)) { $scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path }".to_string(),
        "  if ([string]::IsNullOrWhiteSpace($scriptRoot)) { throw 'Unable to resolve provisioning script root. Run Apply-BitOSDTProvisioning.ps1 from file context.' }".to_string(),
        "  Write-Log (\"Using package-local provisioning root: {0}\" -f $scriptRoot)".to_string(),
        "  $contentRoot = Split-Path -Parent $scriptRoot".to_string(),
        "  $directScriptSources = @($scriptRoot) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -Unique".to_string(),
        "  foreach ($source in $directScriptSources) {".to_string(),
        "    Get-ChildItem -LiteralPath $source -File -ErrorAction SilentlyContinue | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $scriptsTarget $_.Name) -Force }".to_string(),
        "  }".to_string(),
        "  $nestedScriptSources = @((Join-Path $scriptRoot 'Scripts')) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -Unique".to_string(),
        "  foreach ($source in $nestedScriptSources) {".to_string(),
        "    Copy-Item -Path (Join-Path $source '*') -Destination $scriptsTarget -Recurse -Force -ErrorAction SilentlyContinue".to_string(),
        "  }".to_string(),
        "  $appSources = @((Join-Path $contentRoot 'Apps'), (Join-Path $scriptRoot 'Apps')) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -Unique".to_string(),
        "  foreach ($source in $appSources) { Copy-Item -Path (Join-Path $source '*') -Destination $appsTarget -Recurse -Force -ErrorAction SilentlyContinue }".to_string(),
        "  $fileSources = @((Join-Path $contentRoot 'Files'), (Join-Path $scriptRoot 'Files')) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -Unique".to_string(),
        "  foreach ($source in $fileSources) { Copy-Item -Path (Join-Path $source '*') -Destination $filesTarget -Recurse -Force -ErrorAction SilentlyContinue }".to_string(),
        "  Get-ChildItem -LiteralPath $scriptRoot -File -ErrorAction SilentlyContinue | Where-Object { @('.ps1', '.psm1', '.psd1') -notcontains $_.Extension.ToLowerInvariant() } | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $appsTarget $_.Name) -Force }".to_string(),
        "}".to_string(),
        "$scriptsCount = Get-StagedItemCount -Path $scriptsTarget".to_string(),
        "$appsCount = Get-StagedItemCount -Path $appsTarget".to_string(),
        "$filesCount = Get-StagedItemCount -Path $filesTarget".to_string(),
        "Write-Log (\"Staged runtime payloads. scripts={0}; apps={1}; files={2}\" -f $scriptsCount, $appsCount, $filesCount)".to_string(),
        "$installAppsPath = Join-Path $scriptsTarget 'installapps.ps1'".to_string(),
        "if (Test-Path -LiteralPath $installAppsPath) {".to_string(),
        "  Write-Log 'installapps.ps1 staged and pending first admin sign-in orchestration.'".to_string(),
        "} else {".to_string(),
        "  Write-Log 'installapps.ps1 not present; no application phase is pending after package apply.'".to_string(),
        "}".to_string(),
        format!("$orchestratorPath = Join-Path $scriptsTarget '{}'", PROVISIONING_ORCHESTRATOR_SCRIPT),
        "if (-not (Test-Path -LiteralPath $orchestratorPath)) { throw \"Provisioning orchestrator script missing at $orchestratorPath\" }".to_string(),
        "$runOncePath = 'HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce'".to_string(),
        "if (-not (Test-Path -LiteralPath $runOncePath)) { New-Item -Path $runOncePath -Force | Out-Null }".to_string(),
        "New-ItemProperty -Path $runOncePath -Name 'BitOSDTProvisioning' -PropertyType String -Value (\"powershell.exe -NoProfile -ExecutionPolicy Bypass -File `\"{0}`\"\" -f $orchestratorPath) -Force | Out-Null".to_string(),
        "Write-Log 'Bootstrap completed. Package apply is complete and post-sign-in provisioning will resume at the next administrator sign-in.' 'SUCCESS'".to_string(),
    ]
    .join("\n")
}

fn stage_request_shared_script_payload(
    request: &OobeProfileRequest,
    staging_dir: &Path,
    effective_trigger_mode: &TriggerMode,
) -> Result<GeneratedScriptPayload, String> {
    let scripts_dir = staging_dir.join("Scripts");
    fs::create_dir_all(&scripts_dir)
        .map_err(|e| format!("Failed to create scripts directory: {}", e))?;

    let embedded_runtime_paths = build_embedded_installer_runtime_path_map(request)?;
    let staged_payload_runtime_paths = build_local_payload_runtime_path_map(request)?;
    let mut app_runtime_paths = collect_app_runtime_path_overrides(
        request,
        &embedded_runtime_paths,
        effective_trigger_mode,
    );
    app_runtime_paths.extend(staged_payload_runtime_paths);

    let mut app_install_config = map_app_install_config(request, &app_runtime_paths);
    let disable_bitlocker_script_needed = *effective_trigger_mode
        == TriggerMode::ProvisioningPackage
        && request.apps.disable_bitlocker;
    let app_script_needed = has_app_work(&request.apps);
    if *effective_trigger_mode == TriggerMode::ProvisioningPackage {
        app_install_config.progress_json_path =
            Some(BITOSDT_PROVISIONING_UI_APP_PROGRESS_PATH.to_string());
    }

    if disable_bitlocker_script_needed {
        fs::write(
            scripts_dir.join(PROVISIONING_BITLOCKER_SCRIPT),
            build_disable_bitlocker_script(),
        )
        .map_err(|e| format!("Failed to write {}: {}", PROVISIONING_BITLOCKER_SCRIPT, e))?;
    }

    if app_script_needed {
        let script = bitosdt::tasks::AppInstaller::generate_install_script(&app_install_config)
            .map_err(|e| format!("Failed to generate installapps.ps1: {}", e))?;
        fs::write(scripts_dir.join("installapps.ps1"), script)
            .map_err(|e| format!("Failed to write installapps.ps1: {}", e))?;
    }

    let custom_script_files = write_custom_scripts(request, &scripts_dir)?;

    if request.enable_debloat {
        let script = if request.debloat_script_content.trim().is_empty() {
            default_debloat_script()
        } else {
            request.debloat_script_content.clone()
        };
        fs::write(scripts_dir.join("debloat.ps1"), script)
            .map_err(|e| format!("Failed to write debloat.ps1: {}", e))?;
    }

    if request.domain_join.enabled
        && ((*effective_trigger_mode == TriggerMode::FirstLogonUsbScan
            && request.domain_join_mode == DomainJoinMode::PostRenameScript)
            || (*effective_trigger_mode == TriggerMode::ProvisioningPackage
                && provisioning_post_signin_domain_join_required(request)))
    {
        fs::write(
            scripts_dir.join("domainjoin.ps1"),
            build_post_rename_domain_join_script(&request.domain_join, false),
        )
        .map_err(|e| format!("Failed to write domainjoin.ps1: {}", e))?;
    }

    if (*effective_trigger_mode == TriggerMode::FirstLogonUsbScan && request.wifi.enabled)
        || (*effective_trigger_mode == TriggerMode::ProvisioningPackage
            && provisioning_post_signin_wifi_required(request))
    {
        fs::write(
            scripts_dir.join(PROVISIONING_WIFI_SCRIPT),
            build_wifi_connect_script(&request.wifi),
        )
        .map_err(|e| format!("Failed to write {}: {}", PROVISIONING_WIFI_SCRIPT, e))?;
    }

    Ok(GeneratedScriptPayload {
        app_script_needed,
        custom_script_files,
    })
}

fn generate_provisioning_package_payload(
    request: &OobeProfileRequest,
    staging_dir: &Path,
    has_install_apps_script: bool,
    custom_script_files: &[String],
) -> Result<(), String> {
    let scripts_dir = staging_dir.join("Scripts");
    let regional_settings = resolve_provisioning_regional_settings(request)?;
    let ui_profile_snapshot = build_provisioning_ui_profile_snapshot(
        request,
        &regional_settings,
        has_install_apps_script,
        custom_script_files,
    )?;

    fs::write(
        staging_dir.join("Apply-BitOSDTProvisioning.ps1"),
        build_provisioning_bootstrap_script(None, request.oobe_config.hide_privacy_settings),
    )
    .map_err(|e| format!("Failed to write Apply-BitOSDTProvisioning.ps1: {}", e))?;

    fs::write(
        scripts_dir.join(PROVISIONING_ORCHESTRATOR_SCRIPT),
        build_provisioning_orchestrator_script(
            request,
            &regional_settings,
            has_install_apps_script,
            custom_script_files,
        ),
    )
    .map_err(|e| {
        format!(
            "Failed to write {}: {}",
            PROVISIONING_ORCHESTRATOR_SCRIPT, e
        )
    })?;

    fs::write(
        scripts_dir.join(PROVISIONING_UI_HTA_FILE),
        generate_provisioning_hta(
            BITOSDT_PROVISIONING_UI_PROFILE_PATH,
            BITOSDT_PROVISIONING_UI_STATE_PATH,
            BITOSDT_PROVISIONING_UI_STATUS_PATH,
            BITOSDT_PROVISIONING_UI_APP_PROGRESS_PATH,
            BITOSDT_PROVISIONING_UI_COMMAND_PATH,
            &format!(
                r"{}\{}",
                BITOSDT_RUNTIME_SCRIPTS_DIR, PROVISIONING_ORCHESTRATOR_SCRIPT
            ),
            BITOSDT_PROVISIONING_UI_HEARTBEAT_PATH,
            BITOSDT_PROVISIONING_UI_SHELL_LOG_PATH,
        ),
    )
    .map_err(|e| format!("Failed to write {}: {}", PROVISIONING_UI_HTA_FILE, e))?;

    fs::write(
        scripts_dir.join(PROVISIONING_KIOSK_HELPER_FILE),
        generate_provisioning_kiosk_helper_ps1(
            BITOSDT_PROVISIONING_UI_SHELL_LOG_PATH,
            "BitOSDT Provisioning",
        ),
    )
    .map_err(|e| format!("Failed to write {}: {}", PROVISIONING_KIOSK_HELPER_FILE, e))?;

    fs::write(
        scripts_dir.join(PROVISIONING_UI_PROFILE_FILE),
        ui_profile_snapshot,
    )
    .map_err(|e| format!("Failed to write {}: {}", PROVISIONING_UI_PROFILE_FILE, e))?;

    let readme =
        build_provisioning_package_readme(request, has_install_apps_script, custom_script_files);

    fs::write(staging_dir.join("PPKG-README.txt"), readme)
        .map_err(|e| format!("Failed to write PPKG-README.txt: {}", e))
}

fn build_provisioning_package_readme(
    request: &OobeProfileRequest,
    has_install_apps_script: bool,
    custom_script_files: &[String],
) -> String {
    let mut post_sign_in_steps = Vec::new();

    if provisioning_post_signin_computer_name_required(request) {
        post_sign_in_steps.push("- Prompted computer name".to_string());
    }
    if provisioning_post_signin_wifi_required(request) {
        post_sign_in_steps.push(
            "- Wi-Fi settings that need hidden-SSID, DNS, or unsupported-auth handling".to_string(),
        );
    }
    if provisioning_post_signin_domain_join_required(request) {
        post_sign_in_steps.push(
            "- Domain join when script mode, OU path, or prompted naming requires it".to_string(),
        );
    }
    if request.apps.disable_bitlocker {
        post_sign_in_steps.push(if request.apps.reboot_after_disable_bitlocker {
            "- BitLocker disable on C: before applications, followed by the saved reboot"
                .to_string()
        } else {
            "- BitLocker disable on C: before applications, continuing without an immediate restart"
                .to_string()
        });
    }
    if has_install_apps_script {
        post_sign_in_steps.push("- Applications with item progress".to_string());
    }
    if !request.apps.copied_items.is_empty() {
        post_sign_in_steps.push("- Copied file deployment".to_string());
    }
    if request.enable_debloat || !custom_script_files.is_empty() {
        post_sign_in_steps.push("- Optional debloat/custom scripts".to_string());
    }
    if post_sign_in_steps.is_empty() {
        post_sign_in_steps.push("- None".to_string());
    }

    format!(
        "BitOSDT ProvisioningPackage Trigger Mode\n\nThis profile is prepared for provisioning package workflows.\n\nExecution flow:\n1) The provisioning package applies native settings first when supported, including fixed computer name, local default user, broad HideOobe behavior, supported Wi-Fi profiles, and supported domain-join settings.\n2) Provisioning package then runs Apply-BitOSDTProvisioning.ps1 in silent context.\n3) Bootstrap prefers removable-media sidecar content beside the .ppkg, then falls back to package-local content if needed.\n4) Bootstrap stages scripts to C:\\BitOSDT\\Scripts, app payloads to C:\\BitOSDT\\Apps, and copied files/folders to C:\\BitOSDT\\Files.\n5) Bootstrap registers first sign-in launch, and the UI host keeps a scheduled resume task until completion.\n6) Start-BitOSDTOrchestrator.ps1 launches a full-screen provisioning HTA with persisted task state.\n7) Runtime restart choices are available for most post-sign-in steps, and the wizard resumes automatically after reboot.\n\nPost-sign-in steps:\n{}\n\nSidecar media layout:\n- Keep <PackageName>.ppkg, Scripts\\, Apps\\, and Files\\ in the same folder on the USB.\n\nNotes:\n- Provisioning apply context is non-interactive; only supported native settings apply during package installation.\n- Any remaining prompts or payload tasks run at first administrator sign-in.\n- Regenerate existing profiles to include this orchestration behavior.\n",
        post_sign_in_steps.join("\n")
    )
}

pub(crate) fn materialize_request_derived_provisioning_payload(
    request: &OobeProfileRequest,
    staging_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(staging_dir)
        .map_err(|e| format!("Failed to create staging directory: {}", e))?;
    fs::create_dir_all(staging_dir.join("Apps"))
        .map_err(|e| format!("Failed to create apps directory: {}", e))?;
    fs::create_dir_all(staging_dir.join("Files"))
        .map_err(|e| format!("Failed to create files directory: {}", e))?;

    let generated = stage_request_shared_script_payload(
        request,
        staging_dir,
        &TriggerMode::ProvisioningPackage,
    )?;
    generate_provisioning_package_payload(
        request,
        staging_dir,
        generated.app_script_needed,
        &generated.custom_script_files,
    )
}

fn create_profile_with_root(
    root: &Path,
    mut request: OobeProfileRequest,
) -> Result<OobeProfileSummary, String> {
    let sanitized_name = sanitize_profile_name(&request.name);
    if sanitized_name.is_empty() {
        return Err("Profile name is required.".to_string());
    }
    request.name = sanitized_name.clone();

    ensure_domain_inputs(&request.domain_join)?;
    ensure_default_user_inputs(&request.default_user)?;
    ensure_wifi_inputs(&request.wifi)?;
    ensure_usb_first_logon_requirements(&request)?;
    let normalized_computer_name =
        validate_computer_name(request.oobe_config.computer_name.as_deref())?;
    request.oobe_config.computer_name = normalized_computer_name.clone();

    fs::create_dir_all(root)
        .map_err(|e| format!("Failed to create OOBE root {}: {}", root.display(), e))?;

    let target_dir = profile_dir(root, &sanitized_name);
    let legacy_root = legacy_oobe_root_path();
    let allow_legacy_lookup = *root == oobe_root_path();
    if profile_exists_with_roots(
        root,
        if allow_legacy_lookup {
            Some(legacy_root.as_path())
        } else {
            None
        },
        &sanitized_name,
    ) && !request.overwrite
    {
        return Err(format!(
            "Profile '{}' already exists. Use overwrite to replace it.",
            sanitized_name
        ));
    }

    let staging_dir = profile_dir(root, &format!(".tmp-{}-{}", sanitized_name, Uuid::new_v4()));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .map_err(|e| format!("Failed to reset staging directory: {}", e))?;
    }
    fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Failed to create staging directory: {}", e))?;

    let scripts_dir = staging_dir.join("Scripts");
    let apps_dir = staging_dir.join("Apps");
    let files_dir = staging_dir.join("Files");
    fs::create_dir_all(&scripts_dir)
        .map_err(|e| format!("Failed to create scripts directory: {}", e))?;
    fs::create_dir_all(&apps_dir).map_err(|e| format!("Failed to create apps directory: {}", e))?;
    fs::create_dir_all(&files_dir)
        .map_err(|e| format!("Failed to create files directory: {}", e))?;

    copy_embedded_installers(&request, &apps_dir)?;
    copy_local_payloads_to_sidecar(&request, &files_dir)?;
    let generated_scripts =
        stage_request_shared_script_payload(&request, &staging_dir, &request.trigger_mode)?;
    let app_script_needed = generated_scripts.app_script_needed;
    let custom_script_files = generated_scripts.custom_script_files;

    if request.trigger_mode == TriggerMode::FirstLogonUsbScan {
        fs::write(
            scripts_dir.join(USB_ORCHESTRATOR_SCRIPT),
            build_usb_orchestrator_script(&request, app_script_needed, &custom_script_files),
        )
        .map_err(|e| format!("Failed to write {}: {}", USB_ORCHESTRATOR_SCRIPT, e))?;
    }

    if request.trigger_mode == TriggerMode::ProvisioningPackage {
        generate_provisioning_package_payload(
            &request,
            &staging_dir,
            app_script_needed,
            &custom_script_files,
        )?;
    } else {
        let first_logon_commands = build_first_logon_commands(
            &request,
            &sanitized_name,
            app_script_needed,
            &custom_script_files,
        );

        let (language, input_locale) = resolve_oobe_locale_settings(&request)?;
        let timezone = if request.timezone.trim().is_empty() {
            DEFAULT_TIMEZONE.to_string()
        } else {
            request.timezone.trim().to_string()
        };
        let usb_bootstrap_password = if request.trigger_mode == TriggerMode::FirstLogonUsbScan {
            Some(generate_bootstrap_password())
        } else {
            None
        };

        let unattend_config = bitosdt::config::UnattendConfig {
            language,
            input_locale,
            timezone,
            oobe: bitosdt::config::OobeConfig {
                skip_machine_oobe: request.oobe_config.skip_machine_oobe,
                skip_user_oobe: request.oobe_config.skip_user_oobe,
                hide_eula: request.oobe_config.hide_eula,
                hide_wireless_setup: request.oobe_config.hide_wireless_setup,
                hide_local_account_screen: request.oobe_config.hide_local_account_screen,
                hide_online_account_screens: request.oobe_config.hide_online_account_screens,
                network_location: map_network_location(&request.oobe_config.network_location),
                protect_your_pc: map_protect_your_pc(&request.oobe_config.protect_your_pc),
            },
            users: map_default_user_for_unattend(&request.default_user),
            administrator_password: usb_bootstrap_password.clone(),
            computer_name: normalized_computer_name,
            product_key: None,
            domain_join: map_domain_join_for_unattend(&request),
            wifi_profile: map_wifi_profile_for_unattend(&request.wifi),
            auto_logon: usb_bootstrap_password.clone().map(|password| {
                bitosdt::config::AutoLogonConfig {
                    username: USB_BOOTSTRAP_ADMIN_USERNAME.to_string(),
                    password,
                    domain: Some(".".to_string()),
                    logon_count: 4,
                }
            }),
            first_logon_commands,
        };

        let mut unattend_xml = bitosdt::config::UnattendGenerator::generate(&unattend_config)
            .map_err(|e| format!("Failed to generate Autounattend.xml: {}", e))?;

        if request.domain_join.enabled && request.domain_join_mode == DomainJoinMode::SpecializeXml
        {
            unattend_xml = render_plaintext_domain_join_password(
                &unattend_xml,
                &request.domain_join.password,
            )?;
        }

        fs::write(staging_dir.join(AUTOUNATTEND_FILE), unattend_xml)
            .map_err(|e| format!("Failed to write {}: {}", AUTOUNATTEND_FILE, e))?;

        if request.trigger_mode == TriggerMode::SetupUnattend {
            write_setup_unattend_readme(&staging_dir, &sanitized_name)?;
        }
    }

    fs::write(
        staging_dir.join(DEPLOYMENT_README_FILE),
        build_deployment_readme(&sanitized_name),
    )
    .map_err(|e| format!("Failed to write {}: {}", DEPLOYMENT_README_FILE, e))?;

    let now = Utc::now().to_rfc3339();
    let created_at = if target_dir.exists() {
        read_manifest(&target_dir)
            .map(|existing| existing.created_at)
            .unwrap_or_else(|_| now.clone())
    } else {
        now.clone()
    };

    let manifest = OobeProfileManifest {
        schema_version: OOBE_MANIFEST_SCHEMA_VERSION,
        name: sanitized_name.clone(),
        description: request.description.clone(),
        created_at,
        updated_at: now.clone(),
        request,
    };
    write_manifest(&staging_dir, &manifest)?;

    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| {
            format!(
                "Failed to remove existing profile {}: {}",
                target_dir.display(),
                e
            )
        })?;
    }

    fs::rename(&staging_dir, &target_dir)
        .map_err(|e| format!("Failed to finalize profile {}: {}", target_dir.display(), e))?;

    Ok(OobeProfileSummary {
        name: sanitized_name,
        description: manifest.description,
        path: target_dir.to_string_lossy().to_string(),
        updated_at: now,
        has_manifest: true,
        preflight_warnings: Vec::new(),
    })
}

pub fn create_oobe_profile(request: OobeProfileRequest) -> Result<OobeProfileSummary, String> {
    create_profile_with_root(&oobe_root_path(), request)
}

pub fn list_oobe_profiles() -> Result<Vec<OobeProfileSummary>, String> {
    let root = oobe_root_path();
    let legacy_root = legacy_oobe_root_path();
    if !root.exists() && !legacy_root.exists() {
        return Ok(Vec::new());
    }

    let mut seen = HashSet::new();
    let mut profiles = Vec::new();
    for current_root in [&root, &legacy_root] {
        if !current_root.exists() {
            continue;
        }
        for entry in fs::read_dir(current_root)
            .map_err(|e| format!("Failed to list {}: {}", current_root.display(), e))?
        {
            let entry = entry.map_err(|e| format!("Failed to inspect profile directory: {}", e))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let folder_name = entry.file_name().to_string_lossy().to_string();
            if is_staging_profile_name(&folder_name) {
                continue;
            }
            let dedupe_name = sanitize_profile_name(&folder_name).to_ascii_lowercase();
            if !seen.insert(dedupe_name) {
                continue;
            }

            let preflight_warnings =
                preflight_profile_with_roots(&root, Some(&legacy_root), &folder_name)
                    .map(|p| p.warnings)
                    .unwrap_or_default();
            let manifest_path = path.join(PROFILE_MANIFEST_FILE);
            let has_manifest = manifest_path.is_file();
            if has_manifest {
                match read_manifest(&path) {
                    Ok(manifest) => {
                        profiles.push(OobeProfileSummary {
                            name: folder_name,
                            description: manifest.description,
                            path: path.to_string_lossy().to_string(),
                            updated_at: manifest.updated_at,
                            has_manifest: true,
                            preflight_warnings: preflight_warnings.clone(),
                        });
                    }
                    Err(_) => {
                        profiles.push(OobeProfileSummary {
                            name: folder_name,
                            description: String::new(),
                            path: path.to_string_lossy().to_string(),
                            updated_at: profile_updated_timestamp(&path),
                            has_manifest: false,
                            preflight_warnings: preflight_warnings.clone(),
                        });
                    }
                }
            } else if path.join(AUTOUNATTEND_FILE).is_file()
                || path.join("Apply-BitOSDTProvisioning.ps1").is_file()
            {
                profiles.push(OobeProfileSummary {
                    name: folder_name,
                    description: String::new(),
                    path: path.to_string_lossy().to_string(),
                    updated_at: profile_updated_timestamp(&path),
                    has_manifest: false,
                    preflight_warnings: preflight_warnings.clone(),
                });
            }
        }
    }

    profiles.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(profiles)
}

pub fn get_oobe_profile(name: String) -> Result<OobeProfileDetail, String> {
    let profile_name = sanitize_profile_name(&name);
    if profile_name.is_empty() {
        return Err("Profile name is required.".to_string());
    }
    let path = resolve_oobe_profile_path(&profile_name)
        .ok_or_else(|| format!("Profile not found: {}", profile_name))?;
    if !path.exists() {
        return Err(format!("Profile not found: {}", profile_name));
    }

    let manifest = read_manifest(&path)?;
    Ok(OobeProfileDetail {
        name: profile_name,
        path: path.to_string_lossy().to_string(),
        created_at: manifest.created_at,
        updated_at: manifest.updated_at,
        request: manifest.request,
    })
}

pub fn rename_oobe_profile(name: String, new_name: String) -> Result<OobeProfileSummary, String> {
    let old_name = sanitize_profile_name(&name);
    let next_name = sanitize_profile_name(&new_name);
    if old_name.is_empty() || next_name.is_empty() {
        return Err("Both current and new profile names are required.".to_string());
    }
    if old_name.eq_ignore_ascii_case(&next_name) {
        return Err("New profile name must be different.".to_string());
    }

    let root = oobe_root_path();
    let legacy_root = legacy_oobe_root_path();
    let old_path = resolve_profile_path_with_roots(&root, Some(&legacy_root), &old_name)
        .ok_or_else(|| format!("Profile not found: {}", old_name))?;
    let new_path = profile_dir(&root, &next_name);
    if profile_exists_with_roots(&root, Some(&legacy_root), &next_name) {
        return Err(format!("Profile already exists: {}", next_name));
    }

    if old_path.starts_with(&root) {
        fs::rename(&old_path, &new_path).map_err(|e| {
            format!(
                "Failed to rename profile {} to {}: {}",
                old_name, next_name, e
            )
        })?;
    } else {
        copy_directory_recursive(&old_path, &new_path)?;
        let _ = fs::remove_dir_all(&old_path);
    }

    let legacy_old_path = profile_dir(&legacy_root, &old_name);
    if legacy_old_path.exists() && legacy_old_path != new_path {
        let _ = fs::remove_dir_all(&legacy_old_path);
    }

    if let Ok(mut manifest) = read_manifest(&new_path) {
        manifest.name = next_name.clone();
        manifest.request.name = next_name.clone();
        manifest.updated_at = Utc::now().to_rfc3339();
        let _ = write_manifest(&new_path, &manifest);
    }

    ensure_deployment_readme(&new_path, &next_name)?;

    Ok(OobeProfileSummary {
        name: next_name.clone(),
        description: read_manifest(&new_path)
            .map(|m| m.description)
            .unwrap_or_default(),
        path: new_path.to_string_lossy().to_string(),
        updated_at: profile_updated_timestamp(&new_path),
        has_manifest: new_path.join(PROFILE_MANIFEST_FILE).is_file(),
        preflight_warnings: preflight_profile_with_roots(&root, Some(&legacy_root), &next_name)
            .map(|p| p.warnings)
            .unwrap_or_default(),
    })
}
pub fn duplicate_oobe_profile(
    name: String,
    new_name: String,
) -> Result<OobeProfileSummary, String> {
    let source_name = sanitize_profile_name(&name);
    let target_name = sanitize_profile_name(&new_name);
    if source_name.is_empty() || target_name.is_empty() {
        return Err("Both source and duplicate profile names are required.".to_string());
    }
    let root = oobe_root_path();
    let legacy_root = legacy_oobe_root_path();
    let source_path = resolve_profile_path_with_roots(&root, Some(&legacy_root), &source_name)
        .ok_or_else(|| format!("Profile not found: {}", source_name))?;
    let target_path = profile_dir(&root, &target_name);
    if profile_exists_with_roots(&root, Some(&legacy_root), &target_name) {
        return Err(format!("Profile already exists: {}", target_name));
    }

    copy_directory_recursive(&source_path, &target_path)?;
    ensure_deployment_readme(&target_path, &target_name)?;

    if let Ok(mut manifest) = read_manifest(&target_path) {
        let now = Utc::now().to_rfc3339();
        manifest.name = target_name.clone();
        manifest.request.name = target_name.clone();
        manifest.created_at = now.clone();
        manifest.updated_at = now.clone();
        write_manifest(&target_path, &manifest)?;
    }

    Ok(OobeProfileSummary {
        name: target_name.clone(),
        description: read_manifest(&target_path)
            .map(|m| m.description)
            .unwrap_or_default(),
        path: target_path.to_string_lossy().to_string(),
        updated_at: profile_updated_timestamp(&target_path),
        has_manifest: target_path.join(PROFILE_MANIFEST_FILE).is_file(),
        preflight_warnings: preflight_profile_with_roots(&root, Some(&legacy_root), &target_name)
            .map(|p| p.warnings)
            .unwrap_or_default(),
    })
}

pub fn delete_oobe_profile(name: String) -> Result<(), String> {
    let profile_name = sanitize_profile_name(&name);
    if profile_name.is_empty() {
        return Err("Profile name is required.".to_string());
    }
    let root = oobe_root_path();
    let legacy_root = legacy_oobe_root_path();
    let path = resolve_oobe_profile_path(&profile_name)
        .ok_or_else(|| format!("Profile not found: {}", profile_name))?;
    if !path.exists() {
        return Err(format!("Profile not found: {}", profile_name));
    }
    fs::remove_dir_all(&path)
        .map_err(|e| format!("Failed to delete profile {}: {}", profile_name, e))?;
    for duplicate in [
        profile_dir(&root, &profile_name),
        profile_dir(&legacy_root, &profile_name),
    ] {
        if duplicate.exists() && duplicate != path {
            let _ = fs::remove_dir_all(duplicate);
        }
    }
    Ok(())
}

fn add_directory_to_zip(
    writer: &mut ZipWriter<File>,
    root: &Path,
    current: &Path,
    options: FileOptions,
) -> Result<(), String> {
    for entry in
        fs::read_dir(current).map_err(|e| format!("Failed to list {}: {}", current.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to inspect directory entry: {}", e))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|e| format!("Failed to strip zip prefix: {}", e))?;
        let mut zip_path = relative.to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            if !zip_path.ends_with('/') {
                zip_path.push('/');
            }
            writer
                .add_directory(zip_path, options)
                .map_err(|e| format!("Failed to add zip directory: {}", e))?;
            add_directory_to_zip(writer, root, &path, options)?;
        } else {
            writer
                .start_file(zip_path, options)
                .map_err(|e| format!("Failed to add zip file: {}", e))?;
            let mut file = File::open(&path)
                .map_err(|e| format!("Failed to open {} for zipping: {}", path.display(), e))?;
            std::io::copy(&mut file, writer)
                .map_err(|e| format!("Failed to write zip content: {}", e))?;
        }
    }
    Ok(())
}

pub fn export_oobe_profile_zip(name: String, output_zip_path: String) -> Result<String, String> {
    let profile_name = sanitize_profile_name(&name);
    if profile_name.is_empty() {
        return Err("Profile name is required.".to_string());
    }
    let profile_path = resolve_oobe_profile_path(&profile_name)
        .ok_or_else(|| format!("Profile not found: {}", profile_name))?;

    let output_path = PathBuf::from(output_zip_path.trim());
    if output_path.as_os_str().is_empty() {
        return Err("Output zip path is required.".to_string());
    }
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create export directory: {}", e))?;
        }
    }

    let file = File::create(&output_path)
        .map_err(|e| format!("Failed to create zip {}: {}", output_path.display(), e))?;
    let mut writer = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let root = profile_path
        .parent()
        .ok_or_else(|| "Profile path parent is not available.".to_string())?;
    add_directory_to_zip(&mut writer, root, &profile_path, options)?;
    writer
        .finish()
        .map_err(|e| format!("Failed to finalize zip export: {}", e))?;

    Ok(output_path.to_string_lossy().to_string())
}

fn ensure_zip_entry_safe(path: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(format!("Invalid zip entry path: {}", path));
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("Unsafe zip entry path: {}", path));
    }
    Ok(())
}

fn create_default_manifest(profile_name: &str) -> OobeProfileManifest {
    let now = Utc::now().to_rfc3339();
    OobeProfileManifest {
        schema_version: OOBE_MANIFEST_SCHEMA_VERSION,
        name: profile_name.to_string(),
        description: String::new(),
        created_at: now.clone(),
        updated_at: now,
        request: OobeProfileRequest {
            name: profile_name.to_string(),
            ..Default::default()
        },
    }
}

fn ensure_unique_profile_name(root: &Path, desired_name: &str) -> String {
    let legacy_root = legacy_oobe_root_path();
    let legacy_lookup = if *root == oobe_root_path() {
        Some(legacy_root.as_path())
    } else {
        None
    };
    if !profile_exists_with_roots(root, legacy_lookup, desired_name) {
        return desired_name.to_string();
    }

    for index in 1..1000 {
        let candidate = format!("{}-copy-{}", desired_name, index);
        if !profile_exists_with_roots(root, legacy_lookup, &candidate) {
            return candidate;
        }
    }

    format!("{}-{}", desired_name, Uuid::new_v4().simple())
}

pub fn import_oobe_profile_zip(zip_path: String) -> Result<OobeProfileSummary, String> {
    let zip_path = PathBuf::from(zip_path.trim());
    if !zip_path.is_file() {
        return Err(format!("Import zip was not found: {}", zip_path.display()));
    }

    let root = oobe_root_path();
    fs::create_dir_all(&root)
        .map_err(|e| format!("Failed to create OOBE root {}: {}", root.display(), e))?;

    let temp_extract = root.join(format!(".import-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_extract)
        .map_err(|e| format!("Failed to create import temp directory: {}", e))?;

    let import_result = (|| -> Result<OobeProfileSummary, String> {
        let file =
            File::open(&zip_path).map_err(|e| format!("Failed to open zip archive: {}", e))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| format!("Failed to parse zip archive: {}", e))?;

        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| format!("Failed to inspect zip entry: {}", e))?;
            let entry_name = entry.name().to_string();
            ensure_zip_entry_safe(&entry_name)?;
            let destination = temp_extract.join(&entry_name);

            if entry_name.ends_with('/') {
                fs::create_dir_all(&destination)
                    .map_err(|e| format!("Failed to create extracted directory: {}", e))?;
            } else {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create extracted parent directory: {}", e)
                    })?;
                }
                let mut output = File::create(&destination).map_err(|e| {
                    format!(
                        "Failed to create extracted file {}: {}",
                        destination.display(),
                        e
                    )
                })?;
                std::io::copy(&mut entry, &mut output)
                    .map_err(|e| format!("Failed to extract zip content: {}", e))?;
            }
        }

        let mut candidate_dirs = Vec::new();
        for entry in fs::read_dir(&temp_extract)
            .map_err(|e| format!("Failed to list extracted directory: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to inspect extracted entry: {}", e))?;
            let path = entry.path();
            if path.is_dir() {
                candidate_dirs.push(path);
            }
        }

        if candidate_dirs.is_empty() {
            candidate_dirs.push(temp_extract.clone());
        }

        let is_profile_payload_dir = |dir: &Path| {
            dir.join(PROFILE_MANIFEST_FILE).is_file()
                || dir.join(AUTOUNATTEND_FILE).is_file()
                || dir.join("Apply-BitOSDTProvisioning.ps1").is_file()
        };

        let profile_source = candidate_dirs
            .iter()
            .find(|dir| is_profile_payload_dir(dir))
            .cloned()
            .or_else(|| {
                for dir in &candidate_dirs {
                    if let Ok(entries) = fs::read_dir(dir) {
                        for nested in entries.flatten() {
                            let nested_path = nested.path();
                            if nested_path.is_dir() && is_profile_payload_dir(&nested_path) {
                                return Some(nested_path);
                            }
                        }
                    }
                }
                None
            })
            .ok_or_else(|| {
                format!(
                    "Import archive does not contain {}, {} or {}.",
                    PROFILE_MANIFEST_FILE, AUTOUNATTEND_FILE, "Apply-BitOSDTProvisioning.ps1"
                )
            })?;

        let mut manifest = if profile_source.join(PROFILE_MANIFEST_FILE).is_file() {
            read_manifest(&profile_source)?
        } else {
            let folder_name = profile_source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "ImportedProfile".to_string());
            create_default_manifest(&folder_name)
        };

        let requested_name = sanitize_profile_name(&manifest.request.name);
        let base_name = if requested_name.is_empty() {
            profile_source
                .file_name()
                .map(|n| sanitize_profile_name(&n.to_string_lossy()))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "ImportedProfile".to_string())
        } else {
            requested_name
        };
        let final_name = ensure_unique_profile_name(&root, &base_name);
        let final_path = profile_dir(&root, &final_name);

        copy_directory_recursive(&profile_source, &final_path)?;

        manifest.name = final_name.clone();
        manifest.request.name = final_name.clone();
        manifest.updated_at = Utc::now().to_rfc3339();
        if manifest.created_at.trim().is_empty() {
            manifest.created_at = manifest.updated_at.clone();
        }
        write_manifest(&final_path, &manifest)?;
        ensure_deployment_readme(&final_path, &final_name)?;

        Ok(OobeProfileSummary {
            name: final_name.clone(),
            description: manifest.description,
            path: final_path.to_string_lossy().to_string(),
            updated_at: manifest.updated_at,
            has_manifest: true,
            preflight_warnings: preflight_profile_with_root(&root, &final_name)
                .map(|p| p.warnings)
                .unwrap_or_default(),
        })
    })();

    let _ = fs::remove_dir_all(&temp_extract);
    import_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn collect_powershell_scripts(root: &Path) -> Vec<PathBuf> {
        let mut scripts = Vec::new();
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scripts.extend(collect_powershell_scripts(&path));
                } else if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("ps1"))
                    .unwrap_or(false)
                {
                    scripts.push(path);
                }
            }
        }
        scripts
    }

    #[cfg(windows)]
    fn assert_powershell_script_parses(path: &Path) {
        let escaped_path = path.display().to_string().replace('\'', "''");
        let command = format!(
            "$path = '{}'; $tokens = $null; $errors = $null; [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors) | Out-Null; if ($errors.Count -gt 0) {{ $errors | ForEach-Object {{ Write-Output ('{{0}}:{{1}}:{{2}}' -f $_.Extent.File, $_.Extent.StartLineNumber, $_.Message) }}; exit 1 }}",
            escaped_path
        );

        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &command])
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "failed to invoke powershell.exe for {}: {}",
                    path.display(),
                    e
                )
            });

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!(
                "PowerShell parser rejected {}.\nstdout:\n{}\nstderr:\n{}",
                path.display(),
                stdout,
                stderr
            );
        }
    }

    fn make_test_root(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bitosdt-oobe-{}-{}", prefix, Uuid::new_v4()))
    }

    fn base_request(name: &str) -> OobeProfileRequest {
        OobeProfileRequest {
            name: name.to_string(),
            prompt_for_computer_name: true,
            default_user: DefaultUserUiConfig {
                enabled: true,
                username: "localadmin".to_string(),
                password: "Password123!".to_string(),
                group: "Administrators".to_string(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn create_profile_uses_en_us_defaults_for_locale_settings() {
        let root = make_test_root("locale-en-us");
        fs::create_dir_all(&root).unwrap();

        let request = base_request("ProfileLocaleEnUs");
        create_profile_with_root(&root, request).unwrap();

        let xml =
            fs::read_to_string(root.join("ProfileLocaleEnUs").join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("<UILanguage>en-US</UILanguage>"));
        assert!(xml.contains("<InputLocale>0409:00000409</InputLocale>"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_profile_normalizes_fr_locale_settings() {
        let root = make_test_root("locale-fr-fr");
        fs::create_dir_all(&root).unwrap();

        let mut request = base_request("ProfileLocaleFrFr");
        request.language = "fr-fr".to_string();
        request.input_locale = "fr-fr".to_string();

        create_profile_with_root(&root, request).unwrap();

        let xml =
            fs::read_to_string(root.join("ProfileLocaleFrFr").join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("<UILanguage>fr-FR</UILanguage>"));
        assert!(xml.contains("<InputLocale>fr-FR</InputLocale>"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_profile_supports_non_latin_locale_settings() {
        let root = make_test_root("locale-ja-jp");
        fs::create_dir_all(&root).unwrap();

        let mut request = base_request("ProfileLocaleJaJp");
        request.language = "ja-jp".to_string();
        request.input_locale = "ja-jp".to_string();

        create_profile_with_root(&root, request).unwrap();

        let xml =
            fs::read_to_string(root.join("ProfileLocaleJaJp").join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("<UILanguage>ja-JP</UILanguage>"));
        assert!(xml.contains("<InputLocale>ja-JP</InputLocale>"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_profile_rejects_unsupported_locale_combinations() {
        let root = make_test_root("locale-invalid-combo");
        fs::create_dir_all(&root).unwrap();

        let mut request = base_request("ProfileLocaleInvalid");
        request.language = "fr-FR".to_string();
        request.input_locale = "0409:00000409".to_string();

        let err = create_profile_with_root(&root, request).unwrap_err();
        assert!(err.contains("Unsupported locale combination"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sanitize_profile_name_replaces_invalid_characters() {
        assert_eq!(
            sanitize_profile_name("  My/Profile:One  "),
            "My_Profile_One"
        );
        assert_eq!(sanitize_profile_name("..."), "");
    }

    #[test]
    fn staging_profile_names_are_detected() {
        assert!(is_staging_profile_name(".tmp-ProfileOne-1234"));
        assert!(is_staging_profile_name("  .tmp-ProfileTwo-5678  "));
        assert!(!is_staging_profile_name("ProfileOne"));
    }

    #[test]
    fn plaintext_password_is_written_for_domain_join_component() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<unattend xmlns="urn:schemas-microsoft-com:unattend">
  <settings pass="specialize">
    <component name="Microsoft-Windows-UnattendedJoin">
      <Identification>
        <Credentials>
          <Domain>corp</Domain>
          <Password>ENCODED</Password>
          <Username>joiner</Username>
        </Credentials>
      </Identification>
    </component>
  </settings>
</unattend>"#;

        let updated = render_plaintext_domain_join_password(xml, "P@ssw&rd").unwrap();
        assert!(updated.contains("<Password>P@ssw&amp;rd</Password>"));
    }

    #[test]
    fn preflight_reports_missing_autounattend() {
        let root = make_test_root("preflight-missing-autounattend");
        fs::create_dir_all(&root).unwrap();

        let request = base_request("PreflightOne");
        create_profile_with_root(&root, request).unwrap();
        fs::remove_file(root.join("PreflightOne").join(AUTOUNATTEND_FILE)).unwrap();

        let report = preflight_profile_with_root(&root, "PreflightOne").unwrap();
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("Missing required file")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preflight_detects_folder_name_mismatch() {
        let root = make_test_root("preflight-folder-mismatch");
        fs::create_dir_all(&root).unwrap();

        let request = base_request("MismatchName");
        create_profile_with_root(&root, request).unwrap();
        fs::rename(root.join("MismatchName"), root.join("mismatchname")).unwrap();

        let report = preflight_profile_with_root(&root, "MismatchName").unwrap();
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("Profile folder case mismatch")
                || w.contains("Profile lookup mismatch")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_profile_uses_star_computer_name_when_empty() {
        let root = make_test_root("computer-name");
        fs::create_dir_all(&root).unwrap();
        let request = base_request("ProfileOne");
        create_profile_with_root(&root, request).unwrap();
        let xml = fs::read_to_string(root.join("ProfileOne").join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("<ComputerName>*</ComputerName>"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_profile_includes_default_user_mapping() {
        let root = make_test_root("default-user");
        fs::create_dir_all(&root).unwrap();
        let request = base_request("ProfileTwo");

        create_profile_with_root(&root, request).unwrap();
        let xml = fs::read_to_string(root.join("ProfileTwo").join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("<UserAccounts>"));
        assert!(xml.contains("<Name>localadmin</Name>"));
        assert!(xml.contains("<Group>Administrators</Group>"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn first_logon_mode_includes_builtin_admin_password_and_autologon() {
        let root = make_test_root("usb-autologon");
        fs::create_dir_all(&root).unwrap();

        let request = base_request("UsbBootstrapProfile");
        create_profile_with_root(&root, request).unwrap();

        let xml =
            fs::read_to_string(root.join("UsbBootstrapProfile").join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("<AdministratorPassword>"));
        assert!(xml.contains("<Username>Administrator</Username>"));
        assert!(xml.contains("<Domain>.</Domain>"));
        assert!(xml.contains("<LogonCount>4</LogonCount>"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_profile_includes_wifi_profile() {
        let root = make_test_root("wifi-profile");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProfileWifi");
        request.wifi = OobeWifiConfig {
            enabled: true,
            ssid: "CorpWiFi".to_string(),
            password: "WirelessP@ss123".to_string(),
            authentication: "Wpa2Psk".to_string(),
            encryption: "Aes".to_string(),
            auto_connect: true,
            hidden_network: false,
            dns_server_1: String::new(),
            dns_server_2: String::new(),
        };

        create_profile_with_root(&root, request).unwrap();
        let xml = fs::read_to_string(root.join("ProfileWifi").join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("Microsoft-Windows-Wlansvc"));
        assert!(xml.contains("<name>CorpWiFi</name>"));
        assert!(xml.contains("<keyMaterial>WirelessP@ss123</keyMaterial>"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn embedded_msi_is_copied_and_runs_from_bitosdt_apps() {
        let root = make_test_root("embedded-msi");
        fs::create_dir_all(&root).unwrap();

        let embedded_source = root.join("sample-installer.msi");
        fs::write(&embedded_source, b"msi-binary-placeholder").unwrap();

        let mut request = base_request("ProfileEmbeddedMsi");
        request.apps.custom_installers = vec![OobeCustomInstaller {
            name: "Sample MSI".to_string(),
            path: embedded_source.to_string_lossy().to_string(),
            source_type: Some("EmbeddedFile".to_string()),
            source_file_name: None,
            dependencies: vec![],
            dependency_destination: None,
            silent_args: "/qn /norestart".to_string(),
            installer_type: "Msi".to_string(),
            enabled: true,
        }];

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProfileEmbeddedMsi");
        assert!(profile_path
            .join("Apps")
            .join("sample-installer.msi")
            .is_file());

        let install_script =
            fs::read_to_string(profile_path.join("Scripts").join("installapps.ps1")).unwrap();
        assert!(install_script.contains(r#"C:\BitOSDT\Apps\sample-installer.msi"#));

        let xml = fs::read_to_string(profile_path.join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("Bootstrap USB OOBE payload"));
        assert!(profile_path
            .join("Scripts")
            .join(USB_ORCHESTRATOR_SCRIPT)
            .is_file());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_payloads_are_copied_into_files_sidecar_and_script() {
        let root = make_test_root("payload-sidecar");
        fs::create_dir_all(&root).unwrap();

        let copied_file = root.join("copied.txt");
        let dependency_dir = root.join("SupportFiles");
        fs::create_dir_all(&dependency_dir).unwrap();
        fs::write(&copied_file, b"payload").unwrap();
        fs::write(dependency_dir.join("helper.dll"), b"helper").unwrap();

        let mut request = base_request("ProfilePayloadSidecar");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.apps.copied_items = vec![OobeLocalPayloadItem {
            source_path: copied_file.to_string_lossy().to_string(),
            source_kind: "File".to_string(),
            display_name: None,
        }];
        request.apps.custom_installers = vec![OobeCustomInstaller {
            name: "Installer With Dependency".to_string(),
            path: r"D:\Apps\setup.exe".to_string(),
            source_type: Some("DirectPathOrUrl".to_string()),
            source_file_name: None,
            dependencies: vec![OobeLocalPayloadItem {
                source_path: dependency_dir.to_string_lossy().to_string(),
                source_kind: "Directory".to_string(),
                display_name: None,
            }],
            dependency_destination: Some(r"C:\Vendor\Support".to_string()),
            silent_args: "/quiet".to_string(),
            installer_type: "Exe".to_string(),
            enabled: true,
        }];

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProfilePayloadSidecar");
        assert!(profile_path.join("Files").join("copied.txt").is_file());
        assert!(profile_path
            .join("Files")
            .join("SupportFiles")
            .join("helper.dll")
            .is_file());

        let install_script =
            fs::read_to_string(profile_path.join("Scripts").join("installapps.ps1")).unwrap();
        assert!(install_script.contains(r#"C:\BitOSDT\Files\copied.txt"#));
        assert!(install_script.contains(r#"C:\BitOSDT\Files\SupportFiles"#));
        assert!(install_script.contains(r#"$destinationRoot = "C:\Vendor\Support""#));

        let manifest = fs::read_to_string(profile_path.join(PROFILE_MANIFEST_FILE)).unwrap();
        assert!(manifest.contains("\"copiedItems\""));
        assert!(manifest.contains("\"dependencyDestination\""));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_manager_bootstrap_matches_iso_behavior() {
        let root = make_test_root("pkg-bootstrap");
        fs::create_dir_all(&root).unwrap();

        let mut request = base_request("ProfilePackageBootstrap");
        request.apps.winget_packages = vec![OobeWingetPackage {
            package_id: "Microsoft.VisualStudioCode".to_string(),
            version: None,
            custom_args: None,
            enabled: true,
        }];
        request.apps.chocolatey_packages = vec![OobeChocolateyPackage {
            package_name: "googlechrome".to_string(),
            version: None,
            source: None,
            custom_args: None,
            enabled: true,
        }];
        request.apps.auto_install_chocolatey = true;

        create_profile_with_root(&root, request).unwrap();
        let install_script = fs::read_to_string(
            root.join("ProfilePackageBootstrap")
                .join("Scripts")
                .join("installapps.ps1"),
        )
        .unwrap();

        assert!(install_script.contains("BitOSDTWingetInstallers"));
        assert!(install_script.contains(
            "Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe"
        ));
        assert!(install_script.contains("Chocolatey not found. Installing Chocolatey..."));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn setup_unattend_mode_skips_first_logon_commands_and_writes_readme() {
        let root = make_test_root("setup-unattend");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("SetupOnlyProfile");
        request.trigger_mode = TriggerMode::SetupUnattend;
        request.enable_debloat = true;

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("SetupOnlyProfile");
        let xml = fs::read_to_string(profile_path.join(AUTOUNATTEND_FILE)).unwrap();
        assert!(!xml.contains("FirstLogonCommands"));
        assert!(profile_path.join("SETUP-UNATTEND-README.txt").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_package_mode_writes_ppkg_outputs_without_autounattend() {
        let root = make_test_root("ppkg-mode");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningProfile");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.enable_debloat = true;

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningProfile");
        assert!(!profile_path.join(AUTOUNATTEND_FILE).exists());
        assert!(profile_path.join("Apply-BitOSDTProvisioning.ps1").is_file());
        assert!(profile_path
            .join("Scripts")
            .join(PROVISIONING_ORCHESTRATOR_SCRIPT)
            .is_file());
        assert!(profile_path
            .join("Scripts")
            .join(PROVISIONING_UI_HTA_FILE)
            .is_file());
        assert!(profile_path
            .join("Scripts")
            .join(PROVISIONING_UI_PROFILE_FILE)
            .is_file());
        let hta = fs::read_to_string(profile_path.join("Scripts").join(PROVISIONING_UI_HTA_FILE))
            .unwrap();
        assert!(hta.contains("ui-heartbeat.json"));
        assert!(profile_path.join("PPKG-README.txt").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_package_mode_generates_orchestrator_for_name_prompt_when_missing() {
        let root = make_test_root("ppkg-prompt-auto");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningPromptAuto");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.enable_debloat = true;
        request.prompt_for_computer_name = false;
        request.oobe_config.computer_name = None;

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningPromptAuto");
        let bootstrap =
            fs::read_to_string(profile_path.join("Apply-BitOSDTProvisioning.ps1")).unwrap();
        assert!(bootstrap.contains("BitOSDTProvisioning"));
        assert!(bootstrap.contains(PROVISIONING_ORCHESTRATOR_SCRIPT));
        assert!(bootstrap.contains("Resolve-BitOSDTProvisioningMediaRoot"));
        assert!(bootstrap.contains(r"C:\BitOSDT\Scripts"));
        assert!(bootstrap.contains(r"C:\BitOSDT\Apps"));
        assert!(bootstrap.contains("$HidePrivacySettings = $true"));
        assert!(bootstrap.contains("DisablePrivacyExperience"));
        let orchestrator = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_ORCHESTRATOR_SCRIPT),
        )
        .unwrap();
        assert!(orchestrator.contains("Launch-Ui"));
        assert!(orchestrator.contains("Process-Command"));
        assert!(orchestrator.contains("BitOSDTProvisioningUi"));
        assert!(orchestrator.contains(PROVISIONING_UI_HTA_FILE));
        assert!(orchestrator.contains(BITOSDT_PROVISIONING_UI_STATUS_PATH));
        assert!(orchestrator.contains(BITOSDT_PROVISIONING_UI_HEARTBEAT_PATH));
        assert!(orchestrator.contains("function Wait-ForFreshHeartbeat {"));
        assert!(orchestrator.contains("function Start-ProvisioningUiHost {"));
        assert!(orchestrator.contains("function Find-ProvisioningUiProcess {"));
        assert!(orchestrator.contains("function Ensure-RunOnce {"));
        assert!(orchestrator.contains("function Normalize-RestartChoices {"));
        assert!(orchestrator.contains("function Resolve-RegionalSettings {"));
        assert!(orchestrator.contains("function Apply-RegionalSettings {"));
        assert!(orchestrator.contains("function Set-UkDateFormat {"));
        assert!(orchestrator.contains("Apply-RegionalSettings -Profile $profile"));
        assert!(orchestrator.contains(
            "Failed to write JSON file $Path after multiple attempts: $lastErrorMessage"
        ));
        assert!(orchestrator.contains("for ($attempt = 1; $attempt -le 12; $attempt++) {"));
        assert!(orchestrator.contains("Start-Sleep -Milliseconds 150"));
        assert!(orchestrator.contains("$state.restartChoices = Normalize-RestartChoices -Value $state.restartChoices -Steps $steps"));
        assert!(orchestrator.contains("Provisioning HTA already running and heartbeat is healthy."));
        assert!(orchestrator
            .contains("Provisioning HTA process exists but heartbeat is stale or missing."));
        assert!(orchestrator.contains("Provisioning HTA did not become responsive after launch."));
        assert!(
            orchestrator.contains("Evaluating UI launch. terminalStatus={0}; currentStep={1}; {2}")
        );
        assert!(orchestrator
            .contains("Processing UI command for step {0}; restartNow={1}; currentStep={2}; {3}"));
        assert!(orchestrator.contains(
            "Completed step {0}; taskStatus={1}; rebootPending={2}; restartNow={3}; nextStep={4}"
        ));
        assert!(orchestrator.contains("RunOnce launcher armed for next admin sign-in."));
        assert!(orchestrator.contains("if ($result.rebootPending) {"));
        assert!(orchestrator.contains("Ensure-RunOnce"));
        assert!(
            orchestrator.contains("Provisioning orchestration failed during step {0}: {1}; {2}")
        );
        assert!(orchestrator.contains("Failed to register scheduled task ${ScheduledTaskName}:"));
        assert!(!orchestrator.contains("Failed to register scheduled task $ScheduledTaskName:"));
        assert!(!orchestrator.contains("id = 'bitLocker'"));

        let debloat = fs::read_to_string(profile_path.join("Scripts").join("debloat.ps1")).unwrap();
        assert!(debloat.contains("Failed to remove ${app}: $_"));
        assert!(!debloat.contains("Failed to remove $app: $_"));

        let profile_snapshot = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_UI_PROFILE_FILE),
        )
        .unwrap();
        assert!(profile_snapshot.contains(r#""language": "en-US""#));
        assert!(profile_snapshot.contains(r#""inputLocale": "0409:00000409""#));
        assert!(profile_snapshot.contains(r#""timezone": "Pacific Standard Time""#));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_package_mode_adds_bitlocker_step_before_apps_and_persists_settings() {
        let root = make_test_root("ppkg-bitlocker");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningBitLocker");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.apps.disable_bitlocker = true;
        request.apps.reboot_after_disable_bitlocker = true;
        request.apps.winget_packages = vec![OobeWingetPackage {
            package_id: "Microsoft.VisualStudioCode".to_string(),
            version: None,
            custom_args: None,
            enabled: true,
        }];

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningBitLocker");
        let manifest = read_manifest(&profile_path).unwrap();
        assert!(manifest.request.apps.disable_bitlocker);
        assert!(manifest.request.apps.reboot_after_disable_bitlocker);

        let bitlocker_script = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_BITLOCKER_SCRIPT),
        )
        .unwrap();
        assert!(bitlocker_script.contains("manage-bde.exe"));
        assert!(bitlocker_script.contains("manage-bde -status C:"));
        assert!(bitlocker_script.contains("manage-bde -off C:"));
        assert!(bitlocker_script.contains("Protection Off"));
        assert!(bitlocker_script.contains("Decryption in Progress"));

        let orchestrator = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_ORCHESTRATOR_SCRIPT),
        )
        .unwrap();
        let bitlocker_index = orchestrator.find("id = 'bitLocker'").unwrap();
        let apps_index = orchestrator.find("id = 'apps'").unwrap();
        assert!(bitlocker_index < apps_index);
        assert!(orchestrator.contains("$BitLockerEnabled = $true"));
        assert!(orchestrator.contains("$BitLockerRebootAfterDisable = $true"));
        assert!(orchestrator.contains("function Invoke-BitLockerStep {"));
        assert!(orchestrator.contains("Invoke-BitOSDTScript -Name 'disable-bitlocker.ps1'"));

        let profile_snapshot = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_UI_PROFILE_FILE),
        )
        .unwrap();
        assert!(profile_snapshot.contains(r#""disableBitLocker": true"#));
        assert!(profile_snapshot.contains(r#""rebootAfterDisableBitLocker": true"#));

        let readme = fs::read_to_string(profile_path.join("PPKG-README.txt")).unwrap();
        let bitlocker_readme_index = readme
            .find("BitLocker disable on C: before applications")
            .unwrap();
        let apps_readme_index = readme.find("Applications with item progress").unwrap();
        assert!(bitlocker_readme_index < apps_readme_index);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_orchestrator_uses_saved_bitlocker_reboot_choice() {
        let mut request = base_request("ProvisioningBitLockerNoReboot");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.apps.disable_bitlocker = true;
        request.apps.reboot_after_disable_bitlocker = false;

        let regional_settings = resolve_provisioning_regional_settings(&request).unwrap();
        let orchestrator =
            build_provisioning_orchestrator_script(&request, &regional_settings, false, &[]);

        assert!(orchestrator.contains("$BitLockerEnabled = $true"));
        assert!(orchestrator.contains("$BitLockerRebootAfterDisable = $false"));
        assert!(orchestrator.contains(
            "id = 'bitLocker'; title = 'BitLocker'; defaultRestart = $BitLockerRebootAfterDisable"
        ));
    }

    #[test]
    fn provisioning_package_mode_applies_uk_date_override_for_gmt_timezone() {
        let root = make_test_root("ppkg-gmt-timezone");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningGmtTimezone");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.timezone = "GMT Standard Time".to_string();

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningGmtTimezone");
        let orchestrator = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_ORCHESTRATOR_SCRIPT),
        )
        .unwrap();
        assert!(orchestrator.contains("if ($settings.timeZone -eq 'GMT Standard Time') {"));
        assert!(orchestrator.contains("Set-UkDateFormat"));
        assert!(orchestrator.contains(
            "Set-ItemProperty -Path $intlPath -Name 'sShortDate' -Value 'dd/MM/yyyy' -Force"
        ));
        assert!(orchestrator.contains("Set-TimeZone -Id $settings.timeZone -ErrorAction Stop"));

        let profile_snapshot = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_UI_PROFILE_FILE),
        )
        .unwrap();
        assert!(profile_snapshot.contains(r#""timezone": "GMT Standard Time""#));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn provisioning_package_mode_emits_powershell_that_parses_cleanly() {
        let root = make_test_root("ppkg-parse");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningParse");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.enable_debloat = true;
        request.domain_join.enabled = true;
        request.domain_join.domain = "contoso.local".to_string();
        request.domain_join.username = "CONTOSO\\join".to_string();
        request.domain_join.password = "Secret123!".to_string();
        request.wifi = OobeWifiConfig {
            enabled: true,
            ssid: "CorpWiFi".to_string(),
            password: "WirelessP@ss123".to_string(),
            authentication: "Wpa2Psk".to_string(),
            encryption: "Aes".to_string(),
            auto_connect: true,
            hidden_network: false,
            dns_server_1: "10.0.0.10".to_string(),
            dns_server_2: "10.0.0.11".to_string(),
        };
        request.apps.enable_custom_scripts = true;
        request.apps.custom_scripts = vec![OobeCustomScript {
            name: "PostDeploy-Hardening".to_string(),
            content: "Write-Host 'Hardening complete'".to_string(),
            enabled: true,
            continue_on_error: true,
        }];

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningParse");
        let scripts = collect_powershell_scripts(&profile_path);
        assert!(
            !scripts.is_empty(),
            "expected generated provisioning profile to contain PowerShell scripts"
        );
        for script in &scripts {
            assert_powershell_script_parses(script);
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_package_mode_generates_wifi_and_domain_phase_scripts() {
        let root = make_test_root("ppkg-domain-wifi");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningDomainWifi");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.domain_join.enabled = true;
        request.domain_join.domain = "contoso.local".to_string();
        request.domain_join.username = "CONTOSO\\join".to_string();
        request.domain_join.password = "Secret123!".to_string();
        request.wifi = OobeWifiConfig {
            enabled: true,
            ssid: "CorpWiFi".to_string(),
            password: "WirelessP@ss123".to_string(),
            authentication: "Wpa2Psk".to_string(),
            encryption: "Aes".to_string(),
            auto_connect: true,
            hidden_network: false,
            dns_server_1: "10.0.0.10".to_string(),
            dns_server_2: "10.0.0.11".to_string(),
        };

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningDomainWifi");
        assert!(profile_path
            .join("Scripts")
            .join("domainjoin.ps1")
            .is_file());
        assert!(profile_path
            .join("Scripts")
            .join(PROVISIONING_WIFI_SCRIPT)
            .is_file());
        let orchestrator = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_ORCHESTRATOR_SCRIPT),
        )
        .unwrap();
        assert!(orchestrator.contains("Invoke-WifiStep"));
        assert!(orchestrator.contains("Invoke-DomainJoinStep"));
        assert!(orchestrator.contains("Invoke-BitOSDTScript -Name $WifiScriptName"));
        assert!(orchestrator.contains("Invoke-BitOSDTScript -Name 'domainjoin.ps1'"));
        assert!(orchestrator.contains("Ensure-Connectivity"));
        assert!(orchestrator
            .contains("if ($DomainJoinEnabled) { $true } else { [bool]$Command.restartNow }"));

        let wifi_script =
            fs::read_to_string(profile_path.join("Scripts").join(PROVISIONING_WIFI_SCRIPT))
                .unwrap();
        assert!(wifi_script.contains("$dnsServers = @('10.0.0.10', '10.0.0.11')"));
        assert!(wifi_script.contains("Set-DnsClientServerAddress -InterfaceIndex $wifiAdapter.ifIndex -ServerAddresses $dnsServers -ErrorAction Stop"));

        let domain_script =
            fs::read_to_string(profile_path.join("Scripts").join("domainjoin.ps1")).unwrap();
        assert!(domain_script
            .contains("Domain DNS lookup failed for $DomainName on attempt $attempt of 10."));
        assert!(domain_script.contains("Domain $DomainName could not be resolved. Check the active network and DNS settings before retrying."));
        assert!(!domain_script.contains("$params['NewName']"));

        let ui_profile = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_UI_PROFILE_FILE),
        )
        .unwrap();
        assert!(ui_profile.contains(r#""wifiDnsServers": ["#));
        assert!(ui_profile.contains(r#""10.0.0.10""#));
        assert!(ui_profile.contains(r#""10.0.0.11""#));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_package_mode_skips_post_sign_in_wifi_and_domain_when_native_support_applies() {
        let root = make_test_root("ppkg-native-domain-wifi");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningNativeDomainWifi");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.oobe_config.computer_name = Some("BRANCH-01".to_string());
        request.prompt_for_computer_name = false;
        request.domain_join.enabled = true;
        request.domain_join.domain = "contoso.local".to_string();
        request.domain_join.username = "CONTOSO\\join".to_string();
        request.domain_join.password = "Secret123!".to_string();
        request.domain_join.ou_path = None;
        request.wifi = OobeWifiConfig {
            enabled: true,
            ssid: "CorpWiFi".to_string(),
            password: "WirelessP@ss123".to_string(),
            authentication: "Wpa2Psk".to_string(),
            encryption: "Aes".to_string(),
            auto_connect: true,
            hidden_network: false,
            dns_server_1: String::new(),
            dns_server_2: String::new(),
        };

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningNativeDomainWifi");
        assert!(!profile_path.join("Scripts").join("domainjoin.ps1").exists());
        assert!(!profile_path
            .join("Scripts")
            .join(PROVISIONING_WIFI_SCRIPT)
            .exists());

        let orchestrator = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_ORCHESTRATOR_SCRIPT),
        )
        .unwrap();
        assert!(orchestrator.contains("$DomainJoinEnabled = $false"));
        assert!(orchestrator.contains("$WifiEnabled = $false"));

        let ui_profile = fs::read_to_string(
            profile_path
                .join("Scripts")
                .join(PROVISIONING_UI_PROFILE_FILE),
        )
        .unwrap();
        assert!(ui_profile.contains(r#""computerNameNativeApplied": true"#));
        assert!(ui_profile.contains(r#""wifiNativeProfileApplied": true"#));
        assert!(ui_profile.contains(r#""domainJoinNativeApplied": true"#));
        assert!(ui_profile.contains(r#""wifiEnabled": false"#));
        assert!(ui_profile.contains(r#""domainJoinEnabled": false"#));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_profile_rejects_invalid_wifi_dns_values() {
        let root = make_test_root("wifi-dns-invalid");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProfileWifiDnsInvalid");
        request.wifi.enabled = true;
        request.wifi.ssid = "CorpWiFi".to_string();
        request.wifi.password = "WirelessP@ss123".to_string();
        request.wifi.dns_server_1 = "not-an-ip".to_string();

        let err = create_profile_with_root(&root, request).unwrap_err();
        assert!(err.contains("Primary Wi-Fi DNS"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removable_media_installer_paths_are_normalized_into_bitosdt_apps() {
        let root = make_test_root("removable-apps");
        fs::create_dir_all(&root).unwrap();

        let mut request = base_request("ProfileRemovableApps");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.apps.custom_installers = vec![
            OobeCustomInstaller {
                name: "USB MSI".to_string(),
                path: r"D:\Apps\setup.msi".to_string(),
                source_type: Some("DirectPathOrUrl".to_string()),
                source_file_name: None,
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "/qn /norestart".to_string(),
                installer_type: "Msi".to_string(),
                enabled: true,
            },
            OobeCustomInstaller {
                name: "Nested EXE".to_string(),
                path: r"E:\Apps\Vendor\setup.exe".to_string(),
                source_type: Some("DirectPathOrUrl".to_string()),
                source_file_name: None,
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "/quiet".to_string(),
                installer_type: "Exe".to_string(),
                enabled: true,
            },
            OobeCustomInstaller {
                name: "Leave Alone".to_string(),
                path: r"D:\Installers\setup.exe".to_string(),
                source_type: Some("DirectPathOrUrl".to_string()),
                source_file_name: None,
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "/quiet".to_string(),
                installer_type: "Exe".to_string(),
                enabled: true,
            },
        ];

        create_profile_with_root(&root, request).unwrap();

        let install_script = fs::read_to_string(
            root.join("ProfileRemovableApps")
                .join("Scripts")
                .join("installapps.ps1"),
        )
        .unwrap();
        assert!(install_script.contains(r#"C:\BitOSDT\Apps\setup.msi"#));
        assert!(install_script.contains(r#"C:\BitOSDT\Apps\Vendor\setup.exe"#));
        assert!(install_script.contains(r#"D:\Installers\setup.exe"#));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_root_wins_when_same_profile_exists_in_legacy_root() {
        let canonical_root = make_test_root("provisioning-root");
        let legacy_root = make_test_root("legacy-root");
        let profile_name = "SharedProfile";
        fs::create_dir_all(canonical_root.join(profile_name)).unwrap();
        fs::create_dir_all(legacy_root.join(profile_name)).unwrap();
        fs::write(
            canonical_root.join(profile_name).join(PROFILE_MANIFEST_FILE),
            format!(
                r#"{{"schemaVersion":3,"name":"{0}","description":"canonical","createdAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-02T00:00:00Z","request":{{"name":"{0}"}}}}"#,
                profile_name
            ),
        )
        .unwrap();
        fs::write(
            legacy_root.join(profile_name).join(PROFILE_MANIFEST_FILE),
            format!(
                r#"{{"schemaVersion":3,"name":"{0}","description":"legacy","createdAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z","request":{{"name":"{0}"}}}}"#,
                profile_name
            ),
        )
        .unwrap();

        let resolved =
            resolve_profile_path_with_roots(&canonical_root, Some(&legacy_root), profile_name)
                .expect("profile should resolve");
        assert!(resolved.starts_with(&canonical_root));

        let report =
            preflight_profile_with_roots(&canonical_root, Some(&legacy_root), profile_name)
                .expect("preflight should succeed");
        assert!(report
            .profile_path
            .starts_with(&canonical_root.to_string_lossy().to_string()));

        let _ = fs::remove_dir_all(canonical_root);
        let _ = fs::remove_dir_all(legacy_root);
    }

    #[test]
    fn first_logon_mode_prompts_for_name_when_missing_even_if_toggle_off() {
        let root = make_test_root("firstlogon-prompt-auto");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("FirstLogonPromptAuto");
        request.trigger_mode = TriggerMode::FirstLogonUsbScan;
        request.prompt_for_computer_name = false;
        request.oobe_config.computer_name = None;

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("FirstLogonPromptAuto");
        assert!(profile_path
            .join("Scripts")
            .join(USB_ORCHESTRATOR_SCRIPT)
            .is_file());
        let xml = fs::read_to_string(profile_path.join(AUTOUNATTEND_FILE)).unwrap();
        assert!(xml.contains("Bootstrap USB OOBE payload"));
        let orchestrator =
            fs::read_to_string(profile_path.join("Scripts").join(USB_ORCHESTRATOR_SCRIPT)).unwrap();
        assert!(orchestrator.contains("$PromptForComputerName = $true"));
        assert!(orchestrator.contains("$HidePrivacySettings = $true"));
        assert!(orchestrator.contains("DisablePrivacyExperience"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn provisioning_package_mode_allows_privacy_screen_when_toggle_is_off() {
        let root = make_test_root("ppkg-privacy-off");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningPrivacyOff");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.oobe_config.hide_privacy_settings = false;

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("ProvisioningPrivacyOff");
        let bootstrap =
            fs::read_to_string(profile_path.join("Apply-BitOSDTProvisioning.ps1")).unwrap();
        assert!(bootstrap.contains("$HidePrivacySettings = $false"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn first_logon_mode_skips_prompt_when_computer_name_is_explicit() {
        let root = make_test_root("firstlogon-prompt-skip");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("FirstLogonPromptSkip");
        request.trigger_mode = TriggerMode::FirstLogonUsbScan;
        request.prompt_for_computer_name = false;
        request.oobe_config.computer_name = Some("BRANCHPC01".to_string());

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("FirstLogonPromptSkip");
        let orchestrator =
            fs::read_to_string(profile_path.join("Scripts").join(USB_ORCHESTRATOR_SCRIPT)).unwrap();
        assert!(orchestrator.contains("$PromptForComputerName = $false"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn first_logon_mode_rejects_missing_default_user() {
        let root = make_test_root("firstlogon-no-default-user");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("UsbNoDefaultUser");
        request.default_user.enabled = false;

        let err = create_profile_with_root(&root, request).unwrap_err();
        assert!(err.contains("default local administrator"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn first_logon_mode_rejects_non_admin_default_user() {
        let root = make_test_root("firstlogon-standard-default-user");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("UsbStandardUser");
        request.default_user.group = "Users".to_string();

        let err = create_profile_with_root(&root, request).unwrap_err();
        assert!(err.contains("Administrators group"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preflight_provisioning_mode_skips_unattend_and_first_logon_script_requirements() {
        let root = make_test_root("ppkg-preflight");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProvisioningPreflightProfile");
        request.trigger_mode = TriggerMode::ProvisioningPackage;
        request.prompt_for_computer_name = true;

        create_profile_with_root(&root, request).unwrap();
        let report = preflight_profile_with_root(&root, "ProvisioningPreflightProfile").unwrap();

        assert!(!report
            .warnings
            .iter()
            .any(|w| w.contains("Autounattend.xml")));
        assert!(!report
            .warnings
            .iter()
            .any(|w| w.contains("prompt-pcname.ps1")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preflight_warns_when_inline_ppkg_export_is_missing_regenerated_provisioning_assets() {
        let root = make_test_root("ppkg-inline-warning");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("InlineExportProfile");
        request.trigger_mode = TriggerMode::FirstLogonUsbScan;

        create_profile_with_root(&root, request).unwrap();

        let profile_path = root.join("InlineExportProfile");
        fs::write(profile_path.join("InlineExportProfile.ppkg"), "stub").unwrap();

        let report = preflight_profile_with_root(&root, "InlineExportProfile").unwrap();
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("provisioning sidecar assets")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_manifest_defaults_trigger_mode_for_legacy_manifests() {
        let root = make_test_root("manifest-migration");
        let profile_path = root.join("LegacyProfile");
        fs::create_dir_all(&profile_path).unwrap();
        fs::write(profile_path.join(AUTOUNATTEND_FILE), "<xml />").unwrap();
        let legacy_manifest = r#"{
  "schemaVersion": 1,
  "name": "LegacyProfile",
  "description": "",
  "createdAt": "2024-01-01T00:00:00Z",
  "updatedAt": "2024-01-01T00:00:00Z",
  "request": {
    "name": "LegacyProfile",
    "description": "",
    "overwrite": false,
    "oobeConfig": {},
    "domainJoin": {},
    "domainJoinMode": "SpecializeXml",
    "promptForComputerName": false,
    "defaultUser": {},
    "wifi": {},
    "apps": {},
    "enableDebloat": false,
    "debloatScriptContent": ""
  }
}"#;
        fs::write(profile_path.join(PROFILE_MANIFEST_FILE), legacy_manifest).unwrap();

        let manifest = read_manifest(&profile_path).unwrap();
        assert_eq!(
            manifest.request.trigger_mode,
            TriggerMode::FirstLogonUsbScan
        );
        assert_eq!(manifest.schema_version, 1);
        assert!(!manifest.request.apps.disable_bitlocker);
        assert!(!manifest.request.apps.reboot_after_disable_bitlocker);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn domain_username_round_trips_through_json_and_script_generation() {
        let request = OobeProfileRequest {
            name: "RoundTripProfile".to_string(),
            domain_join: DomainJoinUiConfig {
                enabled: true,
                domain: "contoso.local".to_string(),
                username: r"drake\svc_script_runner".to_string(),
                password: "Secret123!".to_string(),
                ou_path: None,
            },
            ..Default::default()
        };
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains(r#"drake\\svc_script_runner"#));

        let round_trip: OobeProfileRequest = serde_json::from_str(&serialized).unwrap();
        let script = build_post_rename_domain_join_script(&round_trip.domain_join, false);
        assert!(script.contains(r"$Username = 'drake\svc_script_runner'"));
    }

    #[test]
    fn post_rename_mode_creates_domain_join_script_and_usb_orchestrator_cleanup() {
        let root = make_test_root("post-rename");
        fs::create_dir_all(&root).unwrap();
        let mut request = base_request("ProfileThree");
        request.domain_join = DomainJoinUiConfig {
            enabled: true,
            domain: "contoso.local".to_string(),
            username: "CONTOSO\\join".to_string(),
            password: "Secret123!".to_string(),
            ou_path: Some("OU=Computers,DC=contoso,DC=local".to_string()),
        };
        request.domain_join_mode = DomainJoinMode::PostRenameScript;
        request.enable_debloat = true;

        create_profile_with_root(&root, request).unwrap();
        let scripts = root.join("ProfileThree").join("Scripts");
        assert!(scripts.join("domainjoin.ps1").is_file());
        let orchestrator = fs::read_to_string(scripts.join(USB_ORCHESTRATOR_SCRIPT)).unwrap();
        assert!(orchestrator.contains("Clear-Autologon"));
        assert!(orchestrator.contains("Disable-BuiltinAdministrator"));
        assert!(orchestrator.contains("Restart-Computer -Force"));
        assert!(orchestrator.contains("Invoke-BitOSDTScript -Name 'domainjoin.ps1'"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn preflight_warns_for_legacy_usb_profile_regeneration() {
        let root = make_test_root("legacy-usb-preflight");
        let profile_path = root.join("LegacyUsbProfile");
        fs::create_dir_all(profile_path.join("Scripts")).unwrap();
        fs::write(
            profile_path.join(AUTOUNATTEND_FILE),
            r#"<unattend><FirstLogonCommands>Stage AutoUnattend payload Prompt for computer name Join domain</FirstLogonCommands></unattend>"#,
        )
        .unwrap();
        fs::write(
            profile_path.join(PROFILE_MANIFEST_FILE),
            r#"{
  "schemaVersion": 2,
  "name": "LegacyUsbProfile",
  "description": "",
  "createdAt": "2024-01-01T00:00:00Z",
  "updatedAt": "2024-01-01T00:00:00Z",
  "request": {
    "name": "LegacyUsbProfile",
    "description": "",
    "overwrite": false,
    "triggerMode": "FirstLogonUsbScan",
    "oobeConfig": {},
    "domainJoin": {},
    "domainJoinMode": "PostRenameScript",
    "promptForComputerName": true,
    "defaultUser": {
      "enabled": true,
      "username": "localadmin",
      "password": "Password123!",
      "group": "Administrators"
    },
    "wifi": {},
    "apps": {},
    "language": "en-US",
    "inputLocale": "0409:00000409",
    "timezone": "Pacific Standard Time",
    "enableDebloat": false,
    "debloatScriptContent": ""
  }
}"#,
        )
        .unwrap();

        let report = preflight_profile_with_root(&root, "LegacyUsbProfile").unwrap();
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("Legacy USB OOBE profile detected")));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("Legacy USB FirstLogonCommands layout detected")));

        let _ = fs::remove_dir_all(root);
    }
}
