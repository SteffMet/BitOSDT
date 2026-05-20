use bitosdt::build::{
    build_image_with_context, ensure_linux_build_prerequisites,
    resolve_default_lightweight_host_settings, runtime_executable_from_assets,
    serve_lightweight_tree, sync_winpe_asset_bundle, ImageBuildContext, ImageBuildRequest,
    UsbWriter, WinPEBuilder,
};
use bitosdt::catalog::{get_builtin_os_catalog, CatalogSyncService, OsCatalogSyncService};
use bitosdt::core::models::{
    Architecture, DriverPreferences, Image, ImageStatus, LicenseInfo, LicenseType, OsInfo,
    RuntimeDriverConfig,
};
use bitosdt::core::{Config, Database};
use bitosdt::deploy::{
    prepare_runtime_drivers, run_winpe_deploy, DeploymentEngine, DeploymentProgress,
    HardwareDetector, WinpeDeployOptions,
};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::io::{self, Write};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "bitosdt")]
#[command(about = "BitOSDT 2.0 - Windows Deployment Solution")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize BitOSDT configuration
    Init,

    /// Sync driver catalogs from OSDCloud
    SyncCatalogs {
        /// Specific manufacturer to sync (Dell, HP, Lenovo, Microsoft)
        #[arg(short, long)]
        manufacturer: Option<String>,
    },

    /// Sync OS catalog from OSDCloud (Windows 10/11 versions)
    SyncOsCatalogs,

    /// List available OS versions
    OsList {
        /// Filter by language code (e.g., en-us, de-de, fr-fr)
        #[arg(short, long)]
        language: Option<String>,

        /// Filter by release version (e.g., 24H2, 23H2, 22H2)
        #[arg(short, long)]
        release: Option<String>,

        /// Filter by OS (Windows 10 or Windows 11)
        #[arg(short, long)]
        os: Option<String>,

        /// Filter by architecture (x64, arm64)
        #[arg(short, long)]
        arch: Option<String>,
    },

    /// Create a new image
    CreateImage {
        /// Image name
        #[arg(short, long)]
        name: String,

        /// OS version (win11-24h2, win11-23h2, win10-22h2)
        #[arg(short, long)]
        os: String,

        /// License type (Home, Pro, Enterprise)
        #[arg(short, long)]
        license: LicenseArg,
    },

    /// List all images
    ListImages,

    /// Detect and show hardware information
    Hardware,

    /// Find matching DriverPack for current hardware
    MatchDrivers {
        /// Target OS version (24H2, 23H2, 22H2)
        #[arg(short, long)]
        os: String,
    },

    /// Build WinPE image
    BuildWinpe {
        /// Output directory
        #[arg(short, long, default_value = "winpe")]
        output: String,

        /// Architecture (x64, arm64)
        #[arg(short, long, default_value = "x64")]
        arch: String,

        /// Windows ADK installation path (optional)
        #[arg(long)]
        adk_path: Option<String>,
    },

    /// Build a BitOSDT image from the terminal
    BuildImage {
        /// Run an interactive terminal flow
        #[arg(long, default_value_t = false)]
        interactive: bool,

        /// Source mode: cloud or local
        #[arg(long)]
        source: Option<BuildSourceArg>,

        /// Local ISO/ESD/WIM source path
        #[arg(long)]
        source_path: Option<String>,

        /// OS family label (for example Windows 11)
        #[arg(long)]
        os: Option<String>,

        /// Release/build train (for example 24H2)
        #[arg(long)]
        release: Option<String>,

        /// Edition label (for example Pro or Enterprise)
        #[arg(long)]
        edition: Option<String>,

        /// Language code
        #[arg(long)]
        language: Option<String>,

        /// CPU architecture
        #[arg(long)]
        arch: Option<String>,

        /// Override the cloud download URL directly
        #[arg(long)]
        download_url: Option<String>,

        /// Output type
        #[arg(long)]
        output_type: Option<OutputTypeArg>,

        /// Output ISO path
        #[arg(long)]
        output: Option<String>,

        /// ISO volume label
        #[arg(long)]
        volume_label: Option<String>,

        /// Advanced lightweight runtime server URL
        #[arg(long)]
        server_url: Option<String>,

        /// Lightweight publish/export path
        #[arg(long)]
        pxe_export_path: Option<String>,

        /// Local driver path(s)
        #[arg(long)]
        driver_path: Vec<String>,

        /// Create a local administrator account in the built image
        #[arg(long)]
        local_admin_user: Option<String>,

        /// Password for the local administrator account
        #[arg(long)]
        local_admin_password: Option<String>,

        /// Optional display name for the local administrator account
        #[arg(long)]
        local_admin_display_name: Option<String>,

        /// Chocolatey package(s) to install after deployment
        #[arg(long)]
        choco_package: Vec<String>,

        /// Convenience flag to install Google Chrome via Chocolatey
        #[arg(long, default_value_t = false)]
        install_chrome: bool,

        /// Path to a Linux WinPE asset bundle
        #[arg(long)]
        winpe_assets: Option<String>,

        /// Include the GUI runtime when available
        #[arg(long, default_value_t = false)]
        include_gui: bool,
    },

    /// Manage Linux WinPE asset bundles
    WinpeAssets {
        #[command(subcommand)]
        command: WinpeAssetCommand,
    },

    /// Serve the staged lightweight publish tree over HTTP from the terminal
    LightweightHost {
        #[command(subcommand)]
        command: LightweightHostCommand,
    },

    /// List available USB devices
    ListUsb,

    /// Deploy image to disk
    Deploy {
        /// Image ID to deploy
        #[arg(short, long)]
        image: String,

        /// Target disk number
        #[arg(short, long)]
        disk: u32,

        /// Use UEFI (default) or BIOS
        #[arg(long, default_value = "true")]
        uefi: bool,
    },

    /// Resolve and optionally inject runtime drivers in WinPE
    RuntimeDrivers {
        /// Path to WinPE deploy/runtime config JSON
        #[arg(long)]
        config: String,

        /// Offline Windows path to inject into
        #[arg(long)]
        windows_path: Option<String>,

        /// Only prepare and stage the matching driver pack
        #[arg(long, default_value = "false")]
        prepare_only: bool,

        /// Optional runtime server URL override (used for lightweight cache fetches)
        #[arg(long)]
        server_url: Option<String>,
    },

    /// Execute the native WinPE deployment runtime
    #[command(hide = true)]
    WinpeDeploy {
        /// Path to deploy.json inside WinPE
        #[arg(long, default_value = "X:\\BitOSDT\\Config\\deploy.json")]
        config: String,

        /// Path to runtime-drivers.json inside WinPE
        #[arg(long, default_value = "X:\\BitOSDT\\Config\\runtime-drivers.json")]
        runtime_driver_config: String,

        /// Override the runtime deploy log path
        #[arg(long, default_value = "X:\\BitOSDT\\Logs\\deploy.log")]
        log_path: String,

        /// Override the runtime deploy status path
        #[arg(long, default_value = "X:\\BitOSDT\\State\\deploy-status.json")]
        status_path: String,

        /// Do not reboot after deployment completes
        #[arg(long, default_value_t = false)]
        skip_reboot: bool,
    },

    /// Show system information
    Info,
}

