use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OsType {
    Windows10,
    Windows11,
    WindowsServer2022,
    WindowsServer2025,
    Other,
}

impl OsType {
    pub fn display_name(&self) -> &'static str {
        match self {
            OsType::Windows10 => "Windows 10",
            OsType::Windows11 => "Windows 11",
            OsType::WindowsServer2022 => "Windows Server 2022",
            OsType::WindowsServer2025 => "Windows Server 2025",
            OsType::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Architecture {
    #[serde(rename = "x64")]
    X64,
    #[serde(rename = "arm64")]
    Arm64,
}

impl Architecture {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum LicenseType {
    Home,
    Pro,
    Enterprise,
    Education,
    Ltsc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ActivationType {
    Retail,
    Volume,
    Oem,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageStatus {
    Draft,
    Ready,
    Building,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub os_info: OsInfo,
    pub license: LicenseInfo,
    pub status: ImageStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub built_at: Option<DateTime<Utc>>,
    pub workspace_path: Option<PathBuf>,
    pub wim_path: Option<PathBuf>,
    pub iso_path: Option<PathBuf>,
    pub config: DeployConfig,
    #[serde(default)]
    pub wizard_state_json: Option<serde_json::Value>,
    pub size_bytes: Option<u64>,
    pub hash_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub os_type: OsType,
    pub version: String,
    pub architecture: Architecture,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub license_type: LicenseType,
    pub activation_type: Option<ActivationType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub target_disk: Option<u32>,
    pub uefi: bool,
    pub interactive: bool,
    pub cleanup: bool,
    pub wim_path: Option<PathBuf>,
    pub os_version: String,
    pub driver_prefs: DriverPreferences,
    #[serde(default)]
    pub runtime_driver_context: Option<RuntimeDriverContext>,
    pub unattend: Option<PathBuf>,
    pub tasks: Option<Vec<Task>>,
    pub autopilot: Option<AutopilotConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPreferences {
    pub use_driverpacks: bool,
    pub use_cloud_drivers_post_deploy: bool,
    pub allow_unsigned_drivers: bool,
    pub offline_driver_cache: Option<PathBuf>,
    pub embed_drivers_in_winpe: bool,
    #[serde(default)]
    pub runtime_driver_policy: RuntimeDriverPolicy,
}

impl Default for DriverPreferences {
    fn default() -> Self {
        Self {
            use_driverpacks: true,
            use_cloud_drivers_post_deploy: false,
            allow_unsigned_drivers: true,
            offline_driver_cache: None,
            embed_drivers_in_winpe: false,
            runtime_driver_policy: RuntimeDriverPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDriverSource {
    DriverpackCache,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDriverFailurePolicy {
    Continue,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeDriverPolicy {
    pub enabled: bool,
    pub source: RuntimeDriverSource,
    pub refresh_catalog_online: bool,
    pub bundle_common_boot_drivers: bool,
    pub failure_policy: RuntimeDriverFailurePolicy,
}

impl Default for RuntimeDriverPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            source: RuntimeDriverSource::DriverpackCache,
            refresh_catalog_online: true,
            bundle_common_boot_drivers: true,
            failure_policy: RuntimeDriverFailurePolicy::Continue,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeDriverContext {
    pub embedded_catalog_path: Option<PathBuf>,
    pub staged_cache_path: Option<PathBuf>,
    pub cache_download_base_url: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub resolved_manifest_path: Option<PathBuf>,
    pub common_boot_driver_directory: Option<PathBuf>,
    #[serde(default)]
    pub prompt_unc_credentials_at_runtime: Option<bool>,
}

impl RuntimeDriverContext {
    pub fn winpe_default() -> Self {
        Self {
            embedded_catalog_path: Some(PathBuf::from(r"X:\BitOSDT\Config\driverpacks.json")),
            staged_cache_path: Some(PathBuf::from(r"X:\BitOSDT\DriverCache")),
            cache_download_base_url: None,
            working_directory: Some(PathBuf::from(r"X:\BitOSDT\DriverCache\working")),
            resolved_manifest_path: Some(PathBuf::from(
                r"X:\BitOSDT\State\runtime-driver-resolution.json",
            )),
            common_boot_driver_directory: Some(PathBuf::from(
                r"X:\BitOSDT\DriverCache\common-boot",
            )),
            prompt_unc_credentials_at_runtime: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDriverConfig {
    pub os_version: String,
    #[serde(default)]
    pub runtime_driver_policy: RuntimeDriverPolicy,
    #[serde(default)]
    pub runtime_driver_context: RuntimeDriverContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDriverManifest {
    pub hardware_manufacturer: String,
    pub hardware_model: String,
    pub os_version: String,
    pub matched_driverpack: Option<DriverPack>,
    pub archive_path: Option<PathBuf>,
    pub extracted_path: Option<PathBuf>,
    pub source: Option<String>,
    pub prepared: bool,
    pub installed_count: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub name: String,
    pub task_type: TaskType,
    pub command: String,
    pub arguments: Vec<String>,
    pub run_once: bool,
    pub requires_reboot: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    InstallApplication,
    RunScript,
    CopyFiles,
    CreateUser,
    DomainJoin,
    RenameComputer,
    RegistryModify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfig {
    pub tenant_id: String,
    pub app_id: String,
    pub profile_json: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub manufacturer: String,
    pub model: String,
    pub product: String,
    pub serial_number: String,
    pub uuid: String,
    pub architecture: Architecture,
    pub form_factor: FormFactor,
    pub is_vm: bool,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub network_adapters: Vec<NetworkAdapterInfo>,
    pub bios: BiosInfo,
    pub chassis_type: Option<u16>,
    pub has_battery: bool,
    pub tpm: Option<TpmInfo>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FormFactor {
    Laptop,
    Desktop,
    Server,
    Tablet,
    SmallFormFactor,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub name: String,
    pub manufacturer: String,
    pub cores: u32,
    pub logical_processors: u32,
    pub max_speed_mhz: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub total_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub index: u32,
    pub model: String,
    pub size_bytes: u64,
    pub size_gb: f64,
    pub media_type: String,
    pub interface_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAdapterInfo {
    pub name: String,
    pub mac_address: String,
    pub adapter_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiosInfo {
    pub manufacturer: String,
    pub version: String,
    pub serial_number: String,
    pub release_date: String,
    pub smbios_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmInfo {
    pub is_activated_initial_value: bool,
    pub is_enabled_initial_value: bool,
    pub is_owned_initial_value: bool,
    pub spec_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPack {
    pub id: String,
    pub manufacturer: String,
    pub product: String,
    pub model: String,
    pub os: String,
    pub os_version: String,
    pub os_build: Option<String>,
    pub architecture: Architecture,
    pub name: String,
    pub filename: String,
    pub url: String,
    pub hash_md5: String,
    pub hash_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub release_date: Option<String>,
    pub catalog_version: String,
    pub last_synced: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSyncStatus {
    pub manufacturer: String,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_sync_success: bool,
    pub entry_count: u32,
    pub source_url: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub default_language: String,
    pub theme: String,
    pub auto_check_updates: bool,
    pub download_path: PathBuf,
    pub workspace_path: PathBuf,
    pub adk_path: Option<PathBuf>,
    #[serde(default)]
    pub suppress_credential_warning: bool,
}
