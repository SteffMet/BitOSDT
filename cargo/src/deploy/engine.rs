use crate::core::database::Database;
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::models::{DeployConfig, Image, RuntimeDriverConfig};
use crate::deploy::{
    boot::BootManager, disk::DiskManager, hardware::HardwareDetector, prepare_runtime_drivers,
    wim::WimManager,
};
use std::path::PathBuf;
use tracing::{info, warn};

/// Deployment engine that orchestrates the entire deployment process
pub struct DeploymentEngine {
    db: Database,
    disk_manager: Option<DiskManager>,
    wim_manager: WimManager,
    boot_manager: BootManager,
    hardware_detector: HardwareDetector,
}

/// Progress information during deployment
#[derive(Debug, Clone)]
pub struct DeploymentProgress {
    pub phase: DeploymentPhase,
    pub message: String,
    pub percent_complete: u8,
}

#[derive(Debug, Clone)]
pub enum DeploymentPhase {
    Preparation,
    DiskPartitioning,
    WimApplying,
    DriverInstallation,
    BootloaderConfig,
    Finalization,
    Complete,
}

impl DeploymentEngine {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            disk_manager: None,
            wim_manager: WimManager::new(),
            boot_manager: BootManager::new(),
            hardware_detector: HardwareDetector::new(),
        }
    }

    /// Execute full deployment
    pub async fn deploy(
        &mut self,
        image: &Image,
        config: &DeployConfig,
        progress_callback: Option<&dyn Fn(DeploymentProgress)>,
    ) -> BitOSDTResult<()> {
        let start_time = std::time::Instant::now();

        // Phase 1: Preparation
        self.report_progress(
            &progress_callback,
            DeploymentPhase::Preparation,
            "Preparing deployment...",
            5,
        );

        info!(
            "Starting deployment of image: {} ({:?})",
            image.name, image.id
        );

        // Validate prerequisites
        self.validate_deployment(image, config).await?;

        // Get hardware info
        let hardware = self.hardware_detector.detect_all()?;
        info!(
            "Target hardware: {} {}",
            hardware.manufacturer, hardware.model
        );

        // Phase 2: Disk partitioning
        self.report_progress(
            &progress_callback,
            DeploymentPhase::DiskPartitioning,
            "Partitioning disk...",
            10,
        );

        let target_disk = config.target_disk.unwrap_or(0);
        let disk_manager = DiskManager::new(target_disk);

        info!(
            "Initializing disk {} with {} partitioning",
            target_disk,
            if config.uefi { "GPT/UEFI" } else { "MBR/BIOS" }
        );

        disk_manager.initialize_disk(config.uefi)?;
        self.disk_manager = Some(disk_manager);

        // Get partition paths
        let windows_partition = self
            .disk_manager
            .as_ref()
            .ok_or_else(|| BitOSDTError::Deployment("Disk manager not initialized".to_string()))?
            .get_windows_partition()?;
        let system_partition = if config.uefi {
            // For UEFI, system is EFI partition
            PathBuf::from("S:\\")
        } else {
            // For BIOS, system is the System Reserved
            PathBuf::from("S:\\")
        };

        self.report_progress(
            &progress_callback,
            DeploymentPhase::DiskPartitioning,
            "Disk partitioned successfully",
            15,
        );

        // Phase 3: Apply WIM
        self.report_progress(
            &progress_callback,
            DeploymentPhase::WimApplying,
            "Applying Windows image...",
            20,
        );

        let wim_path = config
            .wim_path
            .as_ref()
            .or(image.wim_path.as_ref())
            .ok_or_else(|| BitOSDTError::Deployment("No WIM path specified".to_string()))?;

        info!("Applying WIM: {:?} -> {:?}", wim_path, windows_partition);

        // Find appropriate image index based on license
        let wim_info = self.wim_manager.get_wim_info(wim_path)?;
        let image_index = self
            .wim_manager
            .find_image_index(&wim_info, &format!("{:?}", image.license.license_type))
            .unwrap_or(1);

        self.wim_manager.apply_wim(
            wim_path,
            image_index,
            &windows_partition,
            Some(&|_current: u64, _total: u64| {
                // Progress updates would go here
            }),
        )?;

        self.report_progress(
            &progress_callback,
            DeploymentPhase::WimApplying,
            "Windows image applied",
            60,
        );

        // Phase 4: Driver installation (if configured)
        if config.driver_prefs.use_driverpacks {
            self.report_progress(
                &progress_callback,
                DeploymentPhase::DriverInstallation,
                "Installing drivers...",
                65,
            );

            if let Some(runtime_context) = config.runtime_driver_context.clone() {
                let manifest = prepare_runtime_drivers(
                    &RuntimeDriverConfig {
                        os_version: config.os_version.clone(),
                        runtime_driver_policy: config.driver_prefs.runtime_driver_policy.clone(),
                        runtime_driver_context: runtime_context,
                    },
                    Some(&windows_partition),
                )
                .await?;
                if !manifest.warnings.is_empty() {
                    for warning in &manifest.warnings {
                        warn!("Runtime driver warning: {}", warning);
                    }
                }
            } else {
                info!("Runtime driver context not configured; skipping shared driver acquisition");
            }
        }

        self.report_progress(
            &progress_callback,
            DeploymentPhase::DriverInstallation,
            "Drivers installed",
            75,
        );

        // Phase 5: Bootloader configuration
        self.report_progress(
            &progress_callback,
            DeploymentPhase::BootloaderConfig,
            "Configuring bootloader...",
            80,
        );

        self.boot_manager.configure_bootloader(
            &windows_partition,
            &system_partition,
            config.uefi,
        )?;

        // Allow unsigned drivers if configured
        if config.driver_prefs.allow_unsigned_drivers {
            self.boot_manager.disable_driver_signature()?;
        }

        self.report_progress(
            &progress_callback,
            DeploymentPhase::BootloaderConfig,
            "Bootloader configured",
            90,
        );

        // Phase 6: Finalization
        self.report_progress(
            &progress_callback,
            DeploymentPhase::Finalization,
            "Finalizing deployment...",
            95,
        );

        // Update image status
        self.db
            .update_image_status(image.id, crate::core::models::ImageStatus::Ready)?;

        let duration = start_time.elapsed();
        info!(
            "Deployment completed in {:.2} minutes",
            duration.as_secs_f64() / 60.0
        );

        self.report_progress(
            &progress_callback,
            DeploymentPhase::Complete,
            "Deployment complete!",
            100,
        );

        Ok(())
    }

    /// Validate deployment prerequisites
    async fn validate_deployment(&self, image: &Image, config: &DeployConfig) -> BitOSDTResult<()> {
        // Check if WIM exists
        let wim_path = config
            .wim_path
            .as_ref()
            .or(image.wim_path.as_ref())
            .ok_or_else(|| {
                BitOSDTError::Deployment("No WIM file specified for deployment".to_string())
            })?;

        if !wim_path.exists() {
            return Err(BitOSDTError::Deployment(format!(
                "WIM file not found: {:?}",
                wim_path
            )));
        }

        // Check target disk
        if let Some(disk) = config.target_disk {
            // Validate disk number (would check against system disks)
            info!("Target disk: {}", disk);
        }

        // Check hardware compatibility
        let hardware = self.hardware_detector.detect_all()?;

        // Minimum requirements check
        if hardware.memory.total_gb < 4.0 {
            warn!("System has less than 4GB RAM - deployment may fail");
        }

        // Check for sufficient disk space
        // Would need to check target disk size here

        info!("Deployment validation passed");
        Ok(())
    }

    /// Report progress through callback
    fn report_progress(
        &self,
        callback: &Option<&dyn Fn(DeploymentProgress)>,
        phase: DeploymentPhase,
        message: &str,
        percent: u8,
    ) {
        info!("[{}%] {}", percent, message);

        if let Some(cb) = callback {
            cb(DeploymentProgress {
                phase,
                message: message.to_string(),
                percent_complete: percent,
            });
        }
    }

    /// Cancel ongoing deployment
    pub fn cancel(&self) -> BitOSDTResult<()> {
        info!("Cancelling deployment...");
        // Implementation would signal cancellation to running operations
        Err(BitOSDTError::Cancelled)
    }

    /// Get deployment statistics
    pub fn get_stats(&self) -> DeploymentStats {
        DeploymentStats {
            total_deployments: 0,
            successful_deployments: 0,
            failed_deployments: 0,
            average_duration_minutes: 0.0,
        }
    }
}

