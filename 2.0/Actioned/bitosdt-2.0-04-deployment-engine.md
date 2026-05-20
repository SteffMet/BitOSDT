# BitOSDT 2.0 - Deployment Engine

## Overview

The Deployment Engine is the Rust binary that runs inside WinPE and orchestrates the entire Windows deployment process.

## Architecture

```
Deployment Engine (deploy.exe running in WinPE)
│
├── Initialization
│   ├── Parse command-line arguments
│   ├── Load configuration
│   └── Initialize logging
│
├── Phase 1: Hardware Detection
│   ├── Query WMI for system info
│   ├── Detect manufacturer/model
│   └── Determine target disk
│
├── Phase 2: Driver Acquisition
│   ├── Download CloudDrivers (MS Update)
│   ├── Download DriverPack
│   └── Cache locally
│
├── Phase 3: Disk Preparation
│   ├── Clear disk
│   ├── Create partition layout
│   └── Format partitions
│
├── Phase 4: Image Deployment
│   ├── Extract WIM from ESD
│   ├── Apply WIM to disk
│   └── Inject drivers
│
├── Phase 5: System Configuration
│   ├── Configure bootloader
│   ├── Install unattend.xml
│   └── Setup post-deployment tasks
│
└── Phase 6: Finalization
    ├── Cleanup temporary files
    └── Reboot
```

## Entry Point

```rust
// src/deploy/main.rs
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Parse arguments
    let args = DeployArgs::parse();
    
    // Load configuration
    let config = DeploymentConfig::load(&args.config_path)?;
    
    // Create and run deployment engine
    let engine = DeploymentEngine::new(config)?;
    
    match engine.deploy().await {
        Ok(()) => {
            info!("Deployment completed successfully!");
            if args.reboot {
                reboot_system();
            }
            Ok(())
        }
        Err(e) => {
            error!("Deployment failed: {}", e);
            if args.pause_on_error {
                pause_console();
            }
            Err(e)
        }
    }
}

#[derive(Parser)]
struct DeployArgs {
    #[arg(short, long, default_value = "X:\\BitOSDT\\config.json")]
    config_path: PathBuf,
    
    #[arg(short, long)]
    reboot: bool,
    
    #[arg(long)]
    pause_on_error: bool,
}
```

## Core Engine Structure

```rust
pub struct DeploymentEngine {
    config: DeploymentConfig,
    hardware: Option<HardwareInfo>,
    temp_dir: PathBuf,
    progress: ProgressTracker,
}

impl DeploymentEngine {
    pub fn new(config: DeploymentConfig) -> Result<Self> {
        let temp_dir = std::env::temp_dir().join("bitosdt-deploy");
        
        Ok(Self {
            config,
            hardware: None,
            temp_dir,
            progress: ProgressTracker::new(),
        })
    }
    
    pub async fn deploy(&mut self) -> Result<()> {
        // Phase 1: Hardware Detection
        self.hardware = Some(self.detect_hardware().await?);
        
        // Phase 2: Driver Acquisition
        let drivers_dir = self.acquire_drivers().await?;
        
        // Phase 3: Disk Preparation
        let target_disk = self.prepare_disk().await?;
        
        // Phase 4: Image Deployment
        self.deploy_image(&target_disk, &drivers_dir).await?;
        
        // Phase 5: System Configuration
        self.configure_system(&target_disk).await?;
        
        // Phase 6: Finalization
        self.finalize().await?;
        
        Ok(())
    }
}
```

## Phase 1: Hardware Detection

```rust
impl DeploymentEngine {
    async fn detect_hardware(&self,
    ) -> Result<HardwareInfo> {
        info!("Detecting hardware...");
        
        let detector = HardwareDetector::new()?;
        let info = detector.detect_all()?;
        
        info!("Detected: {} {}", info.manufacturer, info.model);
        info!("Product SKU: {}", info.product);
        info!("Architecture: {:?}", info.architecture);
        info!("Form Factor: {:?}", info.form_factor);
        
        Ok(info)
    }
}
```