#[derive(Subcommand)]
enum WinpeAssetCommand {
    /// Sync a local WinPE asset bundle into the default cache
    Sync {
        /// Source bundle path
        #[arg(long)]
        source: Option<String>,

        /// Target cache path override
        #[arg(long)]
        target: Option<String>,
    },
}

#[derive(Subcommand)]
enum LightweightHostCommand {
    /// Serve the lightweight publish tree until interrupted
    Serve {
        /// Staging directory to serve
        #[arg(long)]
        staging: Option<String>,

        /// Bind address for the HTTP host
        #[arg(long)]
        bind: Option<String>,

        /// Base URL advertised in health/manifest responses
        #[arg(long)]
        base_url: Option<String>,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum LicenseArg {
    Home,
    Pro,
    Enterprise,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum BuildSourceArg {
    Cloud,
    Local,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum OutputTypeArg {
    FullIso,
    Lightweight,
    Both,
}

impl LicenseArg {
    fn to_license_type(self) -> LicenseType {
        match self {
            LicenseArg::Home => LicenseType::Home,
            LicenseArg::Pro => LicenseType::Pro,
            LicenseArg::Enterprise => LicenseType::Enterprise,
        }
    }
}

fn get_db() -> Option<(Database, Config)> {
    let config = Config::load().ok()?;
    let db = Database::new(&config.database_path).ok()?;
    Some((db, config))
}

fn prompt_line(label: &str, default: Option<&str>) -> Result<String, String> {
    print!(
        "{}{}: ",
        label,
        default
            .map(|value| format!(" [{}]", value))
            .unwrap_or_default()
    );
    io::stdout()
        .flush()
        .map_err(|e| format!("Failed to flush prompt: {}", e))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read input: {}", e))?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.unwrap_or_default().to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn default_oobe_config() -> serde_json::Value {
    json!({
        "skipMachineOobe": true,
        "skipUserOobe": true,
        "hideEula": true,
        "hideWirelessSetup": true,
        "hideLocalAccountScreen": false,
        "hideOnlineAccountScreens": true,
        "networkLocation": "Work",
        "protectYourPc": "Recommended",
        "computerName": null
    })
}

fn default_apps_config() -> serde_json::Value {
    json!({
        "wingetPackages": [],
        "chocolateyPackages": [],
        "customInstallers": [],
        "copiedItems": [],
        "copyDestination": null,
        "autoInstallChocolatey": true,
        "continueOnError": true,
        "enableCustomScripts": false,
        "customScripts": []
    })
}

fn default_windows_update_config() -> serde_json::Value {
    json!({
        "enabled": false,
        "installSecurityUpdates": false,
        "installCriticalUpdates": false,
        "installDriverUpdates": false,
        "excludePreview": true,
        "excludeOptional": false,
        "rebootBehavior": "AutoReboot"
    })
}

fn output_type_label(value: OutputTypeArg) -> &'static str {
    match value {
        OutputTypeArg::FullIso => "FullISO",
        OutputTypeArg::Lightweight => "LightweightISO",
        OutputTypeArg::Both => "Both",
    }
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_os_label(value: &str) -> String {
    if value.to_ascii_lowercase().contains("10") {
        "Windows 10".to_string()
    } else {
        "Windows 11".to_string()
    }
}

fn normalize_arch_label(value: Option<String>) -> String {
    match value
        .as_deref()
        .unwrap_or("x64")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "amd64" | "x86_64" | "x64" => "x64".to_string(),
        "arm64" | "aarch64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

fn arch_catalog_candidates(value: &str) -> Vec<String> {
    match normalize_arch_label(Some(value.to_string())).as_str() {
        "x64" => vec!["x64".to_string(), "amd64".to_string()],
        "arm64" => vec!["arm64".to_string(), "aarch64".to_string()],
        other => vec![other.to_string()],
    }
}

fn os_catalog_candidates(value: &str) -> Vec<String> {
    match normalize_os_label(value).as_str() {
        "Windows 10" => vec!["Windows 10".to_string(), "Win10".to_string()],
        _ => vec!["Windows 11".to_string(), "Win11".to_string()],
    }
}

fn language_catalog_candidates(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return vec!["en-us".to_string()];
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower == trimmed {
        vec![lower]
    } else {
        vec![trimmed.to_string(), lower]
    }
}

fn is_placeholder_download_url(url: &str) -> bool {
    url.trim().is_empty() || url.contains("/placeholder")
}

fn resolve_cloud_entry_for_cli(
    os_name: &str,
    release: &str,
    language: &str,
    arch: &str,
) -> Result<(String, String), String> {
    let os_filter = normalize_os_label(os_name);
    let os_candidates = os_catalog_candidates(os_name);
    let arch_candidates = arch_catalog_candidates(arch);
    let language_candidates = language_catalog_candidates(language);

    if let Some((db, _config)) = get_db() {
        for os_candidate in &os_candidates {
            for arch_candidate in &arch_candidates {
                for language_candidate in &language_candidates {
                    if let Ok(entries) = db.get_os_versions_filtered(
                        Some(os_candidate),
                        Some(release),
                        Some(arch_candidate),
                        Some(language_candidate),
                    ) {
                        if let Some(entry) = entries
                            .into_iter()
                            .find(|entry| !is_placeholder_download_url(&entry.download_url))
                        {
                            return Ok((entry.display_name, entry.download_url));
                        }
                    }
                }
            }
        }
    }

    let built_in = get_builtin_os_catalog()
        .into_iter()
        .find(|entry| {
            entry.os_type.display_name().eq_ignore_ascii_case(&os_filter)
                && entry.version.eq_ignore_ascii_case(release)
                && entry
                    .languages
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(language))
                && !is_placeholder_download_url(&entry.esd_url)
        })
        .ok_or_else(|| {
            format!(
                "No synced cloud catalog entry found for {} {} {} {}. Run 'bitosdt sync-os-catalogs' first or pass --download-url.",
                os_filter, release, language, arch
            )
        })?;

    Ok((built_in.display_name(), built_in.esd_url))
}

fn resolve_latest_cloud_release_for_cli(
    os_name: &str,
    language: &str,
    arch: &str,
) -> Result<String, String> {
    let os_filter = normalize_os_label(os_name);
    let os_candidates = os_catalog_candidates(os_name);
    let arch_candidates = arch_catalog_candidates(arch);
    let language_candidates = language_catalog_candidates(language);

    if let Some((db, _config)) = get_db() {
        for os_candidate in &os_candidates {
            for arch_candidate in &arch_candidates {
                for language_candidate in &language_candidates {
                    if let Ok(entries) = db.get_os_versions_filtered(
                        Some(os_candidate),
                        None,
                        Some(arch_candidate),
                        Some(language_candidate),
                    ) {
                        if let Some(entry) = entries
                            .into_iter()
                            .find(|entry| !is_placeholder_download_url(&entry.download_url))
                        {
                            return Ok(entry.release_id);
                        }
                    }
                }
            }
        }
    }

    let latest = get_builtin_os_catalog()
        .into_iter()
        .filter(|entry| {
            entry.os_type.display_name().eq_ignore_ascii_case(&os_filter)
                && entry
                    .languages
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(language))
                && !is_placeholder_download_url(&entry.esd_url)
        })
        .max_by_key(|entry| entry.build_number.parse::<u32>().unwrap_or_default())
        .ok_or_else(|| {
            format!(
                "Unable to resolve the latest {} {} {} release from a real cloud catalog entry. Run 'bitosdt sync-os-catalogs' first or pass --release/--download-url explicitly.",
                os_filter, language, arch
            )
        })?;

    Ok(latest.version)
}

fn build_local_admin_account(
    username: String,
    password: String,
    display_name: Option<String>,
) -> serde_json::Value {
    json!({
        "username": username,
        "password": password,
        "displayName": display_name,
        "group": "Administrators",
        "passwordNeverExpires": true,
        "requirePasswordChange": false
    })
}

fn add_chocolatey_packages(
    apps: &mut serde_json::Value,
    packages: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    let package_list = apps
        .get_mut("chocolateyPackages")
        .and_then(|value| value.as_array_mut())
        .ok_or_else(|| "Default applications config is missing chocolateyPackages".to_string())?;

    for package_name in packages {
        let trimmed = package_name.trim();
        if trimmed.is_empty() {
            continue;
        }

        if package_list.iter().any(|entry| {
            entry
                .get("packageName")
                .and_then(|value| value.as_str())
                .map(|value| value.eq_ignore_ascii_case(trimmed))
                .unwrap_or(false)
        }) {
            continue;
        }

        package_list.push(json!({
            "packageName": trimmed,
            "version": null,
            "source": null,
            "customArgs": null,
            "enabled": true
        }));
    }

    Ok(())
}

fn resolve_cli_build_request(
    interactive: bool,
    source: Option<BuildSourceArg>,
    source_path: Option<String>,
    os: Option<String>,
    release: Option<String>,
    edition: Option<String>,
    language: Option<String>,
    arch: Option<String>,
    download_url: Option<String>,
    output_type: Option<OutputTypeArg>,
    output: Option<String>,
    volume_label: Option<String>,
    server_url: Option<String>,
    pxe_export_path: Option<String>,
    driver_path: Vec<String>,
    local_admin_user: Option<String>,
    local_admin_password: Option<String>,
    local_admin_display_name: Option<String>,
    choco_package: Vec<String>,
    install_chrome: bool,
    include_gui: bool,
) -> Result<ImageBuildRequest, String> {
    let mut source_mode = source;
    let mut source_path = source_path;
    let mut os_name = os;
    let mut release = release;
    let mut edition = edition;
    let mut language = language;
    let mut arch = arch;
    let mut output_type = output_type;
    let mut output = output;
    let mut local_admin_user = local_admin_user;
    let mut local_admin_password = local_admin_password;
    let mut local_admin_display_name = local_admin_display_name;
    let mut choco_package = choco_package;

    if interactive {
        if source_mode.is_none() {
            source_mode = Some(
                match prompt_line("Source mode (cloud/local)", Some("cloud"))?
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "local" => BuildSourceArg::Local,
                    _ => BuildSourceArg::Cloud,
                },
            );
        }
        if os_name.is_none() {
            os_name = Some(prompt_line("OS family", Some("Windows 11"))?);
        }
        if release.is_none() {
            release = empty_to_none(Some(prompt_line("Release (blank for latest)", None)?));
        }
        if edition.is_none() {
            edition = Some(prompt_line("Edition", Some("Pro"))?);
        }
        if language.is_none() {
            language = Some(prompt_line("Language", Some("en-US"))?);
        }
        if arch.is_none() {
            arch = Some(prompt_line("Architecture", Some("x64"))?);
        }
        if matches!(source_mode, Some(BuildSourceArg::Local)) && source_path.is_none() {
            source_path = Some(prompt_line("Local source path", None)?);
        }
        if output_type.is_none() {
            output_type = Some(
                match prompt_line("Output type (full/lightweight/both)", Some("full"))?
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "lightweight" => OutputTypeArg::Lightweight,
                    "both" => OutputTypeArg::Both,
                    _ => OutputTypeArg::FullIso,
                },
            );
        }
        if output.is_none() {
            output = Some(prompt_line("Output ISO path", Some("BitOSDT.iso"))?);
        }
        if local_admin_user.is_none() {
            local_admin_user =
                empty_to_none(Some(prompt_line("Local admin user (blank to skip)", None)?));
        }
        if local_admin_user.is_some() && local_admin_password.is_none() {
            local_admin_password = empty_to_none(Some(prompt_line("Local admin password", None)?));
        }
        if local_admin_user.is_some() && local_admin_display_name.is_none() {
            local_admin_display_name = empty_to_none(Some(prompt_line(
                "Local admin display name (blank to reuse username)",
                None,
            )?));
        }
    }

    let source_mode = source_mode.unwrap_or(BuildSourceArg::Cloud);
    let os_name = normalize_os_label(&os_name.unwrap_or_else(|| "Windows 11".to_string()));
    let edition = edition.unwrap_or_else(|| "Pro".to_string());
    let language = language.unwrap_or_else(|| "en-US".to_string());
    let arch = normalize_arch_label(arch);
    let output_type = output_type.unwrap_or(OutputTypeArg::FullIso);
    let output = output.ok_or_else(|| "--output is required".to_string())?;
    let release = if matches!(source_mode, BuildSourceArg::Cloud) {
        match empty_to_none(release) {
            Some(release) => release,
            None => resolve_latest_cloud_release_for_cli(&os_name, &language, &arch)?,
        }
    } else {
        empty_to_none(release).unwrap_or_else(|| "latest".to_string())
    };

    match (&local_admin_user, &local_admin_password) {
        (Some(_), None) => {
            return Err(
                "--local-admin-password is required when --local-admin-user is set".to_string(),
            )
        }
        (None, Some(_)) => {
            return Err(
                "--local-admin-user is required when --local-admin-password is set".to_string(),
            )
        }
        _ => {}
    }

    let (display_name, resolved_download_url) = if matches!(source_mode, BuildSourceArg::Cloud) {
        if let Some(url) = download_url {
            (os_name.clone(), Some(url))
        } else {
            let (display_name, url) =
                resolve_cloud_entry_for_cli(&os_name, &release, &language, &arch)?;
            let normalized = if display_name.to_ascii_lowercase().contains("windows 10") {
                "Windows 10".to_string()
            } else {
                "Windows 11".to_string()
            };
            (normalized, Some(url))
        }
    } else {
        let local_source = source_path
            .clone()
            .ok_or_else(|| "--source-path is required for local builds".to_string())?;
        if !PathBuf::from(&local_source).exists() {
            return Err(format!(
                "Local source path does not exist: {}",
                local_source
            ));
        }
        (os_name.clone(), None)
    };

    let mut apps = default_apps_config();
    if install_chrome {
        choco_package.push("googlechrome".to_string());
    }
    add_chocolatey_packages(&mut apps, choco_package)?;

    let mut user_accounts = Vec::new();
    if let (Some(username), Some(password)) = (local_admin_user, local_admin_password) {
        user_accounts.push(build_local_admin_account(
            username.clone(),
            password,
            local_admin_display_name.or(Some(username)),
        ));
    }

    Ok(ImageBuildRequest {
        windows_version: display_name,
        windows_build: release.clone(),
        windows_edition: edition.clone(),
        windows_channel: None, // CLI default: no channel constraint (will be inferred from edition during build)
        language: Some(language.clone()),
        output_type: output_type_label(output_type).to_string(),
        output_path: output,
        volume_label: volume_label.unwrap_or_else(|| format!("BITOSDT-{}", release)),
        source_path,
        download_url: resolved_download_url,
        target_disk: None,
        delivery_mode: Some("Simple".to_string()),
        server_url,
        driver_paths: driver_path,
        boot_driver_unc_path: None,
        apply_to_offline_windows: Some(false),
        runtime_driver_policy: Some(Default::default()),
        pxe_export_path,
        full_iso_unc_path: None,
        full_iso_unc_username: None,
        full_iso_unc_password: None,
        full_iso_http_url: None,
        prompt_unc_credentials_at_runtime: None,
        include_gui: Some(include_gui),
        existing_image_id: None,
        save_mode: Some("copy".to_string()),
        oobe_config: default_oobe_config(),
        user_accounts,
        domain_join: json!({
            "enabled": false,
            "domain": "",
            "username": "",
            "password": "",
            "ouPath": null
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
        apps,
        windows_update: default_windows_update_config(),
        group_policies: bitosdt::policy::empty_group_policy_selection_value(),
        shell_layout: bitosdt::build::empty_shell_layout_value(),
    })
}

fn resolve_cli_resource_dir(relative: &str) -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join(relative);
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

fn resolve_cli_build_context(
    winpe_assets: Option<String>,
    include_gui: bool,
) -> Result<ImageBuildContext, String> {
    let winpe_assets_dir = winpe_assets
        .map(PathBuf::from)
        .or_else(|| bitosdt::build::default_winpe_assets_path().ok());

    let runtime_executable = if cfg!(target_os = "windows") {
        std::env::current_exe().ok()
    } else {
        winpe_assets_dir
            .as_deref()
            .and_then(runtime_executable_from_assets)
    };

    if cfg!(target_os = "linux") {
        ensure_linux_build_prerequisites(true)
            .map_err(|e| format!("Linux build prerequisites are not met: {}", e))?;
    }

    Ok(ImageBuildContext {
        ui_dir: resolve_cli_resource_dir("dist"),
        winpe_packages_dir: resolve_cli_resource_dir("../WinPE-Dependencies/Packages"),
        common_boot_driver_dir: resolve_cli_resource_dir("../WinPE-Dependencies/Drivers/Common"),
        runtime_driver_catalog: get_db()
            .and_then(|(db, _)| db.get_all_driverpacks().ok())
            .unwrap_or_default(),
        native_runtime_executable: runtime_executable,
        gui_executable: if include_gui && cfg!(target_os = "windows") {
            std::env::current_exe().ok()
        } else {
            None
        },
        simple_publish_path: bitosdt::build::default_simple_publish_path().ok(),
        simple_runtime_url: Some(bitosdt::build::default_simple_runtime_url()),
        winpe_assets_dir,
        persist_built_image: true,
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    if let Err(error) = Config::ensure_app_dir_exists() {
        eprintln!("✗ Failed to initialize BitOSDT app directory: {}", error);
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            println!("BitOSDT 2.0 - Initialization");
            println!("=============================");

            match Config::load() {
                Ok(config) => {
                    println!("✓ Configuration loaded");
                    println!("  Database: {:?}", config.database_path);
                    println!("  Downloads: {:?}", config.settings.download_path);
                    println!("  Workspace: {:?}", config.settings.workspace_path);

                    // Initialize database
                    match Database::new(&config.database_path) {
                        Ok(_db) => {
                            println!("✓ Database initialized successfully");
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to initialize database: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to load configuration: {}", e);
                }
            }
        }

        Commands::SyncCatalogs { manufacturer } => {
            println!("BitOSDT 2.0 - Catalog Sync");
            println!("===========================");

            let Some((db, _config)) = get_db() else {
                eprintln!("✗ Failed to initialize database. Run 'bitosdt init' first.");
                return;
            };

            let sync_service = match CatalogSyncService::new(db) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("✗ Failed to create sync service: {}", e);
                    return;
                }
            };

            if let Some(mfr) = manufacturer {
                println!("Syncing {} catalog...", mfr);
                match sync_service.sync_manufacturer(&mfr).await {
                    Ok(status) => {
                        if status.last_sync_success {
                            println!("✓ Synced {} entries for {}", status.entry_count, mfr);
                        } else {
                            println!(
                                "✗ Sync failed for {}: {}",
                                mfr,
                                status.error_message.unwrap_or_default()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Error syncing {}: {}", mfr, e);
                    }
                }
            } else {
                println!("Syncing all catalogs...");
                match sync_service.sync_all().await {
                    Ok(results) => {
                        for status in results {
                            if status.last_sync_success {
                                println!(
                                    "✓ {}: {} entries",
                                    status.manufacturer, status.entry_count
                                );
                            } else {
                                println!(
                                    "✗ {}: {}",
                                    status.manufacturer,
                                    status.error_message.unwrap_or_default()
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Error during sync: {}", e);
                    }
                }
            }
        }

        Commands::SyncOsCatalogs => {
            println!("BitOSDT 2.0 - OS Catalog Sync");
            println!("==============================");

            let Some((db, _config)) = get_db() else {
                eprintln!("✗ Failed to initialize database. Run 'bitosdt init' first.");
                return;
            };

            let os_sync = match OsCatalogSyncService::new(db) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("✗ Failed to create OS sync service: {}", e);
                    return;
                }
            };

            println!("Syncing OS catalog from OSDCloud...");
            match os_sync.sync().await {
                Ok(status) => {
                    println!("✓ Synced {} OS versions from OSDCloud", status.entry_count);
                    println!("\nUse 'bitosdt os-list' to view available versions.");
                    println!("Use 'bitosdt os-list --language en-us' to filter by language.");
                }
                Err(e) => {
                    eprintln!("✗ Failed to sync OS catalog: {}", e);
                }
            }
        }

        Commands::OsList {
            language,
            release,
            os,
            arch,
        } => {
            println!("BitOSDT 2.0 - Available OS Versions");
            println!("=====================================");

            // Try to load from database first
            let versions: Vec<(String, String, String, String, String, String, u64)> =
                if let Some((db, _config)) = get_db() {
                    let mut db_entries = Vec::new();
                    let os_candidates = os
                        .as_deref()
                        .map(os_catalog_candidates)
                        .unwrap_or_else(|| vec!["Win11".to_string(), "Win10".to_string()]);
                    let arch_candidates = arch
                        .as_deref()
                        .map(arch_catalog_candidates)
                        .unwrap_or_else(|| vec!["amd64".to_string(), "arm64".to_string()]);
                    let language_candidates = language
                        .as_deref()
                        .map(language_catalog_candidates)
                        .unwrap_or_default();

                    for os_candidate in &os_candidates {
                        if !db_entries.is_empty() {
                            break;
                        }

                        for arch_candidate in &arch_candidates {
                            if !db_entries.is_empty() {
                                break;
                            }

                            if language_candidates.is_empty() {
                                if let Ok(entries) = db.get_os_versions_filtered(
                                    Some(os_candidate),
                                    release.as_deref(),
                                    Some(arch_candidate),
                                    None,
                                ) {
                                    db_entries = entries;
                                }
                            } else {
                                for language_candidate in &language_candidates {
                                    if let Ok(entries) = db.get_os_versions_filtered(
                                        Some(os_candidate),
                                        release.as_deref(),
                                        Some(arch_candidate),
                                        Some(language_candidate),
                                    ) {
                                        if !entries.is_empty() {
                                            db_entries = entries;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !db_entries.is_empty() {
                        db_entries
                            .into_iter()
                            .map(|e| {
                                (
                                    e.id,
                                    e.display_name,
                                    e.operating_system,
                                    e.release_id,
                                    e.language_code,
                                    e.architecture,
                                    e.size_bytes.unwrap_or(0),
                                )
                            })
                            .collect()
                    } else {
                        // Fall back to builtin catalog
                        println!(
                        "(Using built-in catalog. Run 'bitosdt sync-os-catalogs' for full list)\n"
                    );
                        get_builtin_os_catalog()
                            .into_iter()
                            .filter(|o| {
                                let os_match = os.as_ref().map_or(true, |f| {
                                    let os_name = o.os_type.display_name().to_lowercase();
                                    os_name.contains(&f.to_lowercase())
                                });
                                let release_match = release
                                    .as_ref()
                                    .map_or(true, |r| o.version.to_lowercase() == r.to_lowercase());
                                let lang_match = language.as_ref().map_or(true, |l| {
                                    o.languages
                                        .iter()
                                        .any(|lang| lang.to_lowercase() == l.to_lowercase())
                                });
                                let arch_match = arch.as_ref().map_or(true, |value| {
                                    normalize_arch_label(Some(value.clone())) == "x64"
                                });
                                os_match && release_match && lang_match && arch_match
                            })
                            .map(|o| {
                                let display = o.display_name();
                                let os_name = o.os_type.display_name().to_string();
                                let lang = o.languages.first().cloned().unwrap_or_default();
                                (
                                    o.id,
                                    display,
                                    os_name,
                                    o.version,
                                    lang,
                                    "x64".to_string(),
                                    o.size_bytes,
                                )
                            })
                            .collect()
                    }
                } else {
                    println!("(Database not available. Using built-in catalog)\n");
                    get_builtin_os_catalog()
                        .into_iter()
                        .map(|o| {
                            let display = o.display_name();
                            let os_name = o.os_type.display_name().to_string();
                            let lang = o.languages.first().cloned().unwrap_or_default();
                            (
                                o.id,
                                display,
                                os_name,
                                o.version,
                                lang,
                                "x64".to_string(),
                                o.size_bytes,
                            )
                        })
                        .collect()
                };

            if versions.is_empty() {
                println!("No OS versions found matching the filters.");
                println!("\nTry:");
                println!("  bitosdt os-list                       # Show all versions");
                println!("  bitosdt os-list --language en-us      # English versions only");
                println!("  bitosdt os-list --release 24H2        # 24H2 versions only");
                println!("  bitosdt os-list --os \"Windows 11\"     # Windows 11 only");
            } else {
                println!("Found {} OS version(s):\n", versions.len());
                for (id, display_name, _os, release, lang, arch, size) in versions.iter().take(50) {
                    println!("{}", display_name);
                    println!("  ID: {}", id);
                    println!(
                        "  Release: {} | Arch: {} | Language: {}",
                        release, arch, lang
                    );
                    println!("  Size: {:.2} GB", *size as f64 / 1_000_000_000.0);
                    println!();
                }
                if versions.len() > 50 {
                    println!(
                        "... and {} more. Use filters to narrow results.",
                        versions.len() - 50
                    );
                }
            }
        }

        Commands::CreateImage { name, os, license } => {
            println!("BitOSDT 2.0 - Create Image");
            println!("==========================");

            let Some((db, _config)) = get_db() else {
                eprintln!("✗ Failed to initialize database. Run 'bitosdt init' first.");
                return;
            };

            // Find OS version
            let catalog = get_builtin_os_catalog();
            let os_version = match catalog.iter().find(|o| o.id == os) {
                Some(o) => o.clone(),
                None => {
                    eprintln!("✗ Unknown OS version: {}", os);
                    println!("Available versions:");
                    for o in catalog {
                        println!("  - {}: {}", o.id, o.display_name());
                    }
                    return;
                }
            };

            let image = Image {
                id: Uuid::new_v4(),
                name: name.clone(),
                description: None,
                os_info: OsInfo {
                    os_type: os_version.os_type.clone(),
                    version: os_version.version.clone(),
                    architecture: Architecture::X64,
                    language: "en-US".to_string(),
                },
                license: LicenseInfo {
                    license_type: license.to_license_type(),
                    activation_type: None,
                },
                status: ImageStatus::Draft,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                built_at: None,
                workspace_path: None,
                wim_path: None,
                iso_path: None,
                config: bitosdt::core::models::DeployConfig {
                    target_disk: None,
                    uefi: true,
                    interactive: true,
                    cleanup: true,
                    wim_path: None,
                    os_version: os_version.version.clone(),
                    driver_prefs: DriverPreferences::default(),
                    runtime_driver_context: None,
                    unattend: None,
                    tasks: None,
                    autopilot: None,
                },
                wizard_state_json: None,
                size_bytes: None,
                hash_sha256: None,
            };

            match db.create_image(&image) {
                Ok(_) => {
                    println!("✓ Image created successfully");
                    println!("  ID: {}", image.id);
                    println!("  Name: {}", image.name);
                    println!(
                        "  OS: {} {}",
                        image.os_info.os_type.display_name(),
                        image.os_info.version
                    );
                    println!("  License: {:?}", image.license.license_type);
                }
                Err(e) => {
                    eprintln!("✗ Failed to create image: {}", e);
                }
            }
        }

        Commands::ListImages => {
            println!("BitOSDT 2.0 - Images");
            println!("====================");

            let Some((db, _config)) = get_db() else {
                eprintln!("✗ Failed to initialize database. Run 'bitosdt init' first.");
                return;
            };

            match db.list_images() {
                Ok(images) => {
                    if images.is_empty() {
                        println!("No images found. Use 'bitosdt create-image' to create one.");
                    } else {
                        for image in images {
                            let status_icon = match image.status {
                                ImageStatus::Ready => "✓",
                                ImageStatus::Building => "⚙",
                                ImageStatus::Failed => "✗",
                                ImageStatus::Draft => "◯",
                            };
                            println!(
                                "{} {} - {} {} ({:?})",
                                status_icon,
                                image.id.to_string().split('-').next().unwrap_or("unknown"),
                                image.name,
                                image.os_info.os_type.display_name(),
                                image.status
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to list images: {}", e);
                }
            }
        }

        Commands::Hardware => {
            println!("BitOSDT 2.0 - Hardware Detection");
            println!("==================================");

            let detector = HardwareDetector::new();
            match detector.detect_all() {
                Ok(info) => {
                    println!("System Information:");
                    println!("  Manufacturer: {}", info.manufacturer);
                    println!("  Model: {}", info.model);
                    println!("  Product: {}", info.product);
                    println!("  Serial: {}", info.serial_number);
                    println!("  UUID: {}", info.uuid);
                    println!("  Form Factor: {:?}", info.form_factor);
                    println!("  Is VM: {}", info.is_vm);

                    println!("\nCPU:");
                    println!("  Name: {}", info.cpu.name);
                    println!("  Manufacturer: {}", info.cpu.manufacturer);
                    println!("  Cores: {}", info.cpu.cores);
                    println!("  Logical Processors: {}", info.cpu.logical_processors);

                    println!("\nMemory:");
                    println!("  Total: {:.2} GB", info.memory.total_gb);

                    println!("\nDisks:");
                    for disk in &info.disks {
                        println!(
                            "  [{}] {} - {:.2} GB ({})",
                            disk.index, disk.model, disk.size_gb, disk.media_type
                        );
                    }

                    println!("\nNetwork Adapters:");
                    for adapter in &info.network_adapters {
                        println!(
                            "  {} - {} ({})",
                            adapter.name, adapter.mac_address, adapter.adapter_type
                        );
                    }

                    println!("\nBIOS:");
                    println!("  Manufacturer: {}", info.bios.manufacturer);
                    println!("  Version: {}", info.bios.version);
                    println!("  Release Date: {}", info.bios.release_date);
                }
                Err(e) => {
                    eprintln!("✗ Failed to detect hardware: {}", e);
                }
            }
        }

        Commands::MatchDrivers { os } => {
            println!("BitOSDT 2.0 - Driver Matching");
            println!("==============================");

            let Some((db, config)) = get_db() else {
                eprintln!("✗ Failed to initialize database. Run 'bitosdt init' first.");
                return;
            };

            let detector = HardwareDetector::new();
            match detector.detect_all() {
                Ok(hardware) => {
                    println!("Detected Hardware:");
                    println!("  Manufacturer: {}", hardware.manufacturer);
                    println!("  Model: {}", hardware.model);
                    println!("  Product: {}", hardware.product);
                    println!("  OS Version Target: {}", os);

                    let driver_manager = match bitosdt::deploy::DriverManager::new(
                        db,
                        config.settings.download_path.join("drivers"),
                    )
                    .await
                    {
                        Ok(dm) => dm,
                        Err(e) => {
                            eprintln!("✗ Failed to initialize driver manager: {}", e);
                            return;
                        }
                    };

                    match driver_manager.find_matching_driverpack(&hardware, &os) {
                        Ok(Some(driverpack)) => {
                            println!("\n✓ Found matching DriverPack:");
                            println!("  Name: {}", driverpack.name);
                            println!("  Manufacturer: {}", driverpack.manufacturer);
                            println!("  Model: {}", driverpack.model);
                            println!("  OS: {} {}", driverpack.os, driverpack.os_version);
                            println!("  Filename: {}", driverpack.filename);
                            println!("  URL: {}", driverpack.url);
                            println!(
                                "  Size: {} MB",
                                driverpack.size_bytes.map(|s| s / 1_000_000).unwrap_or(0)
                            );
                        }
                        Ok(None) => {
                            println!("\n✗ No matching DriverPack found in catalog.");
                            println!("  Try running: bitosdt sync-catalogs");
                        }
                        Err(e) => {
                            eprintln!("✗ Error finding driverpack: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to detect hardware: {}", e);
                }
            }
        }

        Commands::BuildWinpe {
            output,
            arch,
            adk_path,
        } => {
            println!("BitOSDT 2.0 - Build WinPE");
            println!("==========================");

            let config = match Config::load() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("✗ Failed to load configuration: {}", e);
                    return;
                }
            };

            let output_path = config.settings.workspace_path.join(&output);
            let mut builder = WinPEBuilder::new(output_path.clone(), arch.clone());
            let adk_override = adk_path
                .as_ref()
                .map(PathBuf::from)
                .or_else(|| config.settings.adk_path.clone());

            match builder.initialize_with_override(adk_override.as_deref()) {
                Ok(_) => {
                    println!("✓ WinPE Builder initialized");
                    println!("  Architecture: {}", arch);
                    println!("  Output: {:?}", output_path);

                    match builder.create_winpe() {
                        Ok(winpe_dir) => {
                            println!("✓ WinPE created successfully");
                            println!("  Location: {:?}", winpe_dir);
                            println!("\nNext steps:");
                            println!("  1. Mount WIM: bitosdt mount-wim");
                            println!("  2. Add drivers: bitosdt add-drivers");
                            println!("  3. Build ISO: bitosdt build-iso");
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to create WinPE: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to initialize WinPE builder: {}", e);
                }
            }
        }

        Commands::BuildImage {
            interactive,
            source,
            source_path,
            os,
            release,
            edition,
            language,
            arch,
            download_url,
            output_type,
            output,
            volume_label,
            server_url,
            pxe_export_path,
            driver_path,
            local_admin_user,
            local_admin_password,
            local_admin_display_name,
            choco_package,
            install_chrome,
            winpe_assets,
            include_gui,
        } => {
            println!("BitOSDT 2.0 - Build Image");
            println!("=========================");

            let request = match resolve_cli_build_request(
                interactive,
                source,
                source_path,
                os,
                release,
                edition,
                language,
                arch,
                download_url,
                output_type,
                output,
                volume_label,
                server_url,
                pxe_export_path,
                driver_path,
                local_admin_user,
                local_admin_password,
                local_admin_display_name,
                choco_package,
                install_chrome,
                include_gui,
            ) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("✗ Invalid build request: {}", err);
                    return;
                }
            };

            let context = match resolve_cli_build_context(winpe_assets, include_gui) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("✗ Build context setup failed: {}", err);
                    return;
                }
            };

            match build_image_with_context(&request, &context, |progress| {
                println!(
                    "[{:>3}%] {:<10} {}",
                    progress.progress, progress.step, progress.message
                );
            })
            .await
            {
                Ok(path) => {
                    println!("\n✓ Image build completed successfully");
                    println!("  Output: {}", path);
                }
                Err(err) => {
                    eprintln!("\n✗ Image build failed: {}", err);
                }
            }
        }

        Commands::WinpeAssets { command } => {
            match command {
                WinpeAssetCommand::Sync { source, target } => {
                    println!("BitOSDT 2.0 - WinPE Assets Sync");
                    println!("================================");

                    let source_path = source.as_deref().map(PathBuf::from);
                    let target_path = target.as_deref().map(PathBuf::from);

                    match sync_winpe_asset_bundle(source_path.as_deref(), target_path.as_deref()) {
                        Ok(bundle) => {
                            println!("✓ WinPE asset bundle synced");
                            println!("  Root: {}", bundle.root.display());
                            println!("  Media: {}", bundle.media_dir.display());
                            if let Some(runtime) = bundle.runtime_executable {
                                println!("  Runtime: {}", runtime.display());
                            } else {
                                println!("  Runtime: not present (Linux full native runtime not bundled)");
                            }
                        }
                        Err(err) => {
                            eprintln!("✗ WinPE asset sync failed: {}", err);
                        }
                    }
                }
            }
        }

        Commands::LightweightHost { command } => match command {
            LightweightHostCommand::Serve {
                staging,
                bind,
                base_url,
            } => {
                let defaults = match resolve_default_lightweight_host_settings() {
                    Ok(value) => value,
                    Err(err) => {
                        eprintln!("✗ Failed to resolve lightweight host defaults: {}", err);
                        std::process::exit(1);
                    }
                };
                let staging_path = staging
                    .map(PathBuf::from)
                    .unwrap_or_else(|| defaults.0.clone());
                let bind_address = bind.unwrap_or(defaults.1);
                let runtime_base_url = base_url.unwrap_or(defaults.2);

                println!("BitOSDT 2.0 - Lightweight Host");
                println!("===============================");
                println!("  Staging: {}", staging_path.display());
                println!("  Bind: {}", bind_address);
                println!("  Base URL: {}", runtime_base_url);
                println!("  Stop with Ctrl+C");

                if let Err(err) =
                    serve_lightweight_tree(&staging_path, &bind_address, &runtime_base_url).await
                {
                    eprintln!("✗ Lightweight host failed: {}", err);
                    std::process::exit(1);
                }
            }
        },

        Commands::ListUsb => {
            println!("BitOSDT 2.0 - USB Devices");
            println!("==========================");

            match UsbWriter::list_usb_devices() {
                Ok(devices) => {
                    if devices.is_empty() {
                        println!("No USB devices found.");
                    } else {
                        println!("Available devices:");
                        for device in devices {
                            println!("  {}", device);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to list USB devices: {}", e);
                }
            }
        }

        Commands::Deploy { image, disk, uefi } => {
            println!("BitOSDT 2.0 - Deploy Image");
            println!("===========================");

            let Some((db, _config)) = get_db() else {
                eprintln!("✗ Failed to initialize database. Run 'bitosdt init' first.");
                return;
            };

            // Parse image ID (accept partial UUID)
            let image_id = match Uuid::parse_str(&image) {
                Ok(id) => id,
                Err(_) => {
                    // Try to find by partial UUID
                    match db.list_images() {
                        Ok(images) => {
                            if let Some(img) =
                                images.iter().find(|i| i.id.to_string().starts_with(&image))
                            {
                                img.id
                            } else {
                                eprintln!("✗ Image not found: {}", image);
                                return;
                            }
                        }
                        Err(e) => {
                            eprintln!("✗ Failed to list images: {}", e);
                            return;
                        }
                    }
                }
            };

            let image = match db.get_image(image_id) {
                Ok(Some(img)) => img,
                Ok(None) => {
                    eprintln!("✗ Image not found: {}", image_id);
                    return;
                }
                Err(e) => {
                    eprintln!("✗ Failed to get image: {}", e);
                    return;
                }
            };

            println!("Deploying image: {}", image.name);
            println!("  ID: {}", image.id);
            println!(
                "  OS: {} {}",
                image.os_info.os_type.display_name(),
                image.os_info.version
            );
            println!("  Target Disk: {}", disk);
            println!("  UEFI: {}", uefi);
            println!();

            // Update deploy config
            let mut deploy_config = image.config.clone();
            deploy_config.target_disk = Some(disk);
            deploy_config.uefi = uefi;

            let mut engine = DeploymentEngine::new(db);

            match engine
                .deploy(
                    &image,
                    &deploy_config,
                    Some(&|progress: DeploymentProgress| {
                        println!("[{:>3}%] {}", progress.percent_complete, progress.message);
                    }),
                )
                .await
            {
                Ok(_) => {
                    println!("\n✓ Deployment completed successfully!");
                    println!("  System will reboot to continue Windows setup.");
                }
                Err(e) => {
                    eprintln!("\n✗ Deployment failed: {}", e);
                }
            }
        }

        Commands::RuntimeDrivers {
            config,
            windows_path,
            prepare_only,
            server_url,
        } => {
            println!("BitOSDT 2.0 - Runtime Drivers");
            println!("=============================");

            let config_path = PathBuf::from(&config);
            let payload = match std::fs::read_to_string(&config_path) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("✗ Failed to read runtime driver config {}: {}", config, err);
                    std::process::exit(1);
                }
            };

            let runtime_config: RuntimeDriverConfig = match serde_json::from_str(&payload) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!(
                        "✗ Failed to parse runtime driver config {}: {}",
                        config, err
                    );
                    std::process::exit(1);
                }
            };

            let result = if prepare_only || windows_path.is_none() {
                let mut runtime_config = runtime_config.clone();
                if server_url.is_some() {
                    runtime_config
                        .runtime_driver_context
                        .cache_download_base_url = server_url
                        .as_deref()
                        .map(|base| format!("{}/BitOSDT/DriverCache", base.trim_end_matches('/')));
                }
                prepare_runtime_drivers(&runtime_config, None).await
            } else {
                let windows_path = PathBuf::from(windows_path.as_deref().unwrap_or("W:\\"));
                let mut runtime_config = runtime_config.clone();
                if server_url.is_some() {
                    runtime_config
                        .runtime_driver_context
                        .cache_download_base_url = server_url
                        .as_deref()
                        .map(|base| format!("{}/BitOSDT/DriverCache", base.trim_end_matches('/')));
                }
                prepare_runtime_drivers(&runtime_config, Some(&windows_path)).await
            };

            match result {
                Ok(manifest) => {
                    if let Some(driverpack) = manifest.matched_driverpack.as_ref() {
                        println!("✓ Runtime driver match: {}", driverpack.name);
                    } else {
                        println!("⚠ No runtime driver match found.");
                    }

                    if let Some(path) = manifest.extracted_path.as_ref() {
                        println!("  Extracted: {}", path.display());
                    }

                    println!("  Prepared: {}", manifest.prepared);
                    println!("  Installed count: {}", manifest.installed_count);

                    if !manifest.warnings.is_empty() {
                        println!("  Warnings:");
                        for warning in &manifest.warnings {
                            println!("    - {}", warning);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("✗ Runtime driver stage failed: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::WinpeDeploy {
            config,
            runtime_driver_config,
            log_path,
            status_path,
            skip_reboot,
        } => {
            let config_path = PathBuf::from(&config);
            let options = WinpeDeployOptions {
                log_path: PathBuf::from(log_path),
                status_path: PathBuf::from(status_path),
                runtime_driver_config_path: Some(PathBuf::from(runtime_driver_config)),
                skip_reboot,
            };

            match run_winpe_deploy(
                &config_path,
                &options,
                Some(&|status| {
                    println!(
                        "[stage {}/{} {:>3}%] {}",
                        status.stage_index,
                        status.stage_total,
                        status.percent_complete,
                        status.detail_text
                    );
                }),
            )
            .await
            {
                Ok(()) => {
                    if skip_reboot {
                        println!("Native WinPE deployment completed successfully.");
                    }
                }
                Err(err) => {
                    eprintln!("✗ Native WinPE deployment failed: {}", err);
                    std::process::exit(1);
                }
            }
        }

        Commands::Info => {
            println!("BitOSDT 2.0");
            println!("=============");
            println!("Version: {}", env!("CARGO_PKG_VERSION"));
            println!("Platform: {}", std::env::consts::OS);

            if let Ok(config) = Config::load() {
                println!("\nConfiguration:");
                println!("  Database: {:?}", config.database_path);
                println!("  Downloads: {:?}", config.settings.download_path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn normalize_arch_maps_common_x64_aliases() {
        assert_eq!(normalize_arch_label(Some("amd64".to_string())), "x64");
        assert_eq!(normalize_arch_label(Some("x86_64".to_string())), "x64");
        assert_eq!(normalize_arch_label(Some("x64".to_string())), "x64");
        assert_eq!(normalize_arch_label(Some("arm64".to_string())), "arm64");
    }

    #[test]
    fn local_cli_request_includes_local_admin_and_chocolatey_packages() {
        let temp = tempdir().expect("temp dir");
        let source_path = temp.path().join("windows.iso");
        std::fs::write(&source_path, b"fake-iso").expect("create local source");

        let request = resolve_cli_build_request(
            false,
            Some(BuildSourceArg::Local),
            Some(source_path.display().to_string()),
            Some("Windows 11".to_string()),
            None,
            Some("Pro".to_string()),
            Some("en-US".to_string()),
            Some("amd64".to_string()),
            None,
            Some(OutputTypeArg::Lightweight),
            Some(temp.path().join("bitosdt.iso").display().to_string()),
            None,
            None,
            None,
            Vec::new(),
            Some("Steff".to_string()),
            Some("Steffan1".to_string()),
            Some("Steff".to_string()),
            vec!["googlechrome".to_string(), "7zip".to_string()],
            false,
            false,
        )
        .expect("build request");

        assert_eq!(request.windows_version, "Windows 11");
        assert_eq!(request.windows_build, "latest");
        assert_eq!(request.language.as_deref(), Some("en-US"));
        assert_eq!(request.output_type, "LightweightISO");
        assert_eq!(request.user_accounts.len(), 1);
        assert_eq!(request.user_accounts[0]["username"].as_str(), Some("Steff"));
        assert_eq!(
            request.user_accounts[0]["password"].as_str(),
            Some("Steffan1")
        );
        assert_eq!(
            request.user_accounts[0]["group"].as_str(),
            Some("Administrators")
        );

        let packages = request.apps["chocolateyPackages"]
            .as_array()
            .expect("chocolatey packages array");
        assert_eq!(packages.len(), 2);
        assert!(packages.iter().any(|entry| {
            entry["packageName"]
                .as_str()
                .map(|name| name == "googlechrome")
                .unwrap_or(false)
        }));
        assert!(packages.iter().any(|entry| {
            entry["packageName"]
                .as_str()
                .map(|name| name == "7zip")
                .unwrap_or(false)
        }));
    }

    #[test]
    fn install_chrome_flag_deduplicates_existing_chocolatey_package() {
        let mut apps = default_apps_config();
        add_chocolatey_packages(
            &mut apps,
            vec!["googlechrome".to_string(), "GoogleChrome".to_string()],
        )
        .expect("add packages");

        let packages = apps["chocolateyPackages"]
            .as_array()
            .expect("chocolatey packages array");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0]["packageName"].as_str(), Some("googlechrome"));
    }
}
