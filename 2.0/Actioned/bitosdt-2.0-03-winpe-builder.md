# BitOSDT 2.0 - WinPE Builder

## Overview

The WinPE Builder creates customized Windows Preinstallation Environment images with embedded deployment tools.

## WinPE Architecture

```
WinPE Boot Image
│
├── Windows System Files
│   ├── Windows\System32
│   ├── Windows\Boot
│   └── Windows\Setup
│
├── Optional Components (OCs)
│   ├── WinPE-WMI              # WMI support
│   ├── WinPE-NetFx            # .NET Framework
│   ├── WinPE-PowerShell       # PowerShell
│   ├── WinPE-DismCmdlets      # DISM cmdlets
│   ├── WinPE-Scripting        # VBScript/JScript
│   ├── WinPE-WDS-Tools        # WDS tools
│   └── WinPE-EnhancedStorage  # Storage drivers
│
├── BitOSDT Deployment Files
│   ├── deploy.exe             # Main Rust binary
│   ├── deploy.exe.config      # Configuration
│   ├── wimlib\                # WIM tools (optional)
│   └── drivers\               # Injected drivers
│
├── Configuration
│   ├── unattend.xml           # Windows unattended
│   ├── bitosdt.json           # Deployment config
│   └── tasks\                 # Post-deploy tasks
│
└── Boot Configuration
    ├── startnet.cmd           # Startup script
    └── boot.sdi               # Boot data
```

## Prerequisites

### Windows ADK Components

Required ADK installation:
```
C:\Program Files (x86)\Windows Kits\10\
├── Assessment and Deployment Kit\
│   ├── Deployment Tools\
│   │   └── DandISetEnv.bat       # Environment setup
│   ├── Windows Preinstallation Environment\
│   │   └── amd64\                # WinPE base files
│   │       ├── WinPE_OCs\        # Optional components
│   │       ├── media\            # Boot media files
│   │       └── en-us\            # Language files
│   └── Oscdimg\                  # ISO creation tool
└──                               
```

### Required Files

```rust
pub struct AdkPaths {
    pub adk_root: PathBuf,
    pub winpe_root: PathBuf,
    pub winpe_oc_root: PathBuf,
    pub dism_exe: PathBuf,
    pub oscdimg_exe: PathBuf,
    pub copype_ps1: PathBuf,
    pub makewinpemedia_ps1: PathBuf,
}

impl AdkPaths {
    pub fn new() -> Result<Self> {
        let adk_root = Self::find_adk_root()?;
        
        Ok(Self {
            winpe_root: adk_root.join("Windows Preinstallation Environment"),
            winpe_oc_root: adk_root.join("Windows Preinstallation Environment").join("amd64").join("WinPE_OCs"),
            dism_exe: PathBuf::from("dism.exe"),
            oscdimg_exe: adk_root.join("Oscdimg").join("oscdimg.exe"),
            copype_ps1: adk_root.join("Deployment Tools").join("copype.ps1"),
            makewinpemedia_ps1: adk_root.join("Deployment Tools").join("MakeWinPEMedia.ps1"),
            adk_root,
        })
    }
    
    fn find_adk_root() -> Result<PathBuf> {
        // Check registry
        // HKLM\Software\Microsoft\Windows Kits\Installed Roots
        // KitsRoot10 = C:\Program Files (x86)\Windows Kits\10\
        
        // Or check standard paths
        let possible_paths = [
            r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit",
            r"C:\Program Files\Windows Kits\10\Assessment and Deployment Kit",
        ];
        
        for path in &possible_paths {
            let pb = PathBuf::from(path);
            if pb.exists() {
                return Ok(pb);
            }
        }
        
        Err(WinPEError::AdkNotFound)
    }
}
```

## WinPE Build Process

### Step-by-Step Flow

```rust
pub struct WinPEBuilder {
    adk_paths: AdkPaths,
    temp_dir: PathBuf,
    progress: Arc<ProgressTracker>,
}

impl WinPEBuilder {
    pub async fn build(
        &self,
        config: &WinPEConfig,
    ) -> Result<PathBuf> {
        // 1. Create working directory
        let work_dir = self.create_working_directory().await?;
        
        // 2. Copy WinPE base
        let wim_path = self.copy_winpe_base(&work_dir).await?;
        
        // 3. Mount WIM
        let mount_dir = work_dir.join("mount");
        self.mount_wim(&wim_path, &mount_dir).await?;
        
        // 4. Add optional components
        self.add_optional_components(&mount_dir, &config.components).await?;
        
        // 5. Add drivers
        if let Some(drivers) = &config.drivers {
            self.add_drivers(&mount_dir, drivers).await?;
        }
        
        // 6. Copy BitOSDT files
        self.copy_bitosdt_files(&mount_dir, &config.bitosdt_files).await?;
        
        // 7. Configure startup
        self.configure_startup(&mount_dir, &config.startup_config).await?;
        
        // 8. Unmount and commit
        self.unmount_wim(&mount_dir, true).await?;
        
        // 9. Create bootable media
        let output_path = match config.output_format {
            OutputFormat::Wim => wim_path,
            OutputFormat::Iso => self.create_iso(&work_dir, &wim_path).await?,
            OutputFormat::Usb { drive } => {
                self.create_usb_media(&drive, &work_dir).await?;
                work_dir
            }
        };
        
        Ok(output_path)
    }
}
```