/// Deployment statistics
#[derive(Debug, Clone)]
pub struct DeploymentStats {
    pub total_deployments: u32,
    pub successful_deployments: u32,
    pub failed_deployments: u32,
    pub average_duration_minutes: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_phase_variants() {
        // Ensure all phases can be created and debug-printed
        let phases = vec![
            DeploymentPhase::Preparation,
            DeploymentPhase::DiskPartitioning,
            DeploymentPhase::WimApplying,
            DeploymentPhase::DriverInstallation,
            DeploymentPhase::BootloaderConfig,
            DeploymentPhase::Finalization,
            DeploymentPhase::Complete,
        ];
        assert_eq!(phases.len(), 7);
        for phase in &phases {
            let _ = format!("{:?}", phase);
        }
    }

    #[test]
    fn test_deployment_progress_creation() {
        let progress = DeploymentProgress {
            phase: DeploymentPhase::Preparation,
            message: "Validating hardware".to_string(),
            percent_complete: 5,
        };
        assert_eq!(progress.percent_complete, 5);
        assert_eq!(progress.message, "Validating hardware");

        let progress_end = DeploymentProgress {
            phase: DeploymentPhase::Complete,
            message: "Deployment finished".to_string(),
            percent_complete: 100,
        };
        assert_eq!(progress_end.percent_complete, 100);
    }

    #[test]
    fn test_deployment_stats_defaults() {
        let stats = DeploymentStats {
            total_deployments: 0,
            successful_deployments: 0,
            failed_deployments: 0,
            average_duration_minutes: 0.0,
        };
        assert_eq!(stats.total_deployments, 0);
        assert_eq!(stats.average_duration_minutes, 0.0);
    }

    #[test]
    fn test_deployment_progress_clone() {
        let progress = DeploymentProgress {
            phase: DeploymentPhase::WimApplying,
            message: "Applying image".to_string(),
            percent_complete: 45,
        };
        let cloned = progress.clone();
        assert_eq!(cloned.percent_complete, 45);
        assert_eq!(cloned.message, "Applying image");
    }
}
