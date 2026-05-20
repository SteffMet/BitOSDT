#[cfg(not(target_os = "windows"))]
use crate::build::linux_support::extract_iso_image;
use crate::build::{
    runtime_drivers::{stage_runtime_driver_assets, RuntimeDriverAssetConfig},
    winpe_ui::{
        resolve_winpe_compat_spoof_enabled, write_hta_mode_config, write_hta_shell,
        write_initial_status, write_kiosk_helper, write_shell_launcher_cmd,
        write_winpe_compat_spoof_assets, write_winpeshl_ini, WinPEUiMode,
    },
    FileInjection, ImagePrepConfig, ImagePreparer, IsoCreator, RuntimeDomainJoinConfig,
    WinPEBuilder,
};
use crate::config::{resolve_unattend_locale_settings, AutopilotProfile, UnattendConfig};
use crate::core::adk::AdkPaths;
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::{
    resolve_adk_paths, Config, DriverPack, RuntimeDriverConfig, RuntimeDriverContext,
    RuntimeDriverPolicy,
};
use crate::download::{EsdDownloader, HashValidator};
use crate::tasks::{
    AppInstallConfig, InstallerSourceType, InstallerType, LocalPayloadItem, LocalPayloadKind,
    TaskSequence, TaskType,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;
use std::time::Instant;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiskSelectionPolicy {
    #[default]
    ConfigFirstSafeFallback,
    AlwaysDisk0,
    RequireExplicitDisk,
}

#[derive(Debug, Clone)]
pub struct FullIsoBuildConfig {
    pub source_path: PathBuf,
    pub output_path: PathBuf,
    pub volume_label: String,
    pub windows_version: String,
    pub windows_build: String,
    pub windows_edition: String,
    pub language: String,
    pub architecture: String,
    pub wim_index: u32,
    pub target_disk: Option<u32>,
    pub disk_selection_policy: DiskSelectionPolicy,
    pub unattend: UnattendConfig,
    pub autopilot: Option<AutopilotProfile>,
    pub task_sequence: Option<TaskSequence>,
    pub runtime_domain_join: Option<RuntimeDomainJoinConfig>,
    pub workspace: Option<PathBuf>,
    pub download_dir: Option<PathBuf>,
    pub adk_paths: Option<AdkPaths>,
    pub winpe_assets_dir: Option<PathBuf>,
    pub winpe_packages_dir: Option<PathBuf>,
    pub ui_dir: Option<PathBuf>,
    pub native_executable: Option<PathBuf>,
    pub common_boot_driver_dir: Option<PathBuf>,
    pub runtime_driver_catalog: Vec<DriverPack>,
    pub runtime_driver_cache_source: Option<PathBuf>,
    pub driver_paths: Vec<PathBuf>,
    pub apply_drivers_to_offline_windows: bool,
    pub runtime_driver_policy: RuntimeDriverPolicy,
    pub unc_image_path: Option<String>,
    pub unc_auth_username: Option<String>,
    pub unc_auth_password: Option<String>,
    pub http_image_url: Option<String>,
    pub prompt_unc_credentials_at_runtime: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct FullIsoBuildResult {
    pub output_path: PathBuf,
    pub prepared_wim_path: PathBuf,
    pub payload_provenance: PayloadProvenance,
    pub source_path: PathBuf,
    pub workspace: PathBuf,
    pub winpe_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayloadProvenance {
    pub size_bytes: u64,
    pub sha256: String,
    pub file_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FullIsoProgress {
    pub step: String,
    pub progress: u32,
    pub message: String,
}

const CONVERT_PROGRESS_START: u32 = 20;
const CONVERT_PROGRESS_END: u32 = 44;
const PREP_PROGRESS_START: u32 = 45;
const PREP_PROGRESS_END: u32 = 74;

fn scale_progress_range(start: u32, end: u32, progress: u32) -> u32 {
    let bounded = progress.min(100);
    start + ((end.saturating_sub(start)) * bounded / 100)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FullIsoDeployConfig {
    mode: String,
    os_version: String,
    wim_index: u32,
    target_disk: Option<u32>,
    disk_selection_policy: DiskSelectionPolicy,
    runtime_driver_policy: RuntimeDriverPolicy,
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

fn collect_payload_provenance(payload_path: &Path) -> BitOSDTResult<PayloadProvenance> {
    let metadata = fs::metadata(payload_path)?;
    let file_name = payload_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string());

    Ok(PayloadProvenance {
        size_bytes: metadata.len(),
        sha256: HashValidator::calculate_sha256(payload_path)?,
        file_name,
    })
}

impl FullIsoBuildConfig {
    fn validate(&self) -> BitOSDTResult<()> {
        if !self.source_path.exists() {
            return Err(BitOSDTError::NotFound(format!(
                "Source file not found: {}",
                self.source_path.display()
            )));
        }

        let extension = self
            .source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if !matches!(extension.as_str(), "iso" | "esd" | "wim") {
            return Err(BitOSDTError::InvalidInput(format!(
                "Unsupported source format '{}'. Expected .iso, .esd, or .wim",
                extension
            )));
        }

        if matches!(
            self.disk_selection_policy,
            DiskSelectionPolicy::RequireExplicitDisk
        ) && self.target_disk.is_none()
        {
            return Err(BitOSDTError::InvalidInput(
                "Disk selection policy requires an explicit target disk".to_string(),
            ));
        }

        for driver_path in &self.driver_paths {
            if !driver_path.exists() {
                return Err(BitOSDTError::NotFound(format!(
                    "Driver path not found: {}",
                    driver_path.display()
                )));
            }
        }

        Ok(())
    }
}

pub fn build_full_iso<F>(
    config: &FullIsoBuildConfig,
    mut progress_callback: F,
) -> BitOSDTResult<FullIsoBuildResult>
where
    F: FnMut(FullIsoProgress),
{
    config.validate()?;
    let build_started_at = Instant::now();

    let mut emit = |step: &str, progress: u32, message: &str, cb: &mut F| {
        cb(FullIsoProgress {
            step: step.to_string(),
            progress,
            message: message.to_string(),
        });
    };

    emit(
        "init",
        0,
        "Starting full ISO build...",
        &mut progress_callback,
    );

    let download_dir = config
        .download_dir
        .clone()
        .or_else(|| Config::configured_download_path().ok())
        .unwrap_or_else(|| PathBuf::from(r"C:\BitOSDT\Downloads"));
    let workspace = config
        .workspace
        .clone()
        .or_else(|| Config::configured_workspace_path().ok())
        .unwrap_or_else(|| PathBuf::from(r"C:\BitOSDT\Workspace"));
    let resolved_adk = config
        .adk_paths
        .clone()
        .or_else(|| resolve_adk_paths(None, &config.architecture));

    fs::create_dir_all(&download_dir)?;
    fs::create_dir_all(&workspace)?;

    let mut unattend = config.unattend.clone();
    let (language, input_locale) = resolve_unattend_locale_settings(&config.language)?;
    unattend.language = language;
    unattend.input_locale = input_locale;

    emit(
        "source",
        5,
        "Resolving Windows source...",
        &mut progress_callback,
    );
    let source_started_at = Instant::now();
    let source_path = resolve_source_path(
        &config.source_path,
        &workspace,
        &mut emit,
        &mut progress_callback,
    )?;
    info!(
        "Full ISO source resolution completed in {:.2}s",
        source_started_at.elapsed().as_secs_f64()
    );

    emit("source", 15, "Source resolved.", &mut progress_callback);

    let wim_path = workspace.join("install.wim");
    let source_extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let final_wim_path = if source_extension == "esd" {
        emit(
            "convert",
            CONVERT_PROGRESS_START,
            "Converting ESD to WIM...",
            &mut progress_callback,
        );
        let convert_started_at = Instant::now();
        let downloader = EsdDownloader::new_with_adk(workspace.clone(), resolved_adk.clone())?;
        let result =
            downloader.convert_esd_to_wim(&source_path, &wim_path, None, |progress, message| {
                let scaled = scale_progress_range(
                    CONVERT_PROGRESS_START,
                    CONVERT_PROGRESS_END,
                    progress as u32,
                );
                progress_callback(FullIsoProgress {
                    step: "convert".to_string(),
                    progress: scaled,
                    message,
                });
            })?;
        info!(
            "ESD to WIM conversion completed in {:.2}s",
            convert_started_at.elapsed().as_secs_f64()
        );
        result.wim_path
    } else if source_extension == "wim" {
        emit(
            "convert",
            20,
            "Preparing WIM source...",
            &mut progress_callback,
        );
        if source_path != wim_path {
            fs::copy(&source_path, &wim_path)?;
        }
        wim_path
    } else {
        return Err(BitOSDTError::InvalidInput(format!(
            "Unexpected source extension after resolution: {}",
            source_extension
        )));
    };

    emit(
        "prepare",
        PREP_PROGRESS_START,
        "Preparing Windows image...",
        &mut progress_callback,
    );
    let (processed_task_sequence, inject_files) =
        preprocess_task_sequence_for_full_iso(config.task_sequence.as_ref())?;
    let prep_config = ImagePrepConfig {
        source_wim: final_wim_path.clone(),
        wim_index: config.wim_index.max(1),
        unattend: Some(unattend),
        autopilot: config.autopilot.clone(),
        task_sequence: processed_task_sequence,
        inject_files,
        driver_paths: if config.apply_drivers_to_offline_windows {
            config.driver_paths.clone()
        } else {
            Vec::new()
        },
        remove_apps: vec![],
        enable_features: vec![],
        disable_features: vec![],
    };
    let prepare_started_at = Instant::now();
    let preparer = ImagePreparer::new_with_adk(workspace.clone(), resolved_adk.clone())?;
    let prepared_wim =
        preparer.prepare_image_with_progress(&prep_config, &final_wim_path, |progress| {
            progress_callback(FullIsoProgress {
                step: progress.step,
                progress: scale_progress_range(
                    PREP_PROGRESS_START,
                    PREP_PROGRESS_END,
                    progress.progress,
                ),
                message: progress.message,
            });
        })?;
    let payload_provenance = collect_payload_provenance(&prepared_wim)?;
    info!(
        "Offline Windows image preparation completed in {:.2}s",
        prepare_started_at.elapsed().as_secs_f64()
    );

    emit(
        "prepare-complete",
        75,
        "Image preparation complete.",
        &mut progress_callback,
    );
    emit(
        "winpe",
        80,
        "Building WinPE environment...",
        &mut progress_callback,
    );

    let winpe_started_at = Instant::now();
    let mut winpe_builder = WinPEBuilder::new(workspace.clone(), config.architecture.clone());
    let adk_override = resolved_adk.as_ref().map(|adk| adk.root.as_path());
    winpe_builder.initialize_with_assets(adk_override, config.winpe_assets_dir.as_deref())?;
    let winpe_dir = winpe_builder.create_winpe()?;
    let media_runtime_cache_dir = winpe_dir.join("media").join("BitOSDT").join("DriverCache");
    let driver_cache_source_dir = config
        .runtime_driver_cache_source
        .clone()
        .unwrap_or_else(|| download_dir.join("drivers"));
    if driver_cache_source_dir.exists() {
        copy_directory_recursive(&driver_cache_source_dir, &media_runtime_cache_dir)?;
    }

    let winpe_sources = winpe_dir.join("media").join("sources");
    fs::create_dir_all(&winpe_sources)?;
    let payload_copy_started_at = Instant::now();
    fs::copy(&prepared_wim, winpe_sources.join("install.wim"))?;
    info!(
        "Prepared install.wim copied into WinPE media in {:.2}s",
        payload_copy_started_at.elapsed().as_secs_f64()
    );

    emit(
        "winpe",
        86,
        "Customizing WinPE startup...",
        &mut progress_callback,
    );
    let boot_wim = winpe_sources.join("boot.wim");
    if !boot_wim.exists() {
        return Err(BitOSDTError::NotFound(format!(
            "WinPE boot.wim not found: {}",
            boot_wim.display()
        )));
    }
    let winpe_mount_dir = workspace.join("winpe-mount");
    if winpe_mount_dir.exists() {
        let _ = fs::remove_dir_all(&winpe_mount_dir);
    }
    winpe_builder.mount_wim(&boot_wim, &winpe_mount_dir)?;
    let customize_started_at = Instant::now();
    let customize_result = customize_full_iso_winpe(
        &winpe_builder,
        &winpe_mount_dir,
        config,
        &payload_provenance,
    );
    let unmount_result = winpe_builder.unmount_wim(&winpe_mount_dir, customize_result.is_ok());
    customize_result?;
    unmount_result?;
    info!(
        "WinPE customization completed in {:.2}s",
        customize_started_at.elapsed().as_secs_f64()
    );
    info!(
        "Overall WinPE environment build completed in {:.2}s",
        winpe_started_at.elapsed().as_secs_f64()
    );

    emit(
        "iso",
        90,
        "Creating bootable ISO...",
        &mut progress_callback,
    );
    if let Some(parent) = config.output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let media_dir = winpe_dir.join("media");
    let iso_started_at = Instant::now();
    IsoCreator::create_iso_with_adk(
        &media_dir,
        &config.output_path,
        &config.volume_label,
        resolved_adk.as_ref(),
    )?;
    info!(
        "ISO creation completed in {:.2}s",
        iso_started_at.elapsed().as_secs_f64()
    );

    if !config.output_path.exists() {
        return Err(BitOSDTError::WinPE(
            "ISO creation reported success but output file was not found".to_string(),
        ));
    }

    emit(
        "complete",
        100,
        "Build completed successfully.",
        &mut progress_callback,
    );

    info!(
        "Full ISO build completed in {:.2}s",
        build_started_at.elapsed().as_secs_f64()
    );

    Ok(FullIsoBuildResult {
        output_path: config.output_path.clone(),
        prepared_wim_path: prepared_wim,
        payload_provenance,
        source_path,
        workspace,
        winpe_dir,
    })
}

pub(crate) fn preprocess_task_sequence_for_full_iso(
    task_sequence: Option<&TaskSequence>,
) -> BitOSDTResult<(Option<TaskSequence>, Vec<FileInjection>)> {
    let Some(mut sequence) = task_sequence.cloned() else {
        return Ok((None, Vec::new()));
    };

    let mut inject_files = Vec::new();
    let mut embedded_index = 1usize;

    for task in &mut sequence.tasks {
        if let TaskType::InstallApps(app_config) = &mut task.task_type {
            preprocess_app_install_config_for_full_iso(
                app_config,
                &mut inject_files,
                &mut embedded_index,
            )?;
        }
    }

    Ok((Some(sequence), inject_files))
}

fn preprocess_app_install_config_for_full_iso(
    app_config: &mut AppInstallConfig,
    inject_files: &mut Vec<FileInjection>,
    embedded_index: &mut usize,
) -> BitOSDTResult<()> {
    if !app_config.copied_items.is_empty() {
        for payload in &app_config.copied_items {
            add_local_payload_injections_for_full_iso(
                payload,
                app_config.copy_destination.as_deref(),
                inject_files,
            )?;
        }
        app_config.copied_items.clear();
        app_config.copy_destination = None;
    }

    for installer in &mut app_config.custom_installers {
        if !installer.enabled {
            continue;
        }

        if !installer.dependencies.is_empty() {
            for dependency in &installer.dependencies {
                add_local_payload_injections_for_full_iso(
                    dependency,
                    installer.dependency_destination.as_deref(),
                    inject_files,
                )?;
            }
            installer.dependencies.clear();
            installer.dependency_destination = None;
        }

        if installer.source_type != InstallerSourceType::EmbeddedFile {
            continue;
        }

        let source_path = PathBuf::from(&installer.path);
        if !source_path.exists() {
            return Err(BitOSDTError::InvalidInput(format!(
                "Embedded installer not found: {}",
                installer.path
            )));
        }
        if !source_path.is_file() {
            return Err(BitOSDTError::InvalidInput(format!(
                "Embedded installer path is not a file: {}",
                installer.path
            )));
        }

        let expected_ext = expected_installer_extension(&installer.installer_type);
        let actual_ext = source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if actual_ext != expected_ext {
            return Err(BitOSDTError::InvalidInput(format!(
                "Embedded installer '{}' has extension '.{}' but installer type {:?} requires '.{}'",
                installer.path,
                actual_ext,
                installer.installer_type,
                expected_ext
            )));
        }

        let slug = sanitize_installer_slug(&installer.name);
        let staged_file = format!("{}-{}.{}", slug, *embedded_index, expected_ext);
        let image_relative_path = format!(r"BitOSDT\Installers\{}", staged_file);
        let runtime_path = format!(r"C:\BitOSDT\Installers\{}", staged_file);

        inject_files.push(FileInjection {
            source: source_path,
            destination: image_relative_path,
        });

        installer.path = runtime_path;
        installer.source_type = InstallerSourceType::DirectPathOrUrl;
        installer.source_file_name = None;
        *embedded_index += 1;
    }

    Ok(())
}

fn add_local_payload_injections_for_full_iso(
    payload: &LocalPayloadItem,
    destination: Option<&str>,
    inject_files: &mut Vec<FileInjection>,
) -> BitOSDTResult<()> {
    let source_path = PathBuf::from(payload.source_path.trim());
    let destination_root = normalize_payload_destination_root(destination);

    match payload.source_kind {
        LocalPayloadKind::File => {
            if !source_path.is_file() {
                return Err(BitOSDTError::InvalidInput(format!(
                    "Payload file not found: {}",
                    payload.source_path
                )));
            }

            let file_name = source_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .ok_or_else(|| {
                    BitOSDTError::InvalidInput(format!(
                        "Invalid payload file path: {}",
                        payload.source_path
                    ))
                })?;

            inject_files.push(FileInjection {
                source: source_path,
                destination: join_image_relative_path(&destination_root, &file_name),
            });
        }
        LocalPayloadKind::Directory => {
            if !source_path.is_dir() {
                return Err(BitOSDTError::InvalidInput(format!(
                    "Payload directory not found: {}",
                    payload.source_path
                )));
            }

            let root_name = source_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .ok_or_else(|| {
                    BitOSDTError::InvalidInput(format!(
                        "Invalid payload directory path: {}",
                        payload.source_path
                    ))
                })?;

            collect_directory_payload_injections(
                &source_path,
                &source_path,
                &join_image_relative_path(&destination_root, &root_name),
                inject_files,
            )?;
        }
    }

    Ok(())
}

fn collect_directory_payload_injections(
    source_root: &Path,
    current_dir: &Path,
    image_destination_root: &str,
    inject_files: &mut Vec<FileInjection>,
) -> BitOSDTResult<()> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_directory_payload_injections(
                source_root,
                &path,
                image_destination_root,
                inject_files,
            )?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let relative = path.strip_prefix(source_root).map_err(|err| {
            BitOSDTError::InvalidInput(format!(
                "Failed to stage payload directory {}: {}",
                source_root.display(),
                err
            ))
        })?;
        let relative = relative.to_string_lossy().replace('/', "\\");
        let destination = if relative.is_empty() {
            image_destination_root.to_string()
        } else {
            format!(r"{}\{}", image_destination_root, relative)
        };

        inject_files.push(FileInjection {
            source: path,
            destination,
        });
    }

    Ok(())
}

fn normalize_payload_destination_root(destination: Option<&str>) -> String {
    let normalized = destination
        .unwrap_or("")
        .trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string();

    let runtime_root = if normalized.is_empty() {
        r"C:\BitOSDT\Files".to_string()
    } else {
        normalized
    };

    runtime_root
        .split_once(":\\")
        .map(|(_, remainder)| remainder.to_string())
        .unwrap_or_else(|| runtime_root.trim_start_matches('\\').to_string())
}

fn join_image_relative_path(base: &str, leaf: &str) -> String {
    let trimmed_base = base.trim().trim_matches('\\');
    let trimmed_leaf = leaf.trim().trim_matches('\\').replace('/', "\\");
    if trimmed_base.is_empty() {
        trimmed_leaf
    } else if trimmed_leaf.is_empty() {
        trimmed_base.to_string()
    } else {
        format!(r"{}\{}", trimmed_base, trimmed_leaf)
    }
}

fn expected_installer_extension(installer_type: &InstallerType) -> &'static str {
    match installer_type {
        InstallerType::Exe => "exe",
        InstallerType::Msi => "msi",
        InstallerType::Msix => "msix",
        InstallerType::Msp => "msp",
    }
}

fn sanitize_installer_slug(name: &str) -> String {
    let normalized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    let compact = normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if compact.is_empty() {
        "installer".to_string()
    } else {
        compact
    }
}

fn copy_directory_recursive(source: &Path, destination: &Path) -> BitOSDTResult<()> {
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

fn customize_full_iso_winpe(
    winpe_builder: &WinPEBuilder,
    mount_dir: &Path,
    config: &FullIsoBuildConfig,
    payload_provenance: &PayloadProvenance,
) -> BitOSDTResult<()> {
    winpe_builder.enable_powershell_for_language(mount_dir, &config.language)?;
    winpe_builder.enable_extended_components_for_language(mount_dir, &config.language)?;
    let hta_enabled = winpe_builder.enable_hta_for_language(mount_dir, &config.language)?;
    if !hta_enabled && winpe_builder.adk_paths().is_some() {
        return Err(BitOSDTError::WinPE(format!(
            "WinPE-HTA optional component is missing for language '{}'. Install the Windows ADK WinPE-HTA package and rebuild.",
            config.language
        )));
    }

    if config.runtime_driver_policy.bundle_common_boot_drivers {
        if let Some(common_boot_driver_dir) = config.common_boot_driver_dir.as_ref() {
            if common_boot_driver_dir.exists() {
                winpe_builder.add_drivers(mount_dir, common_boot_driver_dir)?;
            }
        }
    }

    for driver_path in &config.driver_paths {
        winpe_builder.add_drivers(mount_dir, driver_path)?;
    }

    let bitosdt_dir = mount_dir.join("BitOSDT");
    let config_dir = bitosdt_dir.join("Config");
    let scripts_dir = bitosdt_dir.join("Scripts");
    let logs_dir = bitosdt_dir.join("Logs");
    let state_dir = bitosdt_dir.join("State");
    let ui_dir = bitosdt_dir.join("UI");
    fs::create_dir_all(&config_dir)?;
    fs::create_dir_all(&scripts_dir)?;
    fs::create_dir_all(&logs_dir)?;
    fs::create_dir_all(&state_dir)?;
    fs::create_dir_all(&ui_dir)?;
    write_winpe_compat_spoof_assets(mount_dir, resolve_winpe_compat_spoof_enabled())?;

    if let Some(native_executable) = config.native_executable.as_ref() {
        if native_executable.exists() {
            fs::copy(native_executable, bitosdt_dir.join("bitosdt.exe"))?;
        }
    }

    if let Some(packages_dir) = config.winpe_packages_dir.as_ref() {
        let dest_packages = mount_dir.join("BitOSDT").join("Packages");
        std::fs::create_dir_all(&dest_packages)?;

        let has_sciter = packages_dir.join("sciter").is_dir();
        if let Ok(entries) = std::fs::read_dir(packages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_lowercase();

                if path.is_dir() {
                    if has_sciter && (name_str == "chrome" || name_str == "supermium") {
                        continue;
                    }
                    let dest_subdir = format!(r"BitOSDT\Packages\{}", name.to_string_lossy());
                    winpe_builder.add_files(mount_dir, &path, &dest_subdir)?;
                } else if path.is_file() {
                    let _ = std::fs::copy(&path, dest_packages.join(&name));
                }
            }
        }

        winpe_builder.inject_vc_runtime_dlls_from_dir(mount_dir, packages_dir)?;

        let custom_fonts_dir = packages_dir.join("fonts");
        if custom_fonts_dir.exists() {
            let _ = winpe_builder.inject_custom_fonts_from_dir(mount_dir, &custom_fonts_dir)?;
        }
    }
    if let Some(ui_dir) = config.ui_dir.as_ref() {
        winpe_builder.add_files(mount_dir, ui_dir, r"BitOSDT\UI")?;
    }

    let mut runtime_driver_context = RuntimeDriverContext::winpe_default();
    if config.common_boot_driver_dir.is_some() {
        runtime_driver_context.common_boot_driver_directory =
            Some(PathBuf::from(r"X:\BitOSDT\DriverCache\common-boot"));
    }
    stage_runtime_driver_assets(
        mount_dir,
        winpe_builder,
        &RuntimeDriverAssetConfig {
            policy: config.runtime_driver_policy.clone(),
            context: runtime_driver_context.clone(),
            catalog: config.runtime_driver_catalog.clone(),
            cache_source: config.runtime_driver_cache_source.clone(),
        },
    )?;

    let runtime_driver_config = RuntimeDriverConfig {
        os_version: config.windows_build.clone(),
        runtime_driver_policy: config.runtime_driver_policy.clone(),
        runtime_driver_context: runtime_driver_context.clone(),
    };
    fs::write(
        config_dir.join("runtime-drivers.json"),
        serde_json::to_string_pretty(&runtime_driver_config)?,
    )?;

    let deploy_config = FullIsoDeployConfig {
        mode: "full_iso".to_string(),
        os_version: config.windows_build.clone(),
        wim_index: config.wim_index.max(1),
        target_disk: config.target_disk,
        disk_selection_policy: config.disk_selection_policy.clone(),
        runtime_driver_policy: config.runtime_driver_policy.clone(),
        runtime_driver_context,
        unc_image_path: config.unc_image_path.clone(),
        unc_auth_username: config.unc_auth_username.clone(),
        unc_auth_password: config.unc_auth_password.clone(),
        http_image_url: config.http_image_url.clone(),
        expected_payload_size_bytes: Some(payload_provenance.size_bytes),
        expected_payload_sha256: Some(payload_provenance.sha256.clone()),
        expected_payload_file_name: payload_provenance.file_name.clone(),
        unattend: config.unattend.clone(),
        autopilot: config.autopilot.clone(),
        task_sequence: config.task_sequence.clone(),
        runtime_domain_join: config.runtime_domain_join.clone(),
        prompt_unc_credentials_at_runtime: config.prompt_unc_credentials_at_runtime,
    };
    let deploy_config_json = serde_json::to_string_pretty(&deploy_config)?;
    fs::write(config_dir.join("deploy.json"), deploy_config_json)?;

    fs::write(
        scripts_dir.join("Deploy-FullIso.ps1"),
        generate_full_iso_deploy_script(),
    )?;
    fs::write(
        scripts_dir.join("Launch-BitOSDT-WinPE.ps1"),
        generate_full_iso_shell_launcher_script(),
    )?;
    write_initial_status(mount_dir, WinPEUiMode::FullIso)?;

    if config.prompt_unc_credentials_at_runtime.unwrap_or(false) {
        use crate::build::provisioning_ui::generate_credential_prompt_hta;
        let hta_content = generate_credential_prompt_hta("UNC");
        fs::write(scripts_dir.join("CredentialPrompt.hta"), hta_content)?;
    }

    let prefer_native_runtime = !cfg!(target_os = "windows")
        || config.prompt_unc_credentials_at_runtime.unwrap_or(false)
        || config
            .runtime_domain_join
            .as_ref()
            .is_some_and(|value| value.prompt_for_credentials_at_runtime);
    if prefer_native_runtime && !bitosdt_dir.join("bitosdt.exe").exists() {
        return Err(BitOSDTError::Validation(
            "Linux-built Full ISO media requires a WinPE-native bitosdt.exe in the asset bundle."
                .to_string(),
        ));
    }

    if !prefer_native_runtime {
        let fallback_cmd = concat!(
            "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"X:\\BitOSDT\\Scripts\\Deploy-FullIso.ps1\" -ConfigPath \"X:\\BitOSDT\\Config\\deploy.json\"\r\n",
            "if %ERRORLEVEL% NEQ 0 cmd /k"
        );
        write_hta_mode_config(mount_dir)?;
        write_shell_launcher_cmd(mount_dir, fallback_cmd)?;
        write_hta_shell(mount_dir)?;
        write_kiosk_helper(mount_dir)?;
    }
    write_winpeshl_ini(mount_dir)?;

    let startnet_path = mount_dir
        .join("Windows")
        .join("System32")
        .join("startnet.cmd");
    fs::write(
        startnet_path,
        generate_full_iso_startnet(prefer_native_runtime),
    )?;
    Ok(())
}

fn generate_full_iso_startnet(prefer_native_runtime: bool) -> String {
    if prefer_native_runtime {
        return r#"@echo off
setlocal EnableDelayedExpansion
echo Starting BitOSDT deployment...

wpeinit

set DEPLOY_EXE=X:\BitOSDT\bitosdt.exe
set DEPLOY_CONFIG=X:\BitOSDT\Config\deploy.json
set RUNTIME_DRIVER_CONFIG=X:\BitOSDT\Config\runtime-drivers.json
set STARTNET_LOG=X:\BitOSDT\Logs\startnet.log

if not exist "X:\BitOSDT\Logs" (
    mkdir "X:\BitOSDT\Logs" >nul 2>&1
)
echo [%DATE% %TIME%] startnet.cmd initialized for native runtime>>"%STARTNET_LOG%"

if not exist "%DEPLOY_EXE%" (
    echo.
    echo Native BitOSDT runtime missing: "%DEPLOY_EXE%"
    echo [%DATE% %TIME%] Native runtime missing at "%DEPLOY_EXE%".>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

if not exist "%DEPLOY_CONFIG%" (
    echo.
    echo BitOSDT deployment config missing: "%DEPLOY_CONFIG%"
    echo [%DATE% %TIME%] Deployment config missing at "%DEPLOY_CONFIG%".>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

echo [%DATE% %TIME%] Invoking native WinPE deployment runtime.>>"%STARTNET_LOG%"
"%DEPLOY_EXE%" winpe-deploy --config "%DEPLOY_CONFIG%" --runtime-driver-config "%RUNTIME_DRIVER_CONFIG%"
set DEPLOY_EXIT=!ERRORLEVEL!
if !DEPLOY_EXIT! NEQ 0 (
    echo.
    echo BitOSDT deployment failed. Review logs at X:\BitOSDT\Logs\deploy.log
    echo [%DATE% %TIME%] Native deployment runtime failed with exit code !DEPLOY_EXIT!.>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

echo [%DATE% %TIME%] startnet.cmd completed successfully.>>"%STARTNET_LOG%"
goto :eof
"#
        .replace('\n', "\r\n");
    }

    r#"@echo off
setlocal EnableDelayedExpansion
echo Starting BitOSDT deployment...

:: Initialize WinPE hardware and networking first.
wpeinit

set COMPAT_FLAG=X:\BitOSDT\Config\enable-winpe-compat-spoof.flag
set COMPAT_SCRIPT=X:\BitOSDT\Scripts\Set-WinPE-CompatibilitySpoof.ps1
if exist "%COMPAT_FLAG%" (
    if exist "%COMPAT_SCRIPT%" (
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%COMPAT_SCRIPT%" -Mode Apply >nul 2>&1
    )
)

:: Launch BitOSDT deployment orchestrator.
set WRAPPER=X:\BitOSDT\Scripts\Launch-BitOSDT-WinPE.cmd
set DEPLOY_CONFIG=X:\BitOSDT\Config\deploy.json
set DEPLOY_SCRIPT=X:\BitOSDT\Scripts\Deploy-FullIso.ps1
set STARTNET_LOG=X:\BitOSDT\Logs\startnet.log

if not exist "X:\BitOSDT\Logs" (
    mkdir "X:\BitOSDT\Logs" >nul 2>&1
)
echo [%DATE% %TIME%] startnet.cmd initialized>>"%STARTNET_LOG%"

if exist "%WRAPPER%" (
    echo [%DATE% %TIME%] Invoking shell wrapper "%WRAPPER%">>"%STARTNET_LOG%"
    call "%WRAPPER%"
    set WRAPPER_EXIT=!ERRORLEVEL!
    echo [%DATE% %TIME%] Shell wrapper exit code !WRAPPER_EXIT!>>"%STARTNET_LOG%"
    if !WRAPPER_EXIT! EQU 0 goto :eof
    echo Shell wrapper returned error !WRAPPER_EXIT!. Executing direct fallback...
    echo [%DATE% %TIME%] Wrapper failed. Executing direct fallback path.>>"%STARTNET_LOG%"
) else (
    echo Shell wrapper missing at "%WRAPPER%". Executing direct fallback...
    echo [%DATE% %TIME%] Shell wrapper missing at "%WRAPPER%".>>"%STARTNET_LOG%"
)

if not exist "%DEPLOY_SCRIPT%" (
    echo.
    echo BitOSDT deployment fallback script missing: "%DEPLOY_SCRIPT%"
    echo Rebuild WinPE media to include X:\BitOSDT\Scripts\Deploy-FullIso.ps1
    echo [%DATE% %TIME%] Deployment script missing at "%DEPLOY_SCRIPT%".>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

if not exist "%DEPLOY_CONFIG%" (
    echo.
    echo BitOSDT deployment config missing: "%DEPLOY_CONFIG%"
    echo Rebuild WinPE media to include X:\BitOSDT\Config\deploy.json
    echo [%DATE% %TIME%] Deployment config missing at "%DEPLOY_CONFIG%".>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

echo [%DATE% %TIME%] Invoking fallback deploy script "%DEPLOY_SCRIPT%".>>"%STARTNET_LOG%"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%DEPLOY_SCRIPT%" -ConfigPath "%DEPLOY_CONFIG%"
set DEPLOY_EXIT=!ERRORLEVEL!
if !DEPLOY_EXIT! NEQ 0 (
    echo.
    echo BitOSDT deployment failed. Review logs at X:\BitOSDT\Logs\deploy.log
    echo [%DATE% %TIME%] Fallback deploy script failed with exit code !DEPLOY_EXIT!.>>"%STARTNET_LOG%"
    cmd /k
    goto :eof
)

echo [%DATE% %TIME%] startnet.cmd completed successfully.>>"%STARTNET_LOG%"
goto :eof
"#
    .replace('\n', "\r\n")
}

fn generate_full_iso_shell_launcher_script() -> String {
    r#"param()

$ErrorActionPreference = 'Continue'
$DeployConfig = "X:\BitOSDT\Config\deploy.json"
$DeployScript = "X:\BitOSDT\Scripts\Deploy-FullIso.ps1"
$NativeExe = "X:\BitOSDT\bitosdt.exe"
$LogPath = "X:\BitOSDT\Logs\deploy.log"
$StatusPath = "X:\BitOSDT\State\deploy-status.json"

if (-not (Test-Path (Split-Path -Parent $LogPath))) {
    New-Item -Path (Split-Path -Parent $LogPath) -ItemType Directory -Force | Out-Null
}
if (-not (Test-Path (Split-Path -Parent $StatusPath))) {
    New-Item -Path (Split-Path -Parent $StatusPath) -ItemType Directory -Force | Out-Null
}

function Write-LauncherLog {
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::AppendAllText($LogPath, "$timestamp [$Level] $Message`r`n", $utf8NoBom)
    Write-Host "[$Level] $Message"
}

function Write-LauncherStatus {
    param(
        [int]$PercentComplete,
        [string]$StatusText,
        [string]$DetailText,
        [bool]$IsError = $false,
        [string]$ErrorMessage = $null
    )

    try {
        $payload = @{
            schema_version = 1
            mode = "full_iso"
            stage_index = 1
            stage_total = 4
            percent_complete = $PercentComplete
            status_text = $StatusText
            detail_text = $DetailText
            last_updated_utc = (Get-Date).ToUniversalTime().ToString("o")
            is_error = $IsError
            error_message = $ErrorMessage
        } | ConvertTo-Json -Depth 4

        $tmpPath = "$StatusPath.tmp"
        $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
        [System.IO.File]::WriteAllText($tmpPath, $payload, $utf8NoBom)
        Move-Item -Path $tmpPath -Destination $StatusPath -Force
    } catch {
        Write-LauncherLog "Launcher status update failed: $_" "WARN"
    }
}

Write-LauncherLog "WinPE launcher started. Preparing deployment engine handoff."
Write-LauncherStatus -PercentComplete 1 -StatusText "Preparing deployment..." -DetailText "Deployment engine handoff started."

if (-not (Test-Path $DeployScript)) {
    Write-LauncherLog "Deploy script not found at $DeployScript" "ERROR"
    Write-LauncherStatus -PercentComplete 100 -StatusText "Deployment failed" -DetailText "Deploy script is missing." -IsError $true -ErrorMessage "Deploy script missing at $DeployScript"
    exit 1
}

if (-not (Test-Path $DeployConfig)) {
    Write-LauncherLog "Deploy config not found at $DeployConfig" "ERROR"
    Write-LauncherStatus -PercentComplete 100 -StatusText "Deployment failed" -DetailText "Deploy config is missing." -IsError $true -ErrorMessage "Deploy config missing at $DeployConfig"
    exit 1
}

Write-LauncherLog "Invoking Deploy-FullIso.ps1 as primary handoff."
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$DeployScript" -ConfigPath "$DeployConfig"
$scriptExit = $LASTEXITCODE
if ($scriptExit -eq 0) {
    Write-LauncherLog "Deploy-FullIso.ps1 completed successfully."
    exit 0
}
Write-LauncherLog "Primary deploy script failed with code $scriptExit." "WARN"
Write-LauncherStatus -PercentComplete 100 -StatusText "Deployment failed" -DetailText "Primary deployment script failed." -IsError $true -ErrorMessage "Deploy-FullIso.ps1 exited with code $scriptExit"

if (Test-Path $NativeExe) {
    Write-LauncherLog "Native executable is present at $NativeExe, but full native deployment fallback is not implemented in this build." "WARN"
    exit $scriptExit
}

Write-LauncherLog "Native fallback unavailable at $NativeExe." "ERROR"
exit $scriptExit
"#
    .replace('\n', "\r\n")
}

fn generate_full_iso_deploy_script() -> String {
    r#"param(
    [string]$ConfigPath = "X:\BitOSDT\Config\deploy.json"
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$LogPath = "X:\BitOSDT\Logs\deploy.log"
$StatusPath = "X:\BitOSDT\State\deploy-status.json"
$NativeExe = "X:\BitOSDT\bitosdt.exe"
$RuntimeDriverConfigPath = "X:\BitOSDT\Config\runtime-drivers.json"
$LogDir = Split-Path -Parent $LogPath
$script:ResolvedImageShareDrive = $null
if (-not (Test-Path $LogDir)) {
    New-Item -Path $LogDir -ItemType Directory -Force | Out-Null
}
if (-not (Test-Path (Split-Path -Parent $StatusPath))) {
    New-Item -Path (Split-Path -Parent $StatusPath) -ItemType Directory -Force | Out-Null
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::AppendAllText($LogPath, "$timestamp [$Level] $Message`r`n", $utf8NoBom)
    Write-Host "[$Level] $Message"
}

function Write-Status {
    param(
        [int]$StageIndex,
        [int]$PercentComplete,
        [string]$StatusText,
        [string]$DetailText,
        [bool]$IsError = $false,
        [string]$ErrorMessage = $null
    )

    try {
        $payload = @{
            schema_version = 1
            mode = "full_iso"
            stage_index = $StageIndex
            stage_total = 4
            percent_complete = $PercentComplete
            status_text = $StatusText
            detail_text = $DetailText
            last_updated_utc = (Get-Date).ToUniversalTime().ToString("o")
            is_error = $IsError
            error_message = $ErrorMessage
        } | ConvertTo-Json -Depth 4

        $tmpPath = "$StatusPath.tmp"
        $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
        [System.IO.File]::WriteAllText($tmpPath, $payload, $utf8NoBom)
        Move-Item -Path $tmpPath -Destination $StatusPath -Force
    } catch {
        Write-Log "Status update failed: $_" "WARN"
    }
}

function Invoke-Logged {
    param(
        [string]$Exe,
        [string[]]$ArgumentList,
        [int]$TimeoutSeconds = 3600
    )

    $displayArgs = if ($ArgumentList -and $ArgumentList.Count -gt 0) { $ArgumentList -join ' ' } else { "" }
    Write-Log "Running: $Exe $displayArgs"

    $safeExeName = [System.IO.Path]::GetFileNameWithoutExtension($Exe)
    if ([string]::IsNullOrWhiteSpace($safeExeName)) {
        $safeExeName = "process"
    }

    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $stdoutPath = Join-Path $LogDir "$safeExeName-$stamp.stdout.log"
    $stderrPath = Join-Path $LogDir "$safeExeName-$stamp.stderr.log"

    if ($ArgumentList -and $ArgumentList.Count -gt 0) {
        $process = Start-Process -FilePath $Exe -ArgumentList $ArgumentList -NoNewWindow -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    } else {
        $process = Start-Process -FilePath $Exe -NoNewWindow -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    }
    $startTime = Get-Date
    [long]$stdoutOffset = 0
    [long]$stderrOffset = 0

    function Flush-LogFile {
        param(
            [string]$Path,
            [ref]$Offset
        )

        if (-not (Test-Path $Path)) {
            return
        }

        $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
        try {
            if ($stream.Length -le $Offset.Value) {
                return
            }

            $stream.Seek($Offset.Value, [System.IO.SeekOrigin]::Begin) | Out-Null
            $reader = New-Object System.IO.StreamReader($stream)
            try {
                while (-not $reader.EndOfStream) {
                    $line = $reader.ReadLine()
                    if ($null -ne $line -and $line.Trim().Length -gt 0) {
                        Write-Log $line
                    }
                }
                $Offset.Value = $stream.Position
            } finally {
                $reader.Dispose()
            }
        } finally {
            $stream.Dispose()
        }
    }

    while (-not $process.HasExited) {
        Flush-LogFile -Path $stdoutPath -Offset ([ref]$stdoutOffset)
        Flush-LogFile -Path $stderrPath -Offset ([ref]$stderrOffset)

        $elapsedSeconds = (Get-Date) - $startTime
        if ($elapsedSeconds.TotalSeconds -gt $TimeoutSeconds) {
            try {
                Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            } catch {
                Write-Log "Unable to stop timed-out process ${Exe}: $_" "WARN"
            }

            Flush-LogFile -Path $stdoutPath -Offset ([ref]$stdoutOffset)
            Flush-LogFile -Path $stderrPath -Offset ([ref]$stderrOffset)
            Write-Log "Command timed out after $TimeoutSeconds seconds (PID=$($process.Id)): $Exe $displayArgs" "ERROR"
            Write-Log "Process logs captured at stdout=$stdoutPath stderr=$stderrPath" "ERROR"
            throw "Command timed out after $TimeoutSeconds seconds: $Exe"
        }

        Start-Sleep -Milliseconds 250
        $process.Refresh()
    }

    Flush-LogFile -Path $stdoutPath -Offset ([ref]$stdoutOffset)
    Flush-LogFile -Path $stderrPath -Offset ([ref]$stderrOffset)

    try {
        $process.WaitForExit()
    } catch {
        Write-Log "WaitForExit failed for ${Exe}: $_" "WARN"
    }

    $exitCode = $null
    try {
        $exitCode = $process.ExitCode
    } catch {
        Write-Log "Unable to read exit code for ${Exe}: $_" "WARN"
    }

    if ($null -eq $exitCode) {
        $fallbackExitCode = $global:LASTEXITCODE
        if ($fallbackExitCode -is [int]) {
            $exitCode = [int]$fallbackExitCode
            $fallbackLevel = if ($exitCode -eq 0) { "INFO" } else { "WARN" }
            Write-Log "Exit code unavailable from process object for $Exe; using LASTEXITCODE=$exitCode" $fallbackLevel
        } else {
            throw "Exit code unavailable from process object for $Exe and LASTEXITCODE is not set."
        }
    }

    if ($exitCode -ne 0) {
        Write-Log "Process logs captured at stdout=$stdoutPath stderr=$stderrPath" "ERROR"
        throw "Command failed with exit code $($exitCode): $Exe"
    }

    Write-Log "Completed: $Exe (exit=$exitCode)"
}

function Write-ScriptLinesToLog {
    param(
        [string]$Label,
        [string]$Content
    )

    Write-Log "$Label"
    foreach ($line in ($Content -split "\r?\n")) {
        if ($null -ne $line -and $line.Trim().Length -gt 0) {
            Write-Log "  $line"
        }
    }
}

function Get-ImageSearchRoots {
    $roots = @("X:")

    try {
        $fsDrives = Get-PSDrive -PSProvider FileSystem |
            Where-Object { $_.Name -match '^[A-Za-z]$' } |
            ForEach-Object { "$($_.Name):" }

        foreach ($drive in $fsDrives) {
            if ($roots -notcontains $drive) {
                $roots += $drive
            }
        }
    } catch {
        Write-Log "Drive enumeration failed: $_" "WARN"
    }

    return $roots
}

function New-ImageResolutionFailureMessage {
    param(
        [System.Collections.Generic.List[string]]$LocalSearched,
        [System.Collections.Generic.List[string]]$LocalMatches,
        [string]$UncFailure = $null,
        [string]$HttpFailure = $null
    )

    $details = New-Object System.Collections.Generic.List[string]
    $searchedList = if ($LocalSearched.Count -gt 0) {
        $LocalSearched -join ", "
    } else {
        "<none>"
    }

    if ($LocalMatches.Count -gt 0) {
        [void]$details.Add("Local search: found Windows images: $($LocalMatches -join ', ')")
    } else {
        [void]$details.Add("Local search: no Windows image found. Searched: $searchedList")
    }

    if (-not [string]::IsNullOrWhiteSpace($UncFailure)) {
        [void]$details.Add($UncFailure)
    }

    if (-not [string]::IsNullOrWhiteSpace($HttpFailure)) {
        [void]$details.Add($HttpFailure)
    }

    return "Windows image not found. " + ($details -join " ")
}

function Get-ShareRoot {
    param([string]$UncPath)

    if ($UncPath -notmatch '^\\\\[^\\]+\\[^\\]+') {
        throw "Invalid UNC path: $UncPath"
    }

    return $matches[0]
}

function Get-AvailableDriveLetter {
    foreach ($candidate in @("Z","Y","V","U","T","R","Q","P","O","N")) {
        if (-not (Get-PSDrive -Name $candidate -ErrorAction SilentlyContinue)) {
            return $candidate
        }
    }

    throw "No free drive letters are available for temporary UNC image mapping."
}

function Resolve-ConfiguredUncImagePath {
    param(
        [string]$UncPath,
        [string]$Username,
        [string]$Password
    )

    if ([string]::IsNullOrWhiteSpace($Username) -or [string]::IsNullOrWhiteSpace($Password)) {
        throw "UNC credentials are missing from the deploy configuration."
    }

    $shareRoot = Get-ShareRoot -UncPath $UncPath
    $relativePath = $UncPath.Substring($shareRoot.Length).TrimStart('\')
    if ([string]::IsNullOrWhiteSpace($relativePath)) {
        throw "UNC image path must include a file path beneath $shareRoot."
    }

    $driveLetter = Get-AvailableDriveLetter
    $securePassword = ConvertTo-SecureString $Password -AsPlainText -Force
    $credential = [pscredential]::new($Username, $securePassword)
    New-PSDrive -Name $driveLetter -PSProvider FileSystem -Root $shareRoot -Credential $credential -Scope Script -ErrorAction Stop | Out-Null
    $script:ResolvedImageShareDrive = $driveLetter
    Write-Log "Mapped UNC image share $shareRoot to drive ${driveLetter}: for authenticated access."

    return Join-Path "${driveLetter}:\\" $relativePath
}

function Cleanup-MappedImageShare {
    if ([string]::IsNullOrWhiteSpace($script:ResolvedImageShareDrive)) {
        return
    }

    try {
        Remove-PSDrive -Name $script:ResolvedImageShareDrive -Scope Script -Force -ErrorAction SilentlyContinue
        Write-Log "Removed temporary UNC image mapping $($script:ResolvedImageShareDrive):."
    } catch {
        Write-Log "Failed to remove temporary UNC image mapping $($script:ResolvedImageShareDrive): $_" "WARN"
    } finally {
        $script:ResolvedImageShareDrive = $null
    }
}

function Resolve-WindowsImagePath {
    param($Config)

    $localSearched = New-Object System.Collections.Generic.List[string]
    $matches = New-Object System.Collections.Generic.List[string]
    $uncFailure = $null
    $httpFailure = $null
    $relativeCandidates = @(
        "\sources\install.wim",
        "\sources\install.esd",
        "\BitOSDT\install.wim",
        "\BitOSDT\install.esd"
    )

    foreach ($root in Get-ImageSearchRoots) {
        foreach ($relative in $relativeCandidates) {
            $candidate = "$root$relative"
            [void]$localSearched.Add($candidate)
            if (Test-Path $candidate) {
                [void]$matches.Add($candidate)
            }
        }
    }

    if ($matches.Count -gt 1) {
        $matchList = $matches -join ", "
        throw "Multiple local Windows images found: $matchList. Keep exactly one local candidate image accessible in WinPE."
    }

    if ($matches.Count -eq 1) {
        return $matches[0]
    }

    $uncImagePath = "$($Config.unc_image_path)".Trim()
    if (-not [string]::IsNullOrWhiteSpace($uncImagePath)) {
        try {
            $resolvedUncPath = Resolve-ConfiguredUncImagePath -UncPath $uncImagePath -Username "$($Config.unc_auth_username)" -Password "$($Config.unc_auth_password)"
            if (Test-Path $resolvedUncPath) {
                return $resolvedUncPath
            }
            Cleanup-MappedImageShare
            $uncFailure = "UNC path configured but not accessible from WinPE: $uncImagePath"
            Write-Log $uncFailure "WARN"
        } catch {
            Cleanup-MappedImageShare
            $uncFailure = "UNC path configured but authentication failed or the mapped path was not accessible from WinPE: $uncImagePath ($($_.Exception.Message))"
            Write-Log $uncFailure "WARN"
        }
    }

    $httpImageUrl = "$($Config.http_image_url)".Trim()
    if (-not [string]::IsNullOrWhiteSpace($httpImageUrl)) {
        $downloadName = if ($httpImageUrl.ToLowerInvariant().EndsWith(".esd")) { "install.esd" } else { "install.wim" }
        $downloadTarget = Join-Path "X:\BitOSDT" $downloadName
        try {
            Write-Log "Attempting HTTP Windows image download from $httpImageUrl"
            Invoke-WebRequest -Uri $httpImageUrl -OutFile $downloadTarget -UseBasicParsing -TimeoutSec 900
            if (Test-Path $downloadTarget) {
                return $downloadTarget
            }
            $httpFailure = "HTTP download attempted from $httpImageUrl but target file was not found: $downloadTarget"
            Write-Log $httpFailure "WARN"
        } catch {
            $httpFailure = "HTTP download attempted from $httpImageUrl and failed: $_"
            Write-Log $httpFailure "WARN"
        }
    }

    if ($matches.Count -eq 0) {
        throw (New-ImageResolutionFailureMessage -LocalSearched $localSearched -LocalMatches $matches -UncFailure $uncFailure -HttpFailure $httpFailure)
    }
}

function Resolve-TargetDisk {
    param($Config)

    $policy = "$($Config.disk_selection_policy)"
    $configuredDisk = $Config.target_disk

    if ($policy -eq "always_disk0") {
        return 0
    }

    if ($policy -eq "require_explicit_disk") {
        if ($null -eq $configuredDisk) {
            throw "Disk policy requires an explicit target disk, but target_disk is null."
        }
        return [int]$configuredDisk
    }

    if ($null -ne $configuredDisk) {
        return [int]$configuredDisk
    }

    $candidates = Get-Disk | Where-Object {
        $_.OperationalStatus -eq "Online" -and
        -not $_.IsReadOnly -and
        @("USB", "SD", "MMC") -notcontains "$($_.BusType)"
    }

    if ($candidates.Count -eq 0) {
        throw "No eligible non-removable disks were found."
    }

    if ($candidates.Count -gt 1) {
        $diskList = ($candidates | ForEach-Object { $_.Number }) -join ", "
        throw "Ambiguous disk selection ($diskList). Set target_disk explicitly."
    }

    return [int]$candidates[0].Number
}

function Invoke-SystemReboot {
    param(
        [int]$FallbackDelaySeconds = 5,
        [int]$WpeutilTimeoutSeconds = 15,
        [int]$WatchdogDelaySeconds = 25
    )

    Write-Log "Arming reboot watchdog (delay=${WatchdogDelaySeconds}s)..."
    $watchdogCommand = "Start-Sleep -Seconds $WatchdogDelaySeconds; shutdown.exe /r /t 0 /f"
    $watchdogStarted = $false
    try {
        $watchdogProcess = Start-Process -FilePath "powershell.exe" -ArgumentList @(
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            $watchdogCommand
        ) -PassThru
        $watchdogStarted = $true
        Write-Log "Reboot watchdog started (PID=$($watchdogProcess.Id))."
    } catch {
        Write-Log "Failed to start reboot watchdog: $_" "WARN"
    }

    Write-Log "Attempting reboot via wpeutil (timeout=${WpeutilTimeoutSeconds}s)..."
    $wpeutilExited = $false
    try {
        $wpeutilProcess = Start-Process -FilePath "wpeutil.exe" -ArgumentList @("reboot") -PassThru
        if ($wpeutilProcess.WaitForExit($WpeutilTimeoutSeconds * 1000)) {
            $wpeutilExited = $true
            Write-Log "wpeutil reboot exited with code $($wpeutilProcess.ExitCode)."
        } else {
            Write-Log "wpeutil reboot timed out after $WpeutilTimeoutSeconds seconds. Terminating process." "WARN"
            try {
                Stop-Process -Id $wpeutilProcess.Id -Force -ErrorAction SilentlyContinue
            } catch {
                Write-Log "Unable to terminate timed-out wpeutil process: $_" "WARN"
            }
        }
    } catch {
        Write-Log "wpeutil reboot failed to start or execute: $_" "WARN"
    }

    if ($wpeutilExited) {
        Write-Log "wpeutil reboot returned control; waiting $FallbackDelaySeconds seconds before forced fallback."
        Start-Sleep -Seconds $FallbackDelaySeconds
    } else {
        Write-Log "Proceeding immediately to forced reboot fallback because wpeutil did not complete." "WARN"
    }

    Write-Log "Issuing forced reboot fallback via shutdown.exe /r /t 0 /f" "WARN"
    try {
        & shutdown.exe /r /t 0 /f
    } catch {
        throw "Fallback reboot command failed: $_"
    }

    $shutdownExit = $LASTEXITCODE
    if ($shutdownExit -is [int] -and $shutdownExit -ne 0) {
        throw "Fallback reboot command returned exit code $shutdownExit"
    }

    if ($watchdogStarted) {
        Write-Log "Watchdog remains armed as final safety net."
    } else {
        Write-Log "Watchdog was not armed; relying on issued fallback reboot command." "WARN"
    }

    Start-Sleep -Seconds 5
    throw "Reboot command issued but the system did not restart."
}

function Invoke-PostBootHandoff {
    param([bool]$IsUefi)

    Write-Log "Refreshing boot metadata before reboot..."
    try {
        wpeutil UpdateBootInfo | Out-Null
    } catch {
        Write-Log "wpeutil UpdateBootInfo failed during post-boot handoff: $_" "WARN"
    }

    if ($IsUefi) {
        Write-Log "Attempting one-time firmware bootsequence handoff to Windows Boot Manager..."
        $bootSequenceExit = $null
        try {
            & bcdedit.exe /set "{fwbootmgr}" bootsequence "{bootmgr}"
            $bootSequenceExit = $LASTEXITCODE
        } catch {
            Write-Log "bcdedit bootsequence command threw an exception: $_" "WARN"
        }

        if ($bootSequenceExit -is [int] -and $bootSequenceExit -ne 0) {
            Write-Log "bcdedit bootsequence returned exit code $bootSequenceExit" "WARN"
        }
    }
}

$deployExitCode = 0

try {
    Write-Log "BitOSDT Full ISO deployment script started."
    Write-Status -StageIndex 1 -PercentComplete 2 -StatusText "Preparing deployment..." -DetailText "Validating configuration and image paths."

    if (-not (Test-Path $ConfigPath)) {
        throw "Deploy config not found at $ConfigPath"
    }

    $config = Get-Content -Path $ConfigPath -Raw | ConvertFrom-Json
    $wimPath = Resolve-WindowsImagePath -Config $config
    Write-Log "Selected Windows image: $wimPath"
    Write-Status -StageIndex 1 -PercentComplete 10 -StatusText "Preparing deployment..." -DetailText "Resolved Windows image and deployment policy."

    $wimIndex = [int]$config.wim_index
    if ($wimIndex -lt 1) {
        $wimIndex = 1
    }

    $targetDisk = Resolve-TargetDisk -Config $config
    Write-Log "Selected target disk: $targetDisk"
    Write-Status -StageIndex 1 -PercentComplete 18 -StatusText "Preparing deployment..." -DetailText "Target disk selected. Preparing partitions."

    wpeutil UpdateBootInfo | Out-Null
    $fwType = (Get-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control" -Name PEFirmwareType -ErrorAction SilentlyContinue).PEFirmwareType
    $isUefi = $fwType -eq 2
    Write-Log ("Firmware mode: " + $(if ($isUefi) { "UEFI" } else { "BIOS/Legacy" }))

    $diskpartScript = if ($isUefi) {
@"
select disk $targetDisk
clean
convert gpt
create partition efi size=100
format quick fs=fat32 label="System"
assign letter=S
create partition msr size=16
create partition primary
format quick fs=ntfs label="Windows"
assign letter=W
"@
    } else {
@"
select disk $targetDisk
clean
convert mbr
create partition primary size=500
format quick fs=ntfs label="System"
assign letter=S
active
create partition primary
format quick fs=ntfs label="Windows"
assign letter=W
"@
    }

    $diskpartScriptPath = "X:\BitOSDT\Logs\partition.txt"
    Write-Log "Writing diskpart script to $diskpartScriptPath"
    Write-ScriptLinesToLog -Label "Diskpart script contents:" -Content $diskpartScript
    Set-Content -Path $diskpartScriptPath -Value $diskpartScript -Encoding ascii
    $diskpartTimer = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-Logged -Exe "diskpart.exe" -ArgumentList @("/s", $diskpartScriptPath) -TimeoutSeconds 300
    $diskpartTimer.Stop()
    Write-Log ("Disk partitioning elapsed seconds: {0:N2}" -f $diskpartTimer.Elapsed.TotalSeconds)
    Write-Status -StageIndex 1 -PercentComplete 20 -StatusText "Preparing deployment..." -DetailText "Disk partitioning complete."

    Write-Status -StageIndex 2 -PercentComplete 22 -StatusText "Applying Windows image..." -DetailText "Running DISM /Apply-Image."
    $dismLogPath = "X:\BitOSDT\Logs\dism-apply.log"
    $applyTimer = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-Logged -Exe "dism.exe" -ArgumentList @(
        "/Apply-Image",
        "/ImageFile:$wimPath",
        "/Index:$wimIndex",
        "/ApplyDir:W:\",
        "/LogPath:$dismLogPath"
    )
    $applyTimer.Stop()
    Write-Log ("DISM apply elapsed seconds: {0:N2}" -f $applyTimer.Elapsed.TotalSeconds)
    Write-Log "DISM apply log path: $dismLogPath"
    Write-Status -StageIndex 2 -PercentComplete 80 -StatusText "Applying Windows image..." -DetailText "Windows image applied successfully."

    Write-Status -StageIndex 3 -PercentComplete 82 -StatusText "Installing drivers..." -DetailText "Running shared native runtime driver stage."
    $driverStageTimer = [System.Diagnostics.Stopwatch]::StartNew()
    if (Test-Path $NativeExe) {
        Write-Log "Invoking native runtime driver stage with $NativeExe"
        Invoke-Logged -Exe $NativeExe -ArgumentList @(
            "runtime-drivers",
            "--config",
            $RuntimeDriverConfigPath,
            "--windows-path",
            "W:\"
        )
    } else {
        Write-Log "Native runtime executable unavailable at $NativeExe. Skipping shared runtime driver stage." "WARN"
    }
    $driverStageTimer.Stop()
    Write-Log ("Driver stage elapsed seconds: {0:N2}" -f $driverStageTimer.Elapsed.TotalSeconds)

    Write-Status -StageIndex 3 -PercentComplete 90 -StatusText "Configuring bootloader..." -DetailText "Running BCDBoot."
    $bcdFirmwareType = if ($isUefi) { "UEFI" } else { "BIOS" }
    Write-Log "Running BCDBoot with firmware mode: $bcdFirmwareType"
    $bootloaderTimer = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-Logged -Exe "bcdboot.exe" -ArgumentList @(
        "W:\Windows",
        "/s",
        "S:",
        "/f",
        $bcdFirmwareType
    )
    $bootloaderTimer.Stop()
    Write-Log ("Bootloader stage elapsed seconds: {0:N2}" -f $bootloaderTimer.Elapsed.TotalSeconds)
    Write-Status -StageIndex 3 -PercentComplete 95 -StatusText "Configuring bootloader..." -DetailText "Bootloader configuration complete."
    Invoke-PostBootHandoff -IsUefi:$isUefi

    Write-Log "Deployment completed successfully. Rebooting into Windows..."
    Write-Status -StageIndex 4 -PercentComplete 100 -StatusText "Finalizing deployment..." -DetailText "Deployment complete. Rebooting." 
    Invoke-SystemReboot
} catch {
    Write-Status -StageIndex 4 -PercentComplete 100 -StatusText "Deployment failed" -DetailText "Review X:\BitOSDT\Logs\deploy.log" -IsError $true -ErrorMessage "$_"
    Write-Log "Deployment failed: $_" "ERROR"
    $deployExitCode = 1
} finally {
    Cleanup-MappedImageShare
}

if ($deployExitCode -ne 0) {
    exit $deployExitCode
}
"#
    .replace('\n', "\r\n")
}

fn resolve_source_path<F>(
    source_path: &Path,
    workspace: &Path,
    emit: &mut impl FnMut(&str, u32, &str, &mut F),
    callback: &mut F,
) -> BitOSDTResult<PathBuf>
where
    F: FnMut(FullIsoProgress),
{
    let extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "esd" | "wim" => Ok(source_path.to_path_buf()),
        "iso" => {
            emit("extract", 10, "Extracting Windows ISO...", callback);
            let extract_dir = workspace.join("extracted");
            fs::create_dir_all(&extract_dir)?;
            extract_iso(source_path, &extract_dir)?;

            let sources_dir = extract_dir.join("sources");
            let install_esd = sources_dir.join("install.esd");
            if install_esd.exists() {
                return Ok(install_esd);
            }
            let install_wim = sources_dir.join("install.wim");
            if install_wim.exists() {
                return Ok(install_wim);
            }

            Err(BitOSDTError::NotFound(
                "No install.esd or install.wim found in extracted ISO".to_string(),
            ))
        }
        other => Err(BitOSDTError::InvalidInput(format!(
            "Unsupported source format '{}'",
            other
        ))),
    }
}

#[cfg(target_os = "windows")]
fn extract_iso(source_path: &Path, extract_dir: &Path) -> BitOSDTResult<()> {
    let source = source_path.to_string_lossy().replace('\'', "''");
    let destination = extract_dir.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$iso = Mount-DiskImage -ImagePath '{source}' -PassThru; \
         $drive = ($iso | Get-Volume).DriveLetter; \
         Copy-Item -Path \"$drive`:\\*\" -Destination '{destination}' -Recurse -Force; \
         Dismount-DiskImage -ImagePath '{source}'"
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| BitOSDTError::WinPE(format!("Failed to extract ISO: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(BitOSDTError::WinPE(format!(
            "ISO extraction failed (exit={:?}, stderr={})",
            output.status.code(),
            if stderr.is_empty() {
                "<empty>"
            } else {
                &stderr
            }
        )));
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn extract_iso(_source_path: &Path, _extract_dir: &Path) -> BitOSDTResult<()> {
    extract_iso_image(_source_path, _extract_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{CustomInstaller, LocalPayloadItem, LocalPayloadKind};
    use std::fs;
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

    fn temp_file_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bitosdt-{}-{}", Uuid::new_v4(), name))
    }

    #[test]
    fn full_iso_language_settings_supports_french_locale() {
        let (language, input_locale) =
            resolve_unattend_locale_settings("fr-fr").expect("fr-fr should resolve");
        assert_eq!(language, "fr-FR");
        assert_eq!(input_locale, "fr-FR");
    }

    #[test]
    fn preprocess_task_sequence_stages_embedded_installers() {
        let embedded_path = temp_file_path("tool.msi");
        fs::write(&embedded_path, b"dummy").expect("failed to create temp installer");

        let sequence = TaskSequence {
            id: Uuid::new_v4(),
            name: "Test".to_string(),
            tasks: vec![crate::tasks::TaskDefinition {
                id: Uuid::new_v4(),
                name: "Install Apps".to_string(),
                task_type: TaskType::InstallApps(AppInstallConfig {
                    copied_items: vec![],
                    copy_destination: None,
                    winget_packages: vec![],
                    chocolatey_packages: vec![],
                    custom_installers: vec![CustomInstaller {
                        name: "My Tool".to_string(),
                        path: embedded_path.to_string_lossy().to_string(),
                        source_type: InstallerSourceType::EmbeddedFile,
                        source_file_name: None,
                        dependencies: vec![],
                        dependency_destination: None,
                        silent_args: "/qn".to_string(),
                        installer_type: InstallerType::Msi,
                        success_codes: vec![0, 3010],
                        enabled: true,
                    }],
                    auto_install_chocolatey: true,
                    continue_on_error: true,
                    log_path: "C:\\BitOSDT\\Logs\\app-install.log".to_string(),
                    progress_json_path: None,
                }),
                order: 10,
                enabled: true,
                continue_on_error: true,
                requires_reboot: false,
            }],
            settings: crate::tasks::TaskSettings::default(),
        };

        let (processed, inject_files) =
            preprocess_task_sequence_for_full_iso(Some(&sequence)).expect("preprocess failed");

        let processed = processed.expect("expected processed task sequence");
        assert_eq!(inject_files.len(), 1);
        assert_eq!(
            inject_files[0].destination,
            r"BitOSDT\Installers\my-tool-1.msi"
        );

        let task = processed.tasks.first().expect("missing task");
        let app_config = match &task.task_type {
            TaskType::InstallApps(cfg) => cfg,
            _ => panic!("unexpected task type"),
        };
        let installer = app_config
            .custom_installers
            .first()
            .expect("missing installer");
        assert_eq!(installer.path, r"C:\BitOSDT\Installers\my-tool-1.msi");
        assert_eq!(installer.source_type, InstallerSourceType::DirectPathOrUrl);

        let _ = fs::remove_file(&embedded_path);
    }

    #[test]
    fn preprocess_task_sequence_rejects_missing_embedded_installers() {
        let missing = temp_file_path("missing.msi");
        let sequence = TaskSequence {
            id: Uuid::new_v4(),
            name: "Test".to_string(),
            tasks: vec![crate::tasks::TaskDefinition {
                id: Uuid::new_v4(),
                name: "Install Apps".to_string(),
                task_type: TaskType::InstallApps(AppInstallConfig {
                    copied_items: vec![],
                    copy_destination: None,
                    winget_packages: vec![],
                    chocolatey_packages: vec![],
                    custom_installers: vec![CustomInstaller {
                        name: "Missing".to_string(),
                        path: missing.to_string_lossy().to_string(),
                        source_type: InstallerSourceType::EmbeddedFile,
                        source_file_name: None,
                        dependencies: vec![],
                        dependency_destination: None,
                        silent_args: "".to_string(),
                        installer_type: InstallerType::Msi,
                        success_codes: vec![0, 3010],
                        enabled: true,
                    }],
                    auto_install_chocolatey: true,
                    continue_on_error: true,
                    log_path: "C:\\BitOSDT\\Logs\\app-install.log".to_string(),
                    progress_json_path: None,
                }),
                order: 10,
                enabled: true,
                continue_on_error: true,
                requires_reboot: false,
            }],
            settings: crate::tasks::TaskSettings::default(),
        };

        let err = preprocess_task_sequence_for_full_iso(Some(&sequence))
            .expect_err("expected missing embedded installer error");
        assert!(err.to_string().contains("Embedded installer not found"));
    }

    #[test]
    fn preprocess_task_sequence_stages_payload_files_and_directories() {
        let root = temp_file_path("payload-preprocess");
        let dir_payload = root.join("VendorFiles");
        fs::create_dir_all(dir_payload.join("Nested")).expect("create nested payload dir");
        let file_payload = root.join("config.json");
        fs::write(&file_payload, b"{}").expect("write payload file");
        fs::write(dir_payload.join("Nested").join("tool.dll"), b"dll").expect("write nested file");

        let sequence = TaskSequence {
            id: Uuid::new_v4(),
            name: "Payload Test".to_string(),
            tasks: vec![crate::tasks::TaskDefinition {
                id: Uuid::new_v4(),
                name: "Install Apps".to_string(),
                task_type: TaskType::InstallApps(AppInstallConfig {
                    copied_items: vec![
                        LocalPayloadItem {
                            source_path: file_payload.to_string_lossy().to_string(),
                            source_kind: LocalPayloadKind::File,
                            display_name: None,
                        },
                        LocalPayloadItem {
                            source_path: dir_payload.to_string_lossy().to_string(),
                            source_kind: LocalPayloadKind::Directory,
                            display_name: None,
                        },
                    ],
                    copy_destination: Some(r"C:\BitOSDT\Files".to_string()),
                    winget_packages: vec![],
                    chocolatey_packages: vec![],
                    custom_installers: vec![],
                    auto_install_chocolatey: true,
                    continue_on_error: true,
                    log_path: "C:\\BitOSDT\\Logs\\app-install.log".to_string(),
                    progress_json_path: None,
                }),
                order: 10,
                enabled: true,
                continue_on_error: true,
                requires_reboot: false,
            }],
            settings: crate::tasks::TaskSettings::default(),
        };

        let (processed, inject_files) =
            preprocess_task_sequence_for_full_iso(Some(&sequence)).expect("preprocess failed");

        let processed = processed.expect("processed sequence");
        let app_config = match &processed.tasks[0].task_type {
            TaskType::InstallApps(cfg) => cfg,
            _ => panic!("unexpected task type"),
        };
        assert!(app_config.copied_items.is_empty());
        assert_eq!(inject_files.len(), 2);
        assert!(inject_files
            .iter()
            .any(|item| item.destination == r"BitOSDT\Files\config.json"));
        assert!(inject_files
            .iter()
            .any(|item| item.destination == r"BitOSDT\Files\VendorFiles\Nested\tool.dll"));

        let _ = fs::remove_dir_all(&root);
    }

    fn test_full_iso_config() -> FullIsoBuildConfig {
        FullIsoBuildConfig {
            source_path: PathBuf::from("test.wim"),
            output_path: PathBuf::from("test.iso"),
            volume_label: "BITOSDT".to_string(),
            windows_version: "Windows 11".to_string(),
            windows_build: "24H2".to_string(),
            windows_edition: "Enterprise".to_string(),
            language: "en-US".to_string(),
            architecture: "amd64".to_string(),
            wim_index: 1,
            target_disk: None,
            disk_selection_policy: DiskSelectionPolicy::ConfigFirstSafeFallback,
            unattend: UnattendConfig::default(),
            autopilot: None,
            task_sequence: None,
            runtime_domain_join: None,
            workspace: None,
            download_dir: None,
            adk_paths: None,
            winpe_assets_dir: None,
            winpe_packages_dir: None,
            ui_dir: None,
            native_executable: None,
            common_boot_driver_dir: None,
            runtime_driver_catalog: Vec::new(),
            runtime_driver_cache_source: None,
            driver_paths: vec![],
            apply_drivers_to_offline_windows: true,
            runtime_driver_policy: RuntimeDriverPolicy::default(),
            unc_image_path: None,
            unc_auth_username: None,
            unc_auth_password: None,
            http_image_url: None,
            prompt_unc_credentials_at_runtime: None,
        }
    }

    #[test]
    fn full_iso_progress_ranges_are_monotonic() {
        assert_eq!(
            scale_progress_range(CONVERT_PROGRESS_START, CONVERT_PROGRESS_END, 0),
            CONVERT_PROGRESS_START
        );
        assert_eq!(
            scale_progress_range(CONVERT_PROGRESS_START, CONVERT_PROGRESS_END, 100),
            CONVERT_PROGRESS_END
        );
        assert_eq!(
            scale_progress_range(PREP_PROGRESS_START, PREP_PROGRESS_END, 0),
            PREP_PROGRESS_START
        );
        assert_eq!(
            scale_progress_range(PREP_PROGRESS_START, PREP_PROGRESS_END, 100),
            PREP_PROGRESS_END
        );
        assert!(CONVERT_PROGRESS_END < PREP_PROGRESS_START);
    }

    #[test]
    fn scale_progress_range_clamps_percent_overflow() {
        assert_eq!(scale_progress_range(45, 74, 150), 74);
    }

    #[test]
    fn customize_full_iso_winpe_copies_packages() {
        let root = temp_file_path("fulliso-winpe");
        let mount_dir = root.join("mount");
        let packages_dir = root.join("Packages");
        let native_runtime = root.join("bitosdt.exe");

        fs::create_dir_all(mount_dir.join("Windows").join("System32"))
            .expect("create mount system32");
        fs::create_dir_all(&packages_dir).expect("create packages dir");
        fs::create_dir_all(packages_dir.join("tools")).expect("create packages tools");
        fs::write(packages_dir.join("tools").join("utility.exe"), b"dummy")
            .expect("write dummy utility");
        fs::write(&native_runtime, b"runtime").expect("write native runtime");

        let mut config = test_full_iso_config();
        config.winpe_packages_dir = Some(packages_dir);
        config.native_executable = Some(native_runtime);

        let builder = WinPEBuilder::new(root.join("workspace"), "amd64".to_string());
        let payload_provenance = PayloadProvenance {
            size_bytes: 7,
            sha256: "abc123".to_string(),
            file_name: Some("install.wim".to_string()),
        };
        customize_full_iso_winpe(&builder, &mount_dir, &config, &payload_provenance)
            .expect("customize should succeed");

        assert!(
            mount_dir
                .join("BitOSDT")
                .join("Packages")
                .join("tools")
                .join("utility.exe")
                .exists(),
            "tools package should be copied into WinPE"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn full_iso_startnet_contains_deploy_handoff() {
        let startnet = generate_full_iso_startnet(false);
        assert!(startnet.contains("Deploy-FullIso.ps1"));
        assert!(startnet.contains("deploy.json"));
        assert!(startnet.contains("cmd /k"));
        assert!(startnet.contains("goto :eof"));
        assert!(startnet.contains("if not exist \"%DEPLOY_SCRIPT%\""));
        assert!(startnet.contains("if not exist \"%DEPLOY_CONFIG%\""));
        assert!(startnet.contains("set WRAPPER=X:\\BitOSDT\\Scripts\\Launch-BitOSDT-WinPE.cmd"));
    }

    #[test]
    fn shell_startnet_prefers_wrapper_and_falls_back() {
        let startnet = generate_full_iso_startnet(false);
        assert!(startnet.contains("set WRAPPER=X:\\BitOSDT\\Scripts\\Launch-BitOSDT-WinPE.cmd"));
        assert!(startnet
            .contains("set COMPAT_FLAG=X:\\BitOSDT\\Config\\enable-winpe-compat-spoof.flag"));
        assert!(startnet.contains("Set-WinPE-CompatibilitySpoof.ps1"));
        assert!(startnet.contains("call \"%WRAPPER%\""));
        assert!(startnet.contains("Executing direct fallback"));
        assert!(startnet.contains("Shell wrapper missing at \"%WRAPPER%\""));
        assert!(startnet.contains("Deploy-FullIso.ps1"));
        assert!(startnet.contains("set STARTNET_LOG=X:\\BitOSDT\\Logs\\startnet.log"));
        assert!(startnet.contains("Invoking shell wrapper \"%WRAPPER%\""));
        assert!(startnet.contains("Invoking fallback deploy script \"%DEPLOY_SCRIPT%\""));
        assert!(startnet.contains("Deployment config missing at \"%DEPLOY_CONFIG%\""));
    }

    #[test]
    fn native_full_iso_startnet_uses_winpe_deploy_command() {
        let startnet = generate_full_iso_startnet(true);
        assert!(startnet.contains("set DEPLOY_EXE=X:\\BitOSDT\\bitosdt.exe"));
        assert!(startnet.contains("winpe-deploy --config"));
        assert!(startnet.contains("runtime-drivers.json"));
        assert!(!startnet.contains("Deploy-FullIso.ps1"));
        assert!(!startnet.contains("Set-WinPE-CompatibilitySpoof.ps1"));
    }

    #[test]
    fn shell_launcher_script_starts_with_param_block() {
        let script = generate_full_iso_shell_launcher_script();
        let primary_handoff = script
            .find("Invoking Deploy-FullIso.ps1 as primary handoff.")
            .expect("primary handoff marker missing");
        let native_handoff = script
            .find("full native deployment fallback is not implemented")
            .expect("native fallback note missing");
        assert_eq!(first_script_executable_line(&script), Some("param()"));
        assert!(primary_handoff < native_handoff);
        assert!(script.contains("Write-LauncherStatus -PercentComplete 1"));
        assert!(script.contains("WinPE launcher started. Preparing deployment engine handoff."));
        assert!(script.contains("Deploy-FullIso.ps1"));
    }

    #[test]
    fn deploy_script_starts_with_top_level_param_block() {
        let script = generate_full_iso_deploy_script();
        assert_eq!(first_script_executable_line(&script), Some("param("));
    }

    #[test]
    fn deploy_config_serializes_disk_policy_and_target() {
        let cfg = FullIsoDeployConfig {
            mode: "full_iso".to_string(),
            os_version: "24H2".to_string(),
            wim_index: 3,
            target_disk: Some(1),
            disk_selection_policy: DiskSelectionPolicy::ConfigFirstSafeFallback,
            runtime_driver_policy: RuntimeDriverPolicy::default(),
            runtime_driver_context: RuntimeDriverContext::winpe_default(),
            unc_image_path: Some(r"\\wds\reminst\images\install.wim".to_string()),
            unc_auth_username: Some(r"CONTOSO\deploy".to_string()),
            unc_auth_password: Some("Secret123!".to_string()),
            http_image_url: Some("http://deploy.local/install.wim".to_string()),
            expected_payload_size_bytes: Some(123456),
            expected_payload_sha256: Some("abc123".to_string()),
            expected_payload_file_name: Some("install.wim".to_string()),
            unattend: UnattendConfig::default(),
            autopilot: Some(AutopilotProfile {
                tenant_id: "tenant-id".to_string(),
                tenant_domain: "tenant-id.onmicrosoft.com".to_string(),
                device_name_template: None,
                deployment_mode: crate::config::DeploymentMode::UserDriven,
                oobe_config: crate::config::AutopilotOobeConfig::default(),
                group_tag: Some("Branch-A".to_string()),
                assigned_user: None,
            }),
            task_sequence: Some(TaskSequence {
                id: uuid::Uuid::new_v4(),
                name: "BitOSDT".to_string(),
                tasks: vec![],
                settings: crate::tasks::TaskSettings::default(),
            }),
            runtime_domain_join: Some(RuntimeDomainJoinConfig {
                enabled: true,
                prompt_for_credentials_at_runtime: true,
                default_domain: Some("contoso.local".to_string()),
                default_ou_path: Some("OU=Devices,DC=contoso,DC=local".to_string()),
            }),
            prompt_unc_credentials_at_runtime: None,
        };

        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"mode\":\"full_iso\""));
        assert!(json.contains("\"os_version\":\"24H2\""));
        assert!(json.contains("\"wim_index\":3"));
        assert!(json.contains("\"target_disk\":1"));
        assert!(json.contains("\"config_first_safe_fallback\""));
        assert!(json.contains("\"runtime_driver_policy\""));
        assert!(json.contains("\\\\\\\\wds\\\\reminst\\\\images\\\\install.wim"));
        assert!(json.contains("\"unc_auth_username\":\"CONTOSO\\\\deploy\""));
        assert!(json.contains("\"unc_auth_password\":\"Secret123!\""));
        assert!(json.contains("http://deploy.local/install.wim"));
        assert!(json.contains("driverpacks.json"));
        assert!(json.contains("\"expected_payload_size_bytes\":123456"));
        assert!(json.contains("\"expected_payload_sha256\":\"abc123\""));
        assert!(json.contains("\"expected_payload_file_name\":\"install.wim\""));
        assert!(json.contains("\"tenant_id\":\"tenant-id\""));
        assert!(json.contains("\"name\":\"BitOSDT\""));
        assert!(json.contains("\"runtime_domain_join\""));
        assert!(json.contains("\"prompt_for_credentials_at_runtime\":true"));
    }

    #[test]
    fn collect_payload_provenance_captures_size_hash_and_name() {
        let payload_path = temp_file_path("prepared-install.wim");
        fs::write(&payload_path, b"prepared-payload").expect("write payload");

        let provenance =
            collect_payload_provenance(&payload_path).expect("collect payload provenance");

        assert_eq!(provenance.size_bytes, 16);
        assert_eq!(
            provenance.file_name.as_deref(),
            payload_path.file_name().and_then(|value| value.to_str())
        );
        assert_eq!(
            provenance.sha256,
            HashValidator::calculate_sha256(&payload_path).expect("hash payload")
        );

        let _ = fs::remove_file(payload_path);
    }

    #[test]
    fn deploy_script_delimits_process_exit_code_before_colon() {
        let script = generate_full_iso_deploy_script();
        let invalid_exe_interpolation = ["$Ex", "e:"].concat();
        assert!(script.contains("Command failed with exit code $($exitCode): $Exe"));
        assert!(!script.contains("${LASTEXITCODE}:"));
        assert!(!script.contains(&invalid_exe_interpolation));
        assert!(script.contains("Unable to stop timed-out process ${Exe}: $_"));
    }

    #[test]
    fn deploy_script_resolves_windows_image_paths_dynamically() {
        let script = generate_full_iso_deploy_script();
        assert!(script.contains("function Resolve-WindowsImagePath"));
        assert!(script.contains("function Get-ImageSearchRoots"));
        assert!(script.contains("function New-ImageResolutionFailureMessage"));
        assert!(script.contains("function Resolve-ConfiguredUncImagePath"));
        assert!(script.contains("function Cleanup-MappedImageShare"));
        assert!(script.contains("$script:ResolvedImageShareDrive = $null"));
        assert!(script.contains("\\sources\\install.wim"));
        assert!(script.contains("\\sources\\install.esd"));
        assert!(script.contains("\\BitOSDT\\install.wim"));
        assert!(script.contains("\\BitOSDT\\install.esd"));
        assert!(script.contains("Local search: no Windows image found."));
        assert!(script.contains("UNC path configured but not accessible from WinPE:"));
        assert!(script.contains("UNC path configured but authentication failed or the mapped path was not accessible from WinPE:"));
        assert!(script.contains("$($Config.unc_auth_username)"));
        assert!(script.contains("$($Config.unc_auth_password)"));
        assert!(script.contains("New-PSDrive -Name $driveLetter"));
        assert!(script.contains("Remove-PSDrive -Name $script:ResolvedImageShareDrive"));
        assert!(script.contains("HTTP download attempted from $httpImageUrl and failed:"));
        assert!(script.contains("Multiple local Windows images found:"));
        assert!(script.contains("$wimPath = Resolve-WindowsImagePath -Config $config"));
        assert!(script.contains("Selected Windows image: $wimPath"));
        assert!(script
            .contains("$RuntimeDriverConfigPath = \"X:\\BitOSDT\\Config\\runtime-drivers.json\""));
        assert!(script.contains("\"runtime-drivers\""));
        assert!(script.contains("\"--windows-path\""));
    }

    #[test]
    fn deploy_script_streams_output_with_timeout_and_diskpart_diagnostics() {
        let script = generate_full_iso_deploy_script();
        let invalid_exe_interpolation = ["$Ex", "e:"].concat();
        assert!(script.contains("[int]$TimeoutSeconds = 3600"));
        assert!(!script.contains("[string[]]$Args"));
        assert!(script.contains("[string[]]$ArgumentList"));
        assert!(script.contains("if ($ArgumentList -and $ArgumentList.Count -gt 0) {"));
        assert!(script.contains(
            "Start-Process -FilePath $Exe -ArgumentList $ArgumentList -NoNewWindow -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath"
        ));
        assert!(script.contains(
            "Start-Process -FilePath $Exe -NoNewWindow -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath"
        ));
        assert!(script.contains("while (-not $process.HasExited)"));
        assert!(script.contains("$process.WaitForExit()"));
        assert!(script.contains("if ($null -eq $exitCode)"));
        assert!(script.contains("Command timed out after $TimeoutSeconds seconds"));
        assert!(script.contains("Process logs captured at stdout=$stdoutPath stderr=$stderrPath"));
        assert!(script.contains("Unable to stop timed-out process ${Exe}: $_"));
        assert!(!script.contains(&invalid_exe_interpolation));
        assert!(script.contains("function Write-ScriptLinesToLog"));
        assert!(script.contains("Write-Log \"Writing diskpart script to $diskpartScriptPath\""));
        assert!(script.contains(
            "Write-ScriptLinesToLog -Label \"Diskpart script contents:\" -Content $diskpartScript"
        ));
        assert!(script.contains(
            "Invoke-Logged -Exe \"diskpart.exe\" -ArgumentList @(\"/s\", $diskpartScriptPath) -TimeoutSeconds 300"
        ));
        assert!(script.contains("$diskpartTimer = [System.Diagnostics.Stopwatch]::StartNew()"));
        assert!(script.contains("Disk partitioning elapsed seconds"));
        assert!(script.contains("$dismLogPath = \"X:\\BitOSDT\\Logs\\dism-apply.log\""));
        assert!(script.contains("/LogPath:$dismLogPath"));
        assert!(script.contains("DISM apply elapsed seconds"));
        assert!(script.contains("Driver stage elapsed seconds"));
        assert!(script.contains("Bootloader stage elapsed seconds"));
        assert!(script.contains("Invoke-Logged -Exe $NativeExe -ArgumentList @("));
        assert!(script.contains("$RuntimeDriverConfigPath"));
        assert!(script.contains("function Invoke-SystemReboot"));
        assert!(script.contains("function Invoke-PostBootHandoff"));
        assert!(script.contains("wpeutil UpdateBootInfo | Out-Null"));
        assert!(script.contains(
            "$watchdogCommand = \"Start-Sleep -Seconds $WatchdogDelaySeconds; shutdown.exe /r /t 0 /f\""
        ));
        assert!(script.contains(
            "Start-Process -FilePath \"wpeutil.exe\" -ArgumentList @(\"reboot\") -PassThru"
        ));
        assert!(script.contains("$wpeutilProcess.WaitForExit($WpeutilTimeoutSeconds * 1000)"));
        assert!(script.contains(
            "Proceeding immediately to forced reboot fallback because wpeutil did not complete."
        ));
        assert!(script.contains("& shutdown.exe /r /t 0 /f"));
        assert!(script.contains("$bcdFirmwareType = if ($isUefi) { \"UEFI\" } else { \"BIOS\" }"));
        assert!(script.contains("& bcdedit.exe /set \"{fwbootmgr}\" bootsequence \"{bootmgr}\""));
        assert!(script.contains("throw \"Reboot command issued but the system did not restart.\""));
    }

    #[test]
    fn deploy_script_writes_status_file_updates() {
        let script = generate_full_iso_deploy_script();
        assert!(script.contains("$StatusPath = \"X:\\BitOSDT\\State\\deploy-status.json\""));
        assert!(script.contains("function Write-Status"));
        assert!(script.contains("schema_version = 1"));
        assert!(script.contains("mode = \"full_iso\""));
        assert!(script.contains("System.Text.UTF8Encoding($false)"));
        assert!(script.contains("[System.IO.File]::WriteAllText($tmpPath, $payload, $utf8NoBom)"));
        assert!(script.contains("Move-Item -Path $tmpPath -Destination $StatusPath -Force"));
        assert!(script.contains("StageIndex 2"));
        assert!(script.contains("PercentComplete 80"));
        assert!(script.contains("-IsError $true"));
    }
}
