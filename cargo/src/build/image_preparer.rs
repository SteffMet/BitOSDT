#[cfg(not(target_os = "windows"))]
use crate::build::linux_support::apply_wim_updates_from_directory;
use crate::config::{AutopilotGenerator, AutopilotProfile, UnattendConfig, UnattendGenerator};
use crate::core::adk::{resolve_adk_paths, AdkPaths};
use crate::core::errors::{BitOSDTError, BitOSDTResult};
#[cfg(target_os = "windows")]
use crate::core::windows_tools::{
    dism_path_arg, format_process_failure, resolve_dism_exe, run_dism, run_dism_streaming_with_role,
};
use crate::tasks::{TaskRunner, TaskSequence};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Image preparer for modifying Windows installation images
pub struct ImagePreparer {
    _work_dir: PathBuf,
    mount_dir: PathBuf,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    adk_paths: Option<AdkPaths>,
}

/// Configuration for image preparation
#[derive(Debug, Clone)]
pub struct ImagePrepConfig {
    /// Path to source WIM file
    pub source_wim: PathBuf,
    /// WIM index to modify
    pub wim_index: u32,
    /// Unattend configuration (optional)
    pub unattend: Option<UnattendConfig>,
    /// Autopilot profile (optional)
    pub autopilot: Option<AutopilotProfile>,
    /// Task sequence (optional)
    pub task_sequence: Option<TaskSequence>,
    /// Additional files to inject
    pub inject_files: Vec<FileInjection>,
    /// Additional drivers to inject
    pub driver_paths: Vec<PathBuf>,
    /// Remove Windows apps
    pub remove_apps: Vec<String>,
    /// Enable features
    pub enable_features: Vec<String>,
    /// Disable features
    pub disable_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInjection {
    /// Source file path
    pub source: PathBuf,
    /// Destination path within image (relative to root)
    pub destination: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePreparationProgress {
    pub step: String,
    pub progress: u32,
    pub message: String,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImagePreparationStage {
    CopyWim,
    MountImage,
    InjectUnattend,
    InjectAutopilot,
    InjectTaskSequence,
    InjectFiles,
    InjectDrivers,
    RemoveApps,
    EnableFeatures,
    DisableFeatures,
    CommitImage,
    DiscardImage,
    Complete,
}

impl ImagePreparationStage {
    fn step(self) -> &'static str {
        match self {
            Self::CopyWim => "prepare-copy",
            Self::MountImage => "prepare-mount",
            Self::InjectUnattend => "prepare-unattend",
            Self::InjectAutopilot => "prepare-autopilot",
            Self::InjectTaskSequence => "prepare-task-sequence",
            Self::InjectFiles => "prepare-files",
            Self::InjectDrivers => "prepare-drivers",
            Self::RemoveApps => "prepare-remove-apps",
            Self::EnableFeatures => "prepare-enable-features",
            Self::DisableFeatures => "prepare-disable-features",
            Self::CommitImage => "prepare-commit",
            Self::DiscardImage => "prepare-discard",
            Self::Complete => "prepare-complete",
        }
    }

    fn progress(self) -> u32 {
        match self {
            Self::CopyWim => 5,
            Self::MountImage => 15,
            Self::InjectUnattend => 30,
            Self::InjectAutopilot => 40,
            Self::InjectTaskSequence => 50,
            Self::InjectFiles => 60,
            Self::InjectDrivers => 70,
            Self::RemoveApps => 78,
            Self::EnableFeatures => 84,
            Self::DisableFeatures => 88,
            Self::CommitImage | Self::DiscardImage => 95,
            Self::Complete => 100,
        }
    }
}

impl ImagePreparationProgress {
    fn from_stage(stage: ImagePreparationStage, message: impl Into<String>) -> Self {
        Self {
            step: stage.step().to_string(),
            progress: stage.progress(),
            message: message.into(),
        }
    }
}

fn emit_image_preparation_progress<F>(
    progress_callback: &mut F,
    stage: ImagePreparationStage,
    message: impl Into<String>,
) where
    F: FnMut(ImagePreparationProgress),
{
    progress_callback(ImagePreparationProgress::from_stage(stage, message));
}

impl ImagePreparer {
    pub fn new(work_dir: PathBuf) -> BitOSDTResult<Self> {
        let adk_paths = resolve_adk_paths(None, std::env::consts::ARCH);
        Self::new_with_adk(work_dir, adk_paths)
    }

    pub fn new_with_adk(work_dir: PathBuf, adk_paths: Option<AdkPaths>) -> BitOSDTResult<Self> {
        let mount_dir = work_dir.join("mount");
        fs::create_dir_all(&work_dir)?;
        fs::create_dir_all(&mount_dir)?;

        Ok(Self {
            _work_dir: work_dir,
            mount_dir,
            adk_paths,
        })
    }

    /// Prepare a Windows image with all customizations
    pub fn prepare_image(
        &self,
        config: &ImagePrepConfig,
        output_wim: &Path,
    ) -> BitOSDTResult<PathBuf> {
        self.prepare_image_with_progress(config, output_wim, |_| {})
    }

    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    pub fn prepare_image_with_progress<F>(
        &self,
        config: &ImagePrepConfig,
        output_wim: &Path,
        mut progress_callback: F,
    ) -> BitOSDTResult<PathBuf>
    where
        F: FnMut(ImagePreparationProgress),
    {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = &progress_callback;
            return self.prepare_image_linux(config, output_wim);
        }

        #[cfg(target_os = "windows")]
        {
            info!("Starting image preparation: {:?}", config.source_wim);

            if !config.source_wim.exists() {
                return Err(BitOSDTError::NotFound(format!(
                    "Source WIM not found: {:?}",
                    config.source_wim
                )));
            }

            let working_wim = if output_wim != config.source_wim {
                emit_image_preparation_progress(
                    &mut progress_callback,
                    ImagePreparationStage::CopyWim,
                    format!("Copying Windows image to {}...", output_wim.display()),
                );
                info!("Copying WIM to output location...");
                fs::copy(&config.source_wim, output_wim)?;
                output_wim.to_path_buf()
            } else {
                config.source_wim.clone()
            };

            emit_image_preparation_progress(
                &mut progress_callback,
                ImagePreparationStage::MountImage,
                format!(
                    "Mounting Windows image index {} for offline customization...",
                    config.wim_index
                ),
            );
            self.mount_image_with_progress(&working_wim, config.wim_index, |line| {
                emit_image_preparation_progress(
                    &mut progress_callback,
                    ImagePreparationStage::MountImage,
                    format!("Mounting Windows image... {}", line),
                );
            })?;
            let result = self.apply_customizations_with_progress(config, &mut progress_callback);
            let unmount_stage = if result.is_ok() {
                ImagePreparationStage::CommitImage
            } else {
                ImagePreparationStage::DiscardImage
            };
            emit_image_preparation_progress(
                &mut progress_callback,
                unmount_stage,
                if result.is_ok() {
                    "Committing Windows image changes...".to_string()
                } else {
                    "Discarding Windows image changes after an earlier failure...".to_string()
                },
            );
            let unmount_result = self.unmount_image_with_progress(result.is_ok(), |line| {
                emit_image_preparation_progress(
                    &mut progress_callback,
                    unmount_stage,
                    if result.is_ok() {
                        format!("Committing Windows image changes... {}", line)
                    } else {
                        format!("Discarding Windows image changes... {}", line)
                    },
                );
            });

            result?;
            unmount_result?;

            emit_image_preparation_progress(
                &mut progress_callback,
                ImagePreparationStage::Complete,
                format!(
                    "Offline Windows image preparation completed at {}.",
                    output_wim.display()
                ),
            );
            info!("Image preparation complete: {:?}", output_wim);
            Ok(output_wim.to_path_buf())
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn prepare_image_linux(
        &self,
        config: &ImagePrepConfig,
        output_wim: &Path,
    ) -> BitOSDTResult<PathBuf> {
        info!(
            "Starting Linux image preparation with wimlib: {:?}",
            config.source_wim
        );

        if !config.source_wim.exists() {
            return Err(BitOSDTError::NotFound(format!(
                "Source WIM not found: {:?}",
                config.source_wim
            )));
        }

        if !config.driver_paths.is_empty() {
            return Err(BitOSDTError::NotImplemented(
                "Offline driver injection is not supported on Linux builds yet".to_string(),
            ));
        }

        if !config.remove_apps.is_empty()
            || !config.enable_features.is_empty()
            || !config.disable_features.is_empty()
        {
            return Err(BitOSDTError::NotImplemented(
                "Removing apps and toggling Windows features requires Windows DISM today"
                    .to_string(),
            ));
        }

        if self.mount_dir.exists() {
            let _ = fs::remove_dir_all(&self.mount_dir);
        }
        fs::create_dir_all(&self.mount_dir)?;

        let working_wim = if output_wim != config.source_wim {
            fs::copy(&config.source_wim, output_wim)?;
            output_wim.to_path_buf()
        } else {
            config.source_wim.clone()
        };

        let mut noop = |_| {};
        self.apply_customizations_with_progress(config, &mut noop)?;
        apply_wim_updates_from_directory(&working_wim, config.wim_index, &self.mount_dir)?;

        info!("Linux image preparation complete: {:?}", working_wim);
        Ok(working_wim)
    }

    fn apply_customizations_with_progress<F>(
        &self,
        config: &ImagePrepConfig,
        progress_callback: &mut F,
    ) -> BitOSDTResult<()>
    where
        F: FnMut(ImagePreparationProgress),
    {
        // Inject unattend.xml
        if let Some(ref unattend_config) = config.unattend {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::InjectUnattend,
                "Writing unattend.xml into the offline Windows image...",
            );
            self.inject_unattend(unattend_config)?;
        }

        // Inject Autopilot configuration
        if let Some(ref autopilot) = config.autopilot {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::InjectAutopilot,
                "Writing Autopilot configuration into the offline Windows image...",
            );
            self.inject_autopilot(autopilot)?;
        }

        // Inject task sequence scripts
        if let Some(ref task_sequence) = config.task_sequence {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::InjectTaskSequence,
                format!(
                    "Staging task sequence '{}' into the offline image...",
                    task_sequence.name
                ),
            );
            self.inject_task_sequence(task_sequence)?;
        }

        // Inject additional files
        for (index, file) in config.inject_files.iter().enumerate() {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::InjectFiles,
                format!(
                    "Injecting file {} of {} into the offline image: {}",
                    index + 1,
                    config.inject_files.len(),
                    file.destination
                ),
            );
            self.inject_file(&file.source, &file.destination)?;
        }