## Phase 2: Driver Acquisition

```rust
impl DeploymentEngine {
    async fn acquire_drivers(&self,
    ) -> Result<PathBuf> {
        let drivers_dir = self.temp_dir.join("drivers");
        fs::create_dir_all(&drivers_dir).await?;
        
        let hardware = self.hardware.as_ref()
            .ok_or(DeployError::HardwareNotDetected)?;
        
        // CloudDrivers (Microsoft Update)
        if self.config.driver_prefs.use_cloud_drivers {
            info!("Acquiring CloudDrivers...");
            
            let cloud_mgr = CloudDriverManager::new();
            
            for category in &self.config.driver_prefs.driver_categories {
                let category_dir = drivers_dir.join(category.to_lowercase());
                fs::create_dir_all(&category_dir).await?;
                
                cloud_mgr.download_drivers(
                    category,
                    &[],  // Hardware IDs from WMI
                    &category_dir,
                ).await?;
            }
        }
        
        // DriverPack (Manufacturer-specific)
        if self.config.driver_prefs.use_driverpacks {
            info!("Acquiring DriverPack...");
            
            let catalog_mgr = CatalogManager::new();
            let driverpacks = catalog_mgr
                .fetch_driverpack_catalog(&hardware.manufacturer
            ).await?;
            
            if let Some(driverpack) = find_matching_driverpack(
                hardware,
                &driverpacks,
                &self.config.os_version,
            ) {
                info!("Found DriverPack: {}", driverpack.name);
                
                let download_mgr = DriverDownloadManager::new();
                let archive = download_mgr
                    .download_driverpack(driverpack, &drivers_dir)
                    .await?;
                
                // Extract
                let extractor = get_extractor(&driverpack.filename);
                let extract_dir = drivers_dir.join("driverpack");
                extractor.extract(&archive, &extract_dir).await?;
            } else {
                warn!("No matching DriverPack found");
            }
        }
        
        Ok(drivers_dir)
    }
}
```

## Phase 3: Disk Preparation

```rust
impl DeploymentEngine {
    async fn prepare_disk(&self,
    ) -> Result<TargetDisk> {
        info!("Preparing disk...");
        
        let disk_manager = DiskManager::new()?;
        
        // Select target disk
        let target = if let Some(disk_num) = self.config.target_disk {
            disk_manager.get_disk(disk_num)?
        } else {
            disk_manager.select_default_disk()?
        };
        
        info!("Selected disk {}: {}", target.number, target.model);
        
        // Confirm wipe (if interactive)
        if self.config.interactive {
            if !confirm_disk_wipe(&target)? {
                return Err(DeployError::UserCancelled);
            }
        }
        
        // Clear disk
        disk_manager.clear_disk(target.number).await?;
        
        // Create partition layout
        let layout = if self.config.uefi {
            PartitionLayout::gpt_uefi()
        } else {
            PartitionLayout::mbr_bios()
        };
        
        disk_manager.create_partitions(target.number, &layout).await?;
        
        // Format partitions
        let partitions = disk_manager.get_partitions(target.number)?;
        
        for partition in &partitions {
            match partition.partition_type {
                PartitionType::System => {
                    disk_manager.format_partition(
                        partition,
                        FileSystem::Fat32,
                        "System",
                    ).await?;
                }
                PartitionType::Windows => {
                    disk_manager.format_partition(
                        partition,
                        FileSystem::Ntfs,
                        "Windows",
                    ).await?;
                }
                PartitionType::Recovery => {
                    disk_manager.format_partition(
                        partition,
                        FileSystem::Ntfs,
                        "Recovery",
                    ).await?;
                }
                _ => {}
            }
        }
        
        // Assign drive letters
        let assigned = disk_manager.assign_drive_letters(target.number)?;
        
        Ok(TargetDisk {
            disk_number: target.number,
            partitions: assigned,
        })
    }
}

pub struct TargetDisk {
    pub disk_number: u32,
    pub partitions: Vec<AssignedPartition>,
}

pub struct AssignedPartition {
    pub partition_type: PartitionType,
    pub drive_letter: char,
    pub path: PathBuf,
}
```