## Implementation Details

### 1. Copy WinPE Base

```rust
async fn copy_winpe_base(
    &self,
    work_dir: &Path,
) -> Result<PathBuf> {
    let source_wim = self.adk_paths.winpe_root
        .join("amd64")
        .join("en-us")
        .join("winpe.wim");
    
    let dest_wim = work_dir.join("winpe.wim");
    
    fs::copy(&source_wim, &dest_wim).await
        .map_err(|e| WinPEError::CopyFailed(e.to_string()))?;
    
    Ok(dest_wim)
}
```

### 2. Mount WIM

```rust
async fn mount_wim(
    &self,
    wim_path: &Path,
    mount_dir: &Path,
) -> Result<()> {
    fs::create_dir_all(mount_dir).await?;
    
    let output = Command::new(&self.adk_paths.dism_exe)
        .args([
            "/Mount-Wim",
            &format!("/WimFile:{}", wim_path.display()),
            "/Index:1",
            &format!("/MountDir:{}", mount_dir.display()),
        ])
        .output()
        .await?;
    
    if !output.status.success() {
        return Err(WinPEError::MountFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    
    Ok(())
}
```

### 3. Add Optional Components

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WinPEComponent {
    Wmi,                    // WinPE-WMI (required for hardware detection)
    NetFx,                  // WinPE-NetFx (.NET Framework)
    PowerShell,             // WinPE-PowerShell (recommended)
    DismCmdlets,            // WinPE-DismCmdlets
    WdsTools,               // WinPE-WDS-Tools
    EnhancedStorage,        // WinPE-EnhancedStorage
    SecureStartup,          // WinPE-SecureStartup (BitLocker support)
    PlatformId,             // WinPE-PlatformId
    StorageWmi,             // WinPE-StorageWMI (storage cmdlets)
    // NOTE: Scripting (VBScript/JScript) is DEPRECATED in Windows 11 24H2+
    // It is now a Feature on Demand and not included by default.
    // Use PowerShell for all scripting needs.
    #[deprecated(note = "VBScript deprecated in 24H2+. Use PowerShell instead.")]
    Scripting,              // WinPE-Scripting (legacy - avoid)
}

impl WinPEComponent {
    pub fn package_name(&self) -> String {
        match self {
            Self::Wmi => "WinPE-WMI",
            Self::NetFx => "WinPE-NetFx",
            Self::PowerShell => "WinPE-PowerShell",
            Self::DismCmdlets => "WinPE-DismCmdlets",
            #[allow(deprecated)]
            Self::Scripting => "WinPE-Scripting",
            Self::WdsTools => "WinPE-WDS-Tools",
            Self::EnhancedStorage => "WinPE-EnhancedStorage",
            Self::SecureStartup => "WinPE-SecureStartup",
            Self::PlatformId => "WinPE-PlatformId",
            Self::StorageWmi => "WinPE-StorageWMI",
        }
        .to_string()
    }

    pub fn dependencies(&self) -> Vec<WinPEComponent> {
        match self {
            Self::PowerShell => vec![Self::Wmi, Self::NetFx],
            Self::DismCmdlets => vec![Self::Wmi],
            Self::StorageWmi => vec![Self::Wmi],
            _ => vec![],
        }
    }
}