        // Inject drivers
        for (index, driver_path) in config.driver_paths.iter().enumerate() {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::InjectDrivers,
                format!(
                    "Injecting driver folder {} of {}: {}",
                    index + 1,
                    config.driver_paths.len(),
                    driver_path.display()
                ),
            );
            self.inject_drivers_with_progress(driver_path, |line| {
                emit_image_preparation_progress(
                    progress_callback,
                    ImagePreparationStage::InjectDrivers,
                    format!(
                        "Injecting drivers from {}... {}",
                        driver_path.display(),
                        line
                    ),
                );
            })?;
        }

        // Remove apps
        for app in &config.remove_apps {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::RemoveApps,
                format!("Removing provisioned app {} from the offline image...", app),
            );
            self.remove_provisioned_app(app)?;
        }

        // Enable features
        for feature in &config.enable_features {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::EnableFeatures,
                format!(
                    "Enabling Windows feature {} in the offline image...",
                    feature
                ),
            );
            self.enable_feature(feature)?;
        }

        // Disable features
        for feature in &config.disable_features {
            emit_image_preparation_progress(
                progress_callback,
                ImagePreparationStage::DisableFeatures,
                format!(
                    "Disabling Windows feature {} in the offline image...",
                    feature
                ),
            );
            self.disable_feature(feature)?;
        }

        Ok(())
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    fn mount_image_with_progress<F>(
        &self,
        wim_path: &Path,
        index: u32,
        mut progress_callback: F,
    ) -> BitOSDTResult<()>
    where
        F: FnMut(String),
    {
        info!("Mounting image index {}: {:?}", index, wim_path);

        #[cfg(target_os = "windows")]
        {
            let args = vec![
                "/Mount-Wim".to_string(),
                dism_path_arg("/WimFile", wim_path),
                format!("/Index:{}", index),
                dism_path_arg("/MountDir", &self.mount_dir),
            ];

            let output = run_dism_streaming_with_role(
                &args,
                self.adk_paths.as_ref(),
                "prepare-mount",
                |line| {
                    progress_callback(line);
                },
            )
            .map_err(|e| BitOSDTError::WinPE(format!("Failed to run DISM mount: {}", e)))?;

            if !output.status.success() {
                let dism_exe = resolve_dism_exe(self.adk_paths.as_ref());
                return Err(BitOSDTError::WinPE(format!(
                    "DISM mount failed: {}",
                    format_process_failure(&dism_exe, &args, &output)
                )));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = &progress_callback;
            warn!("WIM mounting requires Windows - creating mock structure");
            fs::create_dir_all(self.mount_dir.join("Windows"))?;
            fs::create_dir_all(self.mount_dir.join("Windows").join("Panther"))?;
            fs::create_dir_all(self.mount_dir.join("Windows").join("Setup").join("Scripts"))?;
        }

        Ok(())
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    fn unmount_image_with_progress<F>(
        &self,
        commit: bool,
        mut progress_callback: F,
    ) -> BitOSDTResult<()>
    where
        F: FnMut(String),
    {
        let action = if commit { "committing" } else { "discarding" };
        info!("Unmounting image ({})", action);

        #[cfg(target_os = "windows")]
        {
            let commit_flag = if commit { "/Commit" } else { "/Discard" };

            let args = vec![
                "/Unmount-Wim".to_string(),
                dism_path_arg("/MountDir", &self.mount_dir),
                commit_flag.to_string(),
            ];

            let output = run_dism_streaming_with_role(
                &args,
                self.adk_paths.as_ref(),
                if commit {
                    "prepare-commit"
                } else {
                    "prepare-discard"
                },
                |line| {
                    progress_callback(line);
                },
            )
            .map_err(|e| BitOSDTError::WinPE(format!("Failed to run DISM unmount: {}", e)))?;

            if !output.status.success() {
                let dism_exe = resolve_dism_exe(self.adk_paths.as_ref());
                return Err(BitOSDTError::WinPE(format!(
                    "DISM unmount failed: {}",
                    format_process_failure(&dism_exe, &args, &output)
                )));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = &progress_callback;
            warn!("WIM unmounting requires Windows - skipping");
        }

        Ok(())
    }

    /// Inject unattend.xml into image
    fn inject_unattend(&self, config: &UnattendConfig) -> BitOSDTResult<()> {
        info!("Injecting unattend.xml");

        let unattend_content = UnattendGenerator::generate(config)?;
        let unattend_path = self
            .mount_dir
            .join("Windows")
            .join("Panther")
            .join("unattend.xml");

        if let Some(parent) = unattend_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&unattend_path, unattend_content)?;
        info!("Unattend.xml written to {:?}", unattend_path);

        Ok(())
    }

    /// Inject Autopilot configuration
    fn inject_autopilot(&self, profile: &AutopilotProfile) -> BitOSDTResult<()> {
        info!("Injecting Autopilot configuration");

        let autopilot_dir = self
            .mount_dir
            .join("Windows")
            .join("Provisioning")
            .join("Autopilot");

        AutopilotGenerator::save_configuration(profile, &autopilot_dir)?;

        Ok(())
    }

    /// Inject task sequence scripts
    fn inject_task_sequence(&self, sequence: &TaskSequence) -> BitOSDTResult<()> {
        info!("Injecting task sequence: {}", sequence.name);

        // Generate task files
        let scripts_dir = self.mount_dir.join("Windows").join("Setup").join("Scripts");
        fs::create_dir_all(&scripts_dir)?;

        // Write task files
        TaskRunner::write_task_files(sequence, &scripts_dir)?;

        // Also inject SetupComplete.cmd to Windows\Setup\Scripts
        // This is picked up automatically by Windows Setup
        let files = TaskRunner::generate_task_files(sequence)?;
        if let Some(setup_complete) = files.get("SetupComplete.cmd") {
            let setup_complete_path = scripts_dir.join("SetupComplete.cmd");
            fs::write(&setup_complete_path, setup_complete)?;
            info!("SetupComplete.cmd written to {:?}", setup_complete_path);
        }

        Ok(())
    }

    /// Inject a file into the image
    fn inject_file(&self, source: &Path, destination: &str) -> BitOSDTResult<()> {
        let dest_path = self
            .mount_dir
            .join(destination.trim_start_matches(['/', '\\']));

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(source, &dest_path)?;
        info!("Injected file: {:?} -> {}", source, destination);

        Ok(())
    }

    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    fn inject_drivers_with_progress<F>(
        &self,
        driver_path: &Path,
        mut progress_callback: F,
    ) -> BitOSDTResult<()>
    where
        F: FnMut(String),
    {
        if !driver_path.exists() {
            warn!("Driver path does not exist: {:?}", driver_path);
            return Ok(());
        }

        info!("Injecting drivers from: {:?}", driver_path);

        #[cfg(target_os = "windows")]
        {
            let args = vec![
                dism_path_arg("/Image", &self.mount_dir),
                "/Add-Driver".to_string(),
                dism_path_arg("/Driver", driver_path),
                "/Recurse".to_string(),
            ];

            let output = run_dism_streaming_with_role(
                &args,
                self.adk_paths.as_ref(),
                "prepare-drivers",
                |line| {
                    progress_callback(line);
                },
            )
            .map_err(|e| BitOSDTError::WinPE(format!("Failed to inject drivers: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Some drivers may have failed: {}", stderr);
                // Continue - some failures are expected for incompatible drivers
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = &progress_callback;
            warn!("Driver injection requires Windows DISM");
        }

        Ok(())
    }

    /// Remove a provisioned app from the image
    fn remove_provisioned_app(&self, app_name: &str) -> BitOSDTResult<()> {
        info!("Removing provisioned app: {}", app_name);

        #[cfg(target_os = "windows")]
        {
            let args = vec![
                dism_path_arg("/Image", &self.mount_dir),
                "/Remove-ProvisionedAppxPackage".to_string(),
                format!("/PackageName:{}", app_name),
            ];

            let output = run_dism(&args, self.adk_paths.as_ref())
                .map_err(|e| BitOSDTError::WinPE(format!("Failed to remove app: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to remove app {}: {}", app_name, stderr);
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("App removal requires Windows DISM");
        }

        Ok(())
    }

    /// Enable a Windows feature
    fn enable_feature(&self, feature_name: &str) -> BitOSDTResult<()> {
        info!("Enabling feature: {}", feature_name);

        #[cfg(target_os = "windows")]
        {
            let args = vec![
                dism_path_arg("/Image", &self.mount_dir),
                "/Enable-Feature".to_string(),
                format!("/FeatureName:{}", feature_name),
                "/All".to_string(),
            ];

            let output = run_dism(&args, self.adk_paths.as_ref())
                .map_err(|e| BitOSDTError::WinPE(format!("Failed to enable feature: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to enable feature {}: {}", feature_name, stderr);
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("Feature enablement requires Windows DISM");
        }

        Ok(())
    }

    /// Disable a Windows feature
    fn disable_feature(&self, feature_name: &str) -> BitOSDTResult<()> {
        info!("Disabling feature: {}", feature_name);

        #[cfg(target_os = "windows")]
        {
            let args = vec![
                dism_path_arg("/Image", &self.mount_dir),
                "/Disable-Feature".to_string(),
                format!("/FeatureName:{}", feature_name),
            ];

            let output = run_dism(&args, self.adk_paths.as_ref())
                .map_err(|e| BitOSDTError::WinPE(format!("Failed to disable feature: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to disable feature {}: {}", feature_name, stderr);
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("Feature disablement requires Windows DISM");
        }

        Ok(())
    }

    /// Get list of provisioned apps in mounted image
    pub fn list_provisioned_apps(&self) -> BitOSDTResult<Vec<String>> {
        #[cfg(target_os = "windows")]
        {
            let args = vec![
                dism_path_arg("/Image", &self.mount_dir),
                "/Get-ProvisionedAppxPackages".to_string(),
            ];

            let output = run_dism(&args, self.adk_paths.as_ref())
                .map_err(|e| BitOSDTError::WinPE(format!("Failed to list apps: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::WinPE(format!("DISM failed: {}", stderr)));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let apps: Vec<String> = stdout
                .lines()
                .filter(|line| line.starts_with("PackageName :"))
                .map(|line| line.replace("PackageName :", "").trim().to_string())
                .collect();

            Ok(apps)
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Return mock data for development
            Ok(vec![
                "Microsoft.BingWeather".to_string(),
                "Microsoft.GetHelp".to_string(),
                "Microsoft.MicrosoftStickyNotes".to_string(),
                "Microsoft.WindowsFeedbackHub".to_string(),
                "Microsoft.Xbox.TCUI".to_string(),
            ])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{emit_image_preparation_progress, ImagePreparationProgress, ImagePreparationStage};

    #[test]
    fn image_preparation_stage_progress_is_monotonic() {
        let stages = [
            ImagePreparationStage::CopyWim,
            ImagePreparationStage::MountImage,
            ImagePreparationStage::InjectUnattend,
            ImagePreparationStage::InjectAutopilot,
            ImagePreparationStage::InjectTaskSequence,
            ImagePreparationStage::InjectFiles,
            ImagePreparationStage::InjectDrivers,
            ImagePreparationStage::RemoveApps,
            ImagePreparationStage::EnableFeatures,
            ImagePreparationStage::DisableFeatures,
            ImagePreparationStage::CommitImage,
            ImagePreparationStage::Complete,
        ];

        for pair in stages.windows(2) {
            assert!(pair[0].progress() <= pair[1].progress());
        }
    }

    #[test]
    fn emit_image_preparation_progress_uses_stage_metadata() {
        let mut emitted: Vec<ImagePreparationProgress> = Vec::new();
        emit_image_preparation_progress(
            &mut |progress| emitted.push(progress),
            ImagePreparationStage::InjectDrivers,
            "Injecting drivers",
        );

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].step, "prepare-drivers");
        assert_eq!(emitted[0].progress, 70);
        assert_eq!(emitted[0].message, "Injecting drivers");
    }
}