### Partition Layouts

```rust
pub struct PartitionLayout {
    pub style: PartitionStyle,  // GPT or MBR
    pub partitions: Vec<PartitionSpec>,
}

pub enum PartitionStyle {
    Gpt,
    Mbr,
}

pub struct PartitionSpec {
    pub partition_type: PartitionType,
    pub size_mb: Option<u64>,  // None = rest of disk
    pub is_active: bool,
}

pub enum PartitionType {
    System,      // EFI System Partition (GPT) or System Reserved (MBR)
    Msr,         // Microsoft Reserved (GPT only)
    Windows,     // Main Windows partition
    Recovery,    // Windows Recovery
}

impl PartitionLayout {
    pub fn gpt_uefi() -> Self {
        Self {
            style: PartitionStyle::Gpt,
            partitions: vec![
                PartitionSpec {
                    partition_type: PartitionType::System,
                    size_mb: Some(260),  // 260MB EFI System Partition
                    is_active: true,
                },
                PartitionSpec {
                    partition_type: PartitionType::Msr,
                    size_mb: Some(16),   // 16MB Microsoft Reserved
                    is_active: false,
                },
                PartitionSpec {
                    partition_type: PartitionType::Windows,
                    size_mb: None,       // Remainder of disk
                    is_active: false,
                },
                PartitionSpec {
                    partition_type: PartitionType::Recovery,
                    size_mb: Some(1024), // 1GB Recovery
                    is_active: false,
                },
            ],
        }
    }
    
    pub fn mbr_bios() -> Self {
        Self {
            style: PartitionStyle::Mbr,
            partitions: vec![
                PartitionSpec {
                    partition_type: PartitionType::System,
                    size_mb: Some(350),  // 350MB System Reserved
                    is_active: true,
                },
                PartitionSpec {
                    partition_type: PartitionType::Windows,
                    size_mb: None,       // Remainder of disk
                    is_active: false,
                },
            ],
        }
    }
}
```

## Phase 4: Image Deployment

```rust
impl DeploymentEngine {
    async fn deploy_image(
        &self,
        target: &TargetDisk,
        drivers_dir: &Path,
    ) -> Result<()> {
        info!("Deploying Windows image...");
        
        let windows_partition = target.partitions
            .iter()
            .find(|p| p.partition_type == PartitionType::Windows)
            .ok_or(DeployError::NoWindowsPartition)?;
        
        // Find WIM file
        let wim_path = self.find_wim_file().await?;
        info!("Using WIM: {}", wim_path.display());
        
        // Apply WIM to disk
        let wim_manager = WimManager::new()?;
        
        wim_manager.apply_image(
            &wim_path,
            1,  // Image index
            &windows_partition.path,
            |progress| {
                self.progress.update_wim_apply(progress);
            },
        ).await?;
        
        // Inject drivers
        info!("Injecting drivers...");
        self.inject_drivers(&windows_partition.path, drivers_dir).await?;
        
        Ok(())
    }
    
    async fn find_wim_file(&self,
    ) -> Result<PathBuf> {
        // Look in order:
        // 1. Config-specified WIM path
        // 2. Working directory
        // 3. USB drive (X:\BitOSDT\)
        
        let search_paths = [
            self.config.wim_path.clone(),
            Some(PathBuf::from("X:\\BitOSDT\\install.wim")),
            Some(PathBuf::from("X:\\BitOSDT\\install.esd")),
            Some(PathBuf::from("X:\\sources\\install.wim")),
        ];
        
        for path_opt in &search_paths {
            if let Some(path) = path_opt {
                if path.exists() {
                    return Ok(path.clone());
                }
            }
        }
        
        Err(DeployError::WimNotFound)
    }
    
    async fn inject_drivers(
        &self,
        windows_path: &Path,
        drivers_dir: &Path,
    ) -> Result<()> {
        let installer = OfflineDriverInstaller::new();
        
        // Collect all driver directories
        let mut driver_paths = Vec::new();
        
        for entry in fs::read_dir(drivers_dir).await? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                driver_paths.push(path);
            }
        }
        
        let result = installer.install_drivers(windows_path, &driver_paths)?;
        
        info!(
            "Driver injection complete: {} installed, {} failed",
            result.installed,
            result.failed
        );
        
        Ok(())
    }
}
```