async fn add_optional_components(
    &self,
    mount_dir: &Path,
    components: &[WinPEComponent],
) -> Result<()> {
    // Resolve dependencies
    let mut resolved = Vec::new();
    for comp in components {
        Self::resolve_dependencies(comp, &mut resolved);
    }

    for component in resolved {
        let package_path = self.adk_paths.winpe_oc_root
            .join(format!("{}.cab", component.package_name()));

        info!("Adding component: {}", component.package_name());

        // Note: DISM requires /Flag:Value as a single argument
        let output = Command::new(&self.adk_paths.dism_exe)
            .args([
                format!("/Image:{}", mount_dir.display()),
                "/Add-Package".to_string(),
                format!("/PackagePath:{}", package_path.display()),
            ])
            .output()
            .await?;

        if !output.status.success() {
            warn!(
                "Failed to add component {}: {}",
                component.package_name(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    Ok(())
}

fn resolve_dependencies(
    component: &WinPEComponent,
    resolved: &mut Vec<WinPEComponent>,
) {
    if !resolved.contains(component) {
        for dep in component.dependencies() {
            Self::resolve_dependencies(&dep, resolved);
        }
        resolved.push(*component);
    }
}
```

### 4. Add Drivers

```rust
async fn add_drivers(
    &self,
    mount_dir: &Path,
    drivers: &[PathBuf],
) -> Result<()> {
    for driver_path in drivers {
        info!("Adding drivers from: {}", driver_path.display());

        // Note: DISM requires /Flag:Value as a single argument
        let output = Command::new(&self.adk_paths.dism_exe)
            .args([
                format!("/Image:{}", mount_dir.display()),
                "/Add-Driver".to_string(),
                format!("/Driver:{}", driver_path.display()),
                "/Recurse".to_string(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            warn!(
                "Some drivers failed to add: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    Ok(())
}
```

### 5. Copy BitOSDT Files

```rust
async fn copy_bitosdt_files(
    &self,
    mount_dir: &Path,
    files: &BitOSDTFiles,
) -> Result<()> {
    let bitosdt_dir = mount_dir.join("BitOSDT");
    fs::create_dir_all(&bitosdt_dir).await?;
    
    // Copy deployment binary
    fs::copy(
        &files.deploy_binary,
        bitosdt_dir.join("deploy.exe")
    ).await?;
    
    // Copy configuration
    if let Some(config) = &files.config {
        fs::copy(config, bitosdt_dir.join("config.json")).await?;
    }
    
    // Copy drivers cache
    if let Some(drivers) = &files.drivers_cache {
        let drivers_dir = bitosdt_dir.join("drivers");
        Self::copy_dir_all(drivers, &drivers_dir).await?;
    }
    
    // Copy tasks
    if let Some(tasks) = &files.tasks {
        let tasks_dir = bitosdt_dir.join("tasks");
        Self::copy_dir_all(tasks, &tasks_dir).await?;
    }
    
    // Copy unattend.xml
    if let Some(unattend) = &files.unattend {
        fs::copy(unattend, mount_dir.join("Windows").join("System32").join("sysprep").join("unattend.xml")).await?;
    }
    
    Ok(())
}
```

### 6. Configure Startup

```rust
async fn configure_startup(
    &self,
    mount_dir: &Path,
    config: &StartupConfig,
) -> Result<()> {
    let startnet_path = mount_dir
        .join("Windows").join("System32")
        .join("startnet.cmd");
    
    let startup_script = format!(r#"@echo off
echo Starting BitOSDT Deployment...
wpeinit

{}

:: Start BitOSDT deployment
X:\BitOSDT\deploy.exe {}

{}
"#,
        if config.wait_for_network { 
            "echo Waiting for network...\n:waitloop\nping -n 1 8.8.8.8 > nul 2>&1\nif errorlevel 1 goto waitloop\necho Network connected." 
        } else { "" },
        config.deploy_args.as_deref().unwrap_or(""),
        if config.pause_on_exit { 
            "\necho Deployment completed.\npause" 
        } else { "" }
    );
    
    fs::write(startnet_path, startup_script).await?;
    
    Ok(())
}
```

### 7. Unmount WIM

```rust
async fn unmount_wim(
    &self,
    mount_dir: &Path,
    commit: bool,
) -> Result<()> {
    let mut args = vec![
        "/Unmount-Wim".to_string(),
        format!("/MountDir:{}", mount_dir.display()),
    ];
    
    if commit {
        args.push("/Commit".to_string());
    } else {
        args.push("/Discard".to_string());
    }
    
    let output = Command::new(&self.adk_paths.dism_exe)
        .args(&args)
        .output()
        .await?;
    
    if !output.status.success() {
        return Err(WinPEError::UnmountFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    
    Ok(())
}
```

### 8. Create ISO

```rust
async fn create_iso(
    &self,
    work_dir: &Path,
    wim_path: &Path,
) -> Result<PathBuf> {
    let iso_path = work_dir.join("BitOSDT.iso");

    // Prepare ISO contents
    let iso_root = work_dir.join("iso");
    fs::create_dir_all(&iso_root).await?;

    // Copy boot files
    let boot_dir = iso_root.join("boot");
    fs::create_dir_all(&boot_dir).await?;

    // Copy WinPE media files
    let media_src = self.adk_paths.winpe_root.join("amd64").join("media");
    Self::copy_dir_all(&media_src, &iso_root).await?;

    // Copy customized WIM
    fs::copy(
        wim_path,
        iso_root.join("sources").join("boot.wim")
    ).await?;

    // Create UEFI-bootable ISO using oscdimg
    // -m = ignore max size limit
    // -o = optimize storage (single instance files)
    // -u2 = UDF file system
    // -udfver102 = UDF version 1.02
    // -bootdata = multi-boot configuration for BIOS and UEFI
    let efisys_path = iso_root.join("efi").join("microsoft").join("boot").join("efisys.bin");
    let etfsboot_path = iso_root.join("boot").join("etfsboot.com");

    let output = Command::new(&self.adk_paths.oscdimg_exe)
        .args([
            "-m".to_string(),
            "-o".to_string(),
            "-u2".to_string(),
            "-udfver102".to_string(),
            format!("-bootdata:2#p0,e,b{}#pEF,e,b{}",
                etfsboot_path.display(),
                efisys_path.display()
            ),
            iso_root.to_string_lossy().to_string(),
            iso_path.to_string_lossy().to_string(),
        ])
        .output()
        .await?;

    if !output.status.success() {
        return Err(WinPEError::IsoCreationFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }

    Ok(iso_path)
}
```

## Configuration Structures

```rust
#[derive(Debug, Clone)]
pub struct WinPEConfig {
    pub architecture: Architecture,        // x64, ARM64
    pub components: Vec<WinPEComponent>,   // Optional components to include
    pub drivers: Option<Vec<PathBuf>>,    // Additional drivers to inject
    pub bitosdt_files: BitOSDTFiles,       // BitOSDT deployment files
    pub startup_config: StartupConfig,     // Startup configuration
    pub output_format: OutputFormat,       // WIM, ISO, or USB
}

#[derive(Debug, Clone)]
pub struct BitOSDTFiles {
    pub deploy_binary: PathBuf,            // Path to deploy.exe
    pub config: Option<PathBuf>,           // deploy.exe.config
    pub drivers_cache: Option<PathBuf>,    // Pre-downloaded drivers
    pub tasks: Option<PathBuf>,            // Task definitions
    pub unattend: Option<PathBuf>,         // unattend.xml
}

#[derive(Debug, Clone)]
pub struct StartupConfig {
    pub wait_for_network: bool,            // Wait for network before deployment
    pub deploy_args: Option<String>,       // Arguments to pass to deploy.exe
    pub pause_on_exit: bool,               // Pause window after deployment
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Wim,                                   // Just the WIM file
    Iso,                                   // Bootable ISO
    Usb { drive: String },                // USB drive letter
}
```

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum WinPEError {
    #[error("Windows ADK not found. Please install the Windows ADK.")]
    AdkNotFound,
    
    #[error("Failed to copy WinPE base: {0}")]
    CopyFailed(String),
    
    #[error("Failed to mount WIM: {0}")]
    MountFailed(String),
    
    #[error("Failed to unmount WIM: {0}")]
    UnmountFailed(String),
    
    #[error("Failed to add component: {0}")]
    ComponentAddFailed(String),
    
    #[error("ISO creation failed: {0}")]
    IsoCreationFailed(String),
    
    #[error("USB media creation failed: {0}")]
    UsbCreationFailed(String),
    
    #[error("Insufficient disk space: {0} required, {1} available")]
    InsufficientSpace(u64, u64),
}
```

## Default Configuration

```rust
impl Default for WinPEConfig {
    fn default() -> Self {
        Self {
            architecture: Architecture::X64,
            components: vec![
                WinPEComponent::Wmi,
                WinPEComponent::NetFx,
                WinPEComponent::PowerShell,
                WinPEComponent::DismCmdlets,
                WinPEComponent::StorageWmi,
                // NOTE: Scripting (VBScript) intentionally excluded
                // It is deprecated in Windows 11 24H2+ and adds unnecessary size
            ],
            drivers: None,
            bitosdt_files: BitOSDTFiles::default(),
            startup_config: StartupConfig::default(),
            output_format: OutputFormat::Iso,
        }
    }
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            wait_for_network: true,
            deploy_args: None,
            pause_on_exit: false,
        }
    }
}
```

## Cross-Platform Considerations

### Windows (Primary)
- Full ADK integration
- Native DISM operations
- oscdimg for ISO creation

### Linux (Future)
- Use wimlib instead of DISM
- Use xorriso or mkisofs for ISO
- No native WinPE creation (requires Windows)
- Could create Linux-based boot environment instead

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_component_dependencies() {
        let ps_deps = WinPEComponent::PowerShell.dependencies();
        assert!(ps_deps.contains(&WinPEComponent::Wmi));
        assert!(ps_deps.contains(&WinPEComponent::NetFx));
    }
    
    #[test]
    fn test_adk_paths_detection() {
        // Skip if ADK not installed
        if let Ok(paths) = AdkPaths::new() {
            assert!(paths.winpe_root.exists());
            assert!(paths.oscdimg_exe.exists());
        }
    }
}
```