## Phase 5: System Configuration

```rust
impl DeploymentEngine {
    async fn configure_system(
        &self,
        target: &TargetDisk,
    ) -> Result<()> {
        info!("Configuring system...");
        
        let windows_partition = target.partitions
            .iter()
            .find(|p| p.partition_type == PartitionType::Windows)
            .ok_or(DeployError::NoWindowsPartition)?;
        
        // Configure bootloader
        self.configure_bootloader(target, &windows_partition.path).await?;
        
        // Install unattend.xml
        if let Some(unattend) = &self.config.unattend {
            self.install_unattend(&windows_partition.path, unattend).await?;
        }
        
        // Setup post-deployment tasks
        if let Some(tasks) = &self.config.tasks {
            self.setup_tasks(&windows_partition.path, tasks).await?;
        }
        
        // Install Autopilot
        if let Some(autopilot) = &self.config.autopilot {
            self.install_autopilot(&windows_partition.path, autopilot).await?;
        }
        
        Ok(())
    }
    
    async fn configure_bootloader(
        &self,
        target: &TargetDisk,
        windows_path: &Path,
    ) -> Result<()> {
        info!("Configuring bootloader...");
        
        let system_partition = target.partitions
            .iter()
            .find(|p| p.partition_type == PartitionType::System)
            .ok_or(DeployError::NoSystemPartition)?;
        
        // Use BCDBoot
        let boot_mgr = BootManager::new()?;
        
        if self.config.uefi {
            boot_mgr.configure_uefi(
                windows_path,
                &system_partition.path,
            ).await?;
        } else {
            boot_mgr.configure_bios(
                windows_path,
                &system_partition.path,
            ).await?;
        }
        
        Ok(())
    }
    
    async fn install_unattend(
        &self,
        windows_path: &Path,
        unattend_source: &Path,
    ) -> Result<()> {
        info!("Installing unattend.xml...");
        
        let unattend_dest = windows_path
            .join("Windows")
            .join("System32")
            .join("Sysprep")
            .join("unattend.xml");
        
        fs::create_dir_all(unattend_dest.parent().unwrap()).await?;
        fs::copy(unattend_source, unattend_dest).await?;
        
        Ok(())
    }
    
    async fn setup_tasks(
        &self,
        windows_path: &Path,
        tasks: &[Task],
    ) -> Result<()> {
        info!("Setting up {} post-deployment tasks...", tasks.len());
        
        let task_mgr = TaskManager::new(windows_path)?;
        
        for task in tasks {
            task_mgr.install_task(task).await?;
        }
        
        Ok(())
    }
    
    async fn install_autopilot(
        &self,
        windows_path: &Path,
        autopilot: &AutopilotConfig,
    ) -> Result<()> {
        info!("Installing Autopilot configuration...");
        
        let autopilot_mgr = AutopilotManager::new(windows_path)?;
        autopilot_mgr.install_profile(autopilot).await?;
        
        Ok(())
    }
}
```

## Phase 6: Finalization

```rust
impl DeploymentEngine {
    async fn finalize(&self,
    ) -> Result<()> {
        info!("Finalizing deployment...");
        
        // Cleanup temporary files
        if self.config.cleanup {
            info!("Cleaning up temporary files...");
            fs::remove_dir_all(&self.temp_dir).await.ok();
        }
        
        // Save deployment log
        self.save_deployment_log().await?;
        
        info!("Deployment complete!");
        
        Ok(())
    }
    
    async fn save_deployment_log(&self,
    ) -> Result<()> {
        let log_path = self.temp_dir.join("deployment.log");
        let log_data = serde_json::to_string_pretty(&DeploymentLog {
            timestamp: Utc::now(),
            hardware: self.hardware.clone(),
            config: self.config.clone(),
            status: "success".to_string(),
        })?;
        
        fs::write(log_path, log_data).await?;
        
        Ok(())
    }
}
```

## Deployment Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    pub target_disk: Option<u32>,           // None = auto-select
    pub uefi: bool,
    pub interactive: bool,
    pub cleanup: bool,
    
    // Image source
    pub wim_path: Option<PathBuf>,
    pub os_version: String,               // "24H2"
    
    // Driver preferences
    pub driver_prefs: DriverPreferences,
    
    // Configuration files
    pub unattend: Option<PathBuf>,
    pub tasks: Option<Vec<Task>>,
    pub autopilot: Option<AutopilotConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub name: String,
    pub task_type: TaskType,
    pub command: String,
    pub arguments: Vec<String>,
    pub run_once: bool,
    pub requires_reboot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    #[error("Hardware detection failed: {0}")]
    HardwareDetectionFailed(String),
    
    #[error("Hardware not detected")]
    HardwareNotDetected,
    
    #[error("Failed to acquire drivers: {0}")]
    DriverAcquisitionFailed(String),
    
    #[error("Disk preparation failed: {0}")]
    DiskPreparationFailed(String),
    
    #[error("Image deployment failed: {0}")]
    ImageDeploymentFailed(String),
    
    #[error("System configuration failed: {0}")]
    ConfigurationFailed(String),
    
    #[error("WIM file not found")]
    WimNotFound,
    
    #[error("No Windows partition found")]
    NoWindowsPartition,
    
    #[error("No system partition found")]
    NoSystemPartition,
    
    #[error("User cancelled operation")]
    UserCancelled,
    
    #[error("Bootloader configuration failed: {0}")]
    BootloaderConfigFailed(String),
}
```

## Progress Tracking

```rust
pub struct ProgressTracker {
    current_phase: Arc<AtomicUsize>,
    total_phases: usize,
    phase_progress: Arc<AtomicU8>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            current_phase: Arc::new(AtomicUsize::new(0)),
            total_phases: 6,
            phase_progress: Arc::new(AtomicU8::new(0)),
        }
    }
    
    pub fn next_phase(&self, name: &str) {
        let phase = self.current_phase.fetch_add(1, Ordering::SeqCst) + 1;
        info!("[Phase {}/{}] {}", phase, self.total_phases, name);
    }
    
    pub fn update_phase_progress(&self,
        percent: u8,
    ) {
        self.phase_progress.store(percent, Ordering::SeqCst);
    }
    
    pub fn update_wim_apply(&self,
        progress: WimApplyProgress,
    ) {
        let percent = ((progress.bytes_processed as f64 / progress.total_bytes as f64) * 100.0) as u8;
        self.update_phase_progress(percent);
        
        info!(
            "Applying WIM: {}% ({}/{} bytes)",
            percent,
            progress.bytes_processed,
            progress.total_bytes
        );
    }
    
    pub fn get_overall_progress(&self,
    ) -> u8 {
        let phase = self.current_phase.load(Ordering::SeqCst);
        let phase_progress = self.phase_progress.load(Ordering::SeqCst);
        
        let base = (phase * 100 / self.total_phases) as u8;
        let increment = (phase_progress / self.total_phases as u8);
        
        base + increment
    }
}
```
