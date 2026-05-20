# BitOSDT 2.0 - Complete Image/ISO Creation System

## Executive Summary

This document provides the complete implementation plan for the BitOSDT 2.0 image and ISO creation system. The system supports two deployment modes:

1. **Full ISO** - Self-contained bootable ISO with embedded Windows image, drivers, and configurations
2. **Lightweight ISO** - Minimal WinPE boot media that downloads Windows over network during deployment

Both modes support automatic OOBE skip, application installation (winget/chocolatey/custom), Autopilot configuration, Windows Update, user creation, and domain join - all working 100% out-of-the-box.

---

## Table of Contents

1. [Current Implementation Status](#1-current-implementation-status)
2. [Feature Specifications](#2-feature-specifications)
3. [Architecture Overview](#3-architecture-overview)
4. [Module Implementation Details](#4-module-implementation-details)
5. [UI Specification](#5-ui-specification)
6. [Data Models](#6-data-models)
7. [Testing Plan](#7-testing-plan)
8. [Implementation Checklist](#8-implementation-checklist)

---

## 1. Current Implementation Status

### ✅ Already Implemented

| Component | File | Status |
|-----------|------|--------|
| CLI Framework | `cargo/src/main.rs` | Complete |
| Data Models | `cargo/src/core/models.rs` | Partial - needs extension |
| Database | `cargo/src/core/database.rs` | Complete |
| Configuration | `cargo/src/core/config.rs` | Complete |
| Hardware Detection | `cargo/src/deploy/hardware.rs` | Complete |
| Driver Matching | `cargo/src/catalog/matcher.rs` | Complete |
| Catalog Sync | `cargo/src/catalog/sync_service.rs` | Complete |
| WinPE Builder Base | `cargo/src/build/winpe_builder.rs` | Partial |
| ISO Creator Base | `cargo/src/build/iso_creator.rs` | Partial |
| USB Writer | `cargo/src/build/usb_writer.rs` | Partial |
| Deployment Engine | `cargo/src/deploy/engine.rs` | Partial |
| WIM Operations | `cargo/src/deploy/wim.rs` | Partial |
| Boot Manager | `cargo/src/deploy/boot.rs` | Complete |

### ❌ Not Yet Implemented

| Component | Priority | Complexity |
|-----------|----------|------------|
| ESD Download Service | P0 | Medium |
| ESD→WIM Conversion | P0 | Low |
| Unattend.xml Generator | P0 | Medium |
| Application Installer System | P1 | High |
| Autopilot Configuration | P1 | Medium |
| Post-Install Task System | P0 | High |
| Full ISO Builder | P0 | Medium |
| Lightweight ISO Builder | P1 | High |
| React Dashboard UI | P0 | High |
| 6-Step Creation Wizard | P0 | High |
| Tauri IPC Commands | P1 | Medium |

### 🔧 Needs Enhancement

| Component | Current State | Required Enhancement |
|-----------|--------------|---------------------|
| `image_preparer.rs` | Empty struct | Full WIM preparation pipeline |
| `winpe_builder.rs` | Basic copype | startnet.cmd customization, BitOSDT embedding |
| `iso_creator.rs` | Basic oscdimg | UEFI/BIOS hybrid boot, EFI system partition |
| `models.rs` | Basic Image struct | ImageConfiguration, AppInstallTask, DomainJoinConfig |

---

## 2. Feature Specifications

### 2.1 Create New OEM ISO (Microsoft Windows Download)

**Purpose:** Download official Windows ESDs from Microsoft CDN and convert to bootable ISO

**User Flow:**
```
1. Select Windows Version (10/11/Server)
2. Select Build (24H2, 23H2, 22H2, etc.)
3. Select Edition (Home, Pro, Enterprise, Education)
4. Select Language (en-US, en-GB, de-DE, etc.)
5. Select Architecture (x64, ARM64)
6. Download ESD from Microsoft CDN
7. Convert ESD to WIM
8. Generate bootable ISO
```

**Technical Requirements:**
- [ ] Fetch Windows ESD catalog from OSDCloud or Microsoft
- [ ] Download ESD with progress tracking and resume support
- [ ] Validate ESD hash (SHA256)
- [ ] Convert ESD to WIM using DISM `/Export-Image`
- [ ] Create bootable ISO with UEFI/BIOS support
- [ ] Support cancellation during download/conversion

**Microsoft CDN URL Pattern:**
```
https://software-static.download.prss.microsoft.com/dbazure/988969d5-f34g-4e03-ac9d-1f9786c66749/[ESD_FILENAME]
```

---

### 2.2 Create New Image (Custom Configuration)

**Purpose:** Create a fully configured Windows deployment image with customizations

**Configuration Options:**

#### A. Windows Version & Build
- [ ] Windows 10 (22H2, 21H2)
- [ ] Windows 11 (24H2, 23H2, 22H2)
- [ ] Windows Server 2022/2025
- [ ] Architecture: x64, ARM64
- [ ] Language selection (200+ languages)

#### B. Edition & Licensing
- [ ] Home / Pro / Enterprise / Education / LTSC
- [ ] Retail / Volume / OEM activation
- [ ] Product key input (optional)

#### C. Skip OOBE Configuration
- [ ] Skip region selection (auto-detect or specify)
- [ ] Skip keyboard layout (auto-detect or specify)
- [ ] Skip privacy settings (accept all)
- [ ] Skip network setup (offline account)
- [ ] Skip Microsoft account requirement
- [ ] Skip Cortana/assistant setup
- [ ] Skip license agreement (auto-accept)
- [ ] Computer name template (e.g., `PC-%SERIAL%`, `DESKTOP-%RANDOM%`)

#### D. Install Applications
**Winget Packages:**
- [ ] Package ID input (e.g., `Microsoft.VisualStudioCode`)
- [ ] Multiple packages support
- [ ] Version pinning (optional)
- [ ] Silent install flags

**Chocolatey Packages:**
- [ ] Package name input (e.g., `googlechrome`)
- [ ] Multiple packages support
- [ ] Custom source/repository

**Custom Installers:**
- [ ] EXE path/URL with silent switches
- [ ] MSI path/URL with `/qn /norestart`
- [ ] Pre/post install scripts (PowerShell)

#### E. Autopilot Configure
- [ ] Tenant ID input
- [ ] Azure AD App ID
- [ ] Autopilot profile JSON upload/generation
- [ ] Hardware hash collection
- [ ] Group tag assignment
- [ ] Assigned user (optional)

#### F. Update Image (Windows Update)
- [ ] Enable/disable Windows Update post-install
- [ ] Specific KB selection (optional)
- [ ] Driver updates inclusion
- [ ] Reboot behavior configuration
- [ ] Update timeout setting

#### G. Create Default User
- [ ] Username (required)
- [ ] Password (required, securely stored)
- [ ] Account type: Administrator / Standard User
- [ ] Auto-login enable/disable
- [ ] Password never expires option
- [ ] Require password change at first login

#### H. Join Domain
- [ ] Domain name/URL (e.g., `corp.contoso.com`)
- [ ] Username (domain admin)
- [ ] Password (securely stored)
- [ ] OU path (optional, e.g., `OU=Computers,DC=corp,DC=contoso,DC=com`)
- [ ] Computer name pattern
- [ ] Fallback to workgroup if join fails

#### I. Additional Options (Suggested)
- [ ] Regional settings (timezone, currency, date format)
- [ ] Power settings (never sleep, high performance)
- [ ] Remote Desktop enable/disable
- [ ] BitLocker enable/disable
- [ ] Firewall configuration
- [ ] Network profile (Private/Public)
- [ ] Windows features enable/disable
- [ ] Custom registry modifications
- [ ] Custom scripts (PowerShell/batch)
- [ ] Wallpaper/branding customization

---

### 2.3 Full ISO Output

**Purpose:** Create self-contained bootable ISO that works 100% offline

**Contents:**
```
BitOSDT-Win11-Pro-24H2.iso/
├── boot/
│   ├── bcd
│   ├── boot.sdi
│   └── etfsboot.com
├── efi/
│   ├── boot/
│   │   └── bootx64.efi
│   └── microsoft/
│       └── boot/
│           ├── bcd
│           └── efisys.bin
├── sources/
│   ├── boot.wim          # WinPE with BitOSDT
│   └── install.wim       # Windows image
├── bitosdt/
│   ├── bitosdt.exe       # Deployment binary
│   ├── config.json       # Image configuration
│   ├── unattend.xml      # OOBE configuration
│   ├── autopilot.json    # Autopilot profile (if enabled)
│   ├── drivers/          # Pre-cached drivers
│   └── tasks/
│       ├── SetupComplete.cmd
│       ├── install-apps.ps1
│       ├── join-domain.ps1
│       ├── create-user.ps1
│       └── run-updates.ps1
├── bootmgr
└── bootmgr.efi
```

**Boot Sequence:**
1. BIOS/UEFI boots from ISO
2. WinPE loads with BitOSDT pre-configured
3. startnet.cmd launches `bitosdt.exe deploy --auto`
4. BitOSDT partitions disk, applies WIM, injects drivers
5. Configures bootloader, copies scripts to target
6. Reboots to Windows
7. SetupComplete.cmd runs post-install tasks
8. Windows Update runs (if enabled)
9. Applications install
10. Domain join executes (if configured)
11. System reboots to login screen

---

### 2.4 Lightweight ISO (Network Download)

**Purpose:** Minimal boot media (~500MB) that downloads Windows during deployment

**Use Cases:**
- Deploy many machines without large USB drives
- Always use latest Windows version from source
- Centralized image management
- Reduced storage requirements

**Architecture:**
```
BitOSDT-Lightweight.iso/
├── boot/                 # Standard boot files
├── efi/                  # UEFI boot files
├── sources/
│   └── boot.wim          # WinPE with BitOSDT + network stack
├── bitosdt/
│   ├── bitosdt.exe       # Deployment binary
│   ├── config.json       # Download sources + image config
│   ├── unattend.xml      # OOBE configuration
│   ├── autopilot.json    # Autopilot profile (embedded)
│   └── tasks/            # Post-install scripts (embedded)
├── bootmgr
└── bootmgr.efi
```

**Download Source Options:**
1. **Microsoft CDN** - Download directly from Microsoft (requires internet)
2. **Local HTTP Server** - Download from `http://192.168.1.100/images/`
3. **BitOSDT Server** - Download from centralized BitOSDT instance
4. **SMB Share** - Download from `\\server\images\`
5. **Custom URL** - Any accessible HTTP/HTTPS endpoint

**Network Boot Flow:**
1. WinPE boots and loads network drivers
2. Acquires IP via DHCP (or static config)
3. BitOSDT reads config.json for download source
4. Downloads install.wim from configured source
5. (Optional) Downloads DriverPack from configured source
6. Proceeds with standard deployment
7. Post-install scripts run from embedded copy

**Configuration for Lightweight:**
```json
{
  "deployment_mode": "lightweight",
  "download_source": {
    "type": "http",
    "base_url": "http://192.168.1.100/bitosdt/",
    "wim_path": "images/win11-pro-24h2.wim",
    "drivers_path": "drivers/",
    "verify_hash": true,
    "timeout_seconds": 3600
  },
  "fallback_sources": [
    {
      "type": "microsoft_cdn",
      "esd_id": "win11-24h2-x64-en-us"
    }
  ],
  "network_config": {
    "dhcp": true,
    "wifi_ssid": "DeploymentNetwork",
    "wifi_password": "encrypted:..."
  }
}
```

---

## 3. Architecture Overview

### 3.1 Module Dependency Graph

```
┌─────────────────────────────────────────────────────────────────────┐
│                           UI Layer                                   │
│  ┌─────────────┐  ┌─────────────────┐  ┌──────────────────────────┐│
│  │  Dashboard  │  │  Image Wizard   │  │  Progress/Status View   ││
│  └─────────────┘  └─────────────────┘  └──────────────────────────┘│
├─────────────────────────────────────────────────────────────────────┤
│                        Tauri IPC Layer                               │
│                    (ui/commands.rs)                                  │
├─────────────────────────────────────────────────────────────────────┤
│                      Service Layer                                   │
│  ┌────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │
│  │ ImageService   │  │ DownloadService │  │ DeploymentService   │  │
│  └────────────────┘  └─────────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│                       Core Modules                                   │
│  ┌─────────────┐  ┌─────────────────┐  ┌─────────────────────────┐ │
│  │ download/   │  │ config/         │  │ tasks/                  │ │
│  │ ├─esd.rs    │  │ ├─unattend.rs   │  │ ├─app_installer.rs      │ │
│  │ └─progress.rs│  │ ├─autopilot.rs │  │ ├─domain_join.rs        │ │
│  └─────────────┘  │ └─registry.rs   │  │ ├─user_creator.rs       │ │
│                   └─────────────────┘  │ └─windows_update.rs     │ │
│                                         └─────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│                       Build Modules                                  │
│  ┌───────────────────┐  ┌───────────────────┐  ┌─────────────────┐ │
│  │ image_preparer.rs │  │ winpe_builder.rs  │  │ iso_creator.rs  │ │
│  │ (WIM preparation) │  │ (WinPE customize) │  │ (ISO generation)│ │
│  └───────────────────┘  └───────────────────┘  └─────────────────┘ │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │               lightweight_builder.rs                           │ │
│  │               (Network-boot ISO generation)                    │ │
│  └───────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 New File Structure

```
cargo/src/
├── download/                    # NEW: Download management
│   ├── mod.rs
│   ├── esd_downloader.rs       # Microsoft ESD download
│   ├── progress.rs             # Download progress tracking
│   └── hash_validator.rs       # SHA256 verification
│
├── config/                      # NEW: Configuration generators
│   ├── mod.rs
│   ├── unattend_generator.rs   # Unattend.xml builder
│   ├── autopilot_generator.rs  # Autopilot JSON builder
│   └── registry_generator.rs   # Registry tweaks
│
├── tasks/                       # NEW: Post-install task system
│   ├── mod.rs
│   ├── task_runner.rs          # Task orchestration
│   ├── app_installer.rs        # Winget/Choco/Custom
│   ├── domain_join.rs          # Domain join logic
│   ├── user_creator.rs         # User account creation
│   ├── windows_update.rs       # WU integration
│   └── script_generator.rs     # PowerShell script builder
│
├── build/
│   ├── mod.rs
│   ├── winpe_builder.rs        # ENHANCE: Full customization
│   ├── iso_creator.rs          # ENHANCE: UEFI/BIOS hybrid
│   ├── image_preparer.rs       # IMPLEMENT: Full pipeline
│   ├── usb_writer.rs           # Existing
│   └── lightweight_builder.rs  # NEW: Network boot ISO
│
├── ui/                          # ENHANCE: Tauri commands
│   ├── mod.rs
│   ├── commands.rs             # ENHANCE: All IPC handlers
│   └── state.rs                # NEW: UI state management
│
└── frontend/                    # NEW: React components
    ├── components/
    │   ├── Dashboard.tsx
    │   ├── ImageList.tsx
    │   ├── ImageWizard/
    │   │   ├── Step1_OSSelection.tsx
    │   │   ├── Step2_Edition.tsx
    │   │   ├── Step3_OOBE.tsx
    │   │   ├── Step4_Applications.tsx
    │   │   ├── Step5_Configuration.tsx
    │   │   └── Step6_Review.tsx
    │   └── ProgressView.tsx
    └── hooks/
        └── useImageCreation.ts
```

---

## 4. Module Implementation Details

### 4.1 ESD Download Service (`download/esd_downloader.rs`)

```rust
pub struct EsdDownloader {
    client: reqwest::Client,
    download_path: PathBuf,
    progress_callback: Option<Box<dyn Fn(DownloadProgress)>>,
}

pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub eta_seconds: u64,
    pub percent: f32,
}

impl EsdDownloader {
    /// Download ESD from Microsoft CDN
    pub async fn download_esd(&self, os_version: &OsVersion) -> BitOSDTResult<PathBuf>;
    
    /// Resume interrupted download
    pub async fn resume_download(&self, partial_path: &Path) -> BitOSDTResult<PathBuf>;
    
    /// Validate downloaded file hash
    pub fn validate_hash(&self, file_path: &Path, expected_sha256: &str) -> BitOSDTResult<bool>;
    
    /// Convert ESD to WIM using DISM
    pub fn convert_esd_to_wim(&self, esd_path: &Path, output_wim: &Path) -> BitOSDTResult<()>;
    
    /// Cancel ongoing download
    pub fn cancel(&self) -> BitOSDTResult<()>;
}
```

**DISM Conversion Command:**
```powershell
dism /Export-Image /SourceImageFile:install.esd /SourceIndex:6 /DestinationImageFile:install.wim /Compress:max /CheckIntegrity
```

---

### 4.2 Unattend.xml Generator (`config/unattend_generator.rs`)

```rust
pub struct UnattendGenerator;

pub struct UnattendConfig {
    pub language: String,
    pub timezone: String,
    pub keyboard_layout: String,
    pub skip_oobe: SkipOobeConfig,
    pub user_accounts: Vec<UserAccount>,
    pub computer_name: Option<String>,
    pub product_key: Option<String>,
    pub domain_join: Option<DomainJoinConfig>,
}

pub struct SkipOobeConfig {
    pub skip_machine_oobe: bool,
    pub skip_user_oobe: bool,
    pub hide_eula: bool,
    pub hide_wireless: bool,
    pub network_location: String, // "Home", "Work", "Other"
    pub protect_computer: String, // "1" = recommended, "3" = off
}

impl UnattendGenerator {
    /// Generate complete unattend.xml
    pub fn generate(&self, config: &UnattendConfig) -> BitOSDTResult<String>;
    
    /// Generate OOBE section only
    pub fn generate_oobe_section(&self, skip: &SkipOobeConfig) -> String;
    
    /// Generate user accounts section
    pub fn generate_user_section(&self, users: &[UserAccount]) -> String;
    
    /// Generate domain join section
    pub fn generate_domain_section(&self, domain: &DomainJoinConfig) -> String;
    
    /// Validate generated XML
    pub fn validate_xml(&self, xml: &str) -> BitOSDTResult<bool>;
}
```

**Sample Generated Unattend.xml:**
```xml
<?xml version="1.0" encoding="utf-8"?>
<unattend xmlns="urn:schemas-microsoft-com:unattend">
    <settings pass="windowsPE">
        <component name="Microsoft-Windows-International-Core-WinPE">
            <SetupUILanguage>
                <UILanguage>en-US</UILanguage>
            </SetupUILanguage>
            <InputLocale>0409:00000409</InputLocale>
            <SystemLocale>en-US</SystemLocale>
            <UILanguage>en-US</UILanguage>
            <UserLocale>en-US</UserLocale>
        </component>
    </settings>
    <settings pass="specialize">
        <component name="Microsoft-Windows-Shell-Setup">
            <ComputerName>DESKTOP-*</ComputerName>
            <TimeZone>Pacific Standard Time</TimeZone>
        </component>
        <component name="Microsoft-Windows-UnattendedJoin">
            <Identification>
                <JoinDomain>corp.contoso.com</JoinDomain>
                <MachineObjectOU>OU=Computers,DC=corp,DC=contoso,DC=com</MachineObjectOU>
                <Credentials>
                    <Domain>corp</Domain>
                    <Password>********</Password>
                    <Username>domainadmin</Username>
                </Credentials>
            </Identification>
        </component>
    </settings>
    <settings pass="oobeSystem">
        <component name="Microsoft-Windows-Shell-Setup">
            <OOBE>
                <HideEULAPage>true</HideEULAPage>
                <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>
                <NetworkLocation>Work</NetworkLocation>
                <ProtectYourPC>1</ProtectYourPC>
                <SkipMachineOOBE>true</SkipMachineOOBE>
                <SkipUserOOBE>true</SkipUserOOBE>
            </OOBE>
            <UserAccounts>
                <LocalAccounts>
                    <LocalAccount wcm:action="add">
                        <Name>Admin</Name>
                        <Group>Administrators</Group>
                        <Password>
                            <Value>********</Value>
                            <PlainText>false</PlainText>
                        </Password>
                    </LocalAccount>
                </LocalAccounts>
            </UserAccounts>
            <AutoLogon>
                <Enabled>true</Enabled>
                <Username>Admin</Username>
                <Password>
                    <Value>********</Value>
                    <PlainText>false</PlainText>
                </Password>
                <LogonCount>1</LogonCount>
            </AutoLogon>
            <FirstLogonCommands>
                <SynchronousCommand wcm:action="add">
                    <Order>1</Order>
                    <CommandLine>cmd /c C:\BitOSDT\SetupComplete.cmd</CommandLine>
                    <Description>Run BitOSDT post-install tasks</Description>
                </SynchronousCommand>
            </FirstLogonCommands>
        </component>
    </settings>
</unattend>
```

---

### 4.3 Application Installer System (`tasks/app_installer.rs`)

```rust
pub struct AppInstaller {
    winget_path: PathBuf,
    choco_path: Option<PathBuf>,
}

pub enum AppSource {
    Winget { package_id: String, version: Option<String> },
    Chocolatey { package_name: String, version: Option<String> },
    Custom { 
        path: String,  // Local path or URL
        silent_args: String,
        installer_type: InstallerType,
    },
}

pub enum InstallerType {
    Exe,
    Msi,
    Msix,
}

pub struct AppInstallTask {
    pub id: Uuid,
    pub name: String,
    pub source: AppSource,
    pub order: u32,
    pub required: bool,
    pub reboot_required: bool,
}

impl AppInstaller {
    /// Generate PowerShell script to install all apps
    pub fn generate_install_script(&self, apps: &[AppInstallTask]) -> String;
    
    /// Generate winget install command
    fn winget_command(&self, package_id: &str, version: Option<&str>) -> String;
    
    /// Generate chocolatey install command  
    fn choco_command(&self, package: &str, version: Option<&str>) -> String;
    
    /// Generate custom installer command
    fn custom_command(&self, path: &str, args: &str, installer_type: &InstallerType) -> String;
}
```

**Generated PowerShell Script (install-apps.ps1):**
```powershell
# BitOSDT Application Installation Script
# Generated: 2026-02-04T12:00:00Z

$ErrorActionPreference = "Continue"
$LogPath = "C:\BitOSDT\Logs\app-install.log"

function Write-Log {
    param([string]$Message)
    $Timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    "$Timestamp - $Message" | Out-File -Append $LogPath
    Write-Host $Message
}

Write-Log "Starting application installation..."

# Install winget packages
Write-Log "Installing winget packages..."

$wingetApps = @(
    @{Id="Microsoft.VisualStudioCode"; Version=$null},
    @{Id="Google.Chrome"; Version=$null},
    @{Id="7zip.7zip"; Version=$null}
)

foreach ($app in $wingetApps) {
    Write-Log "Installing $($app.Id)..."
    $args = "install --id $($app.Id) --silent --accept-source-agreements --accept-package-agreements"
    if ($app.Version) { $args += " --version $($app.Version)" }
    Start-Process "winget" -ArgumentList $args -Wait -NoNewWindow
}

# Install Chocolatey packages
Write-Log "Installing Chocolatey packages..."

if (-not (Get-Command choco -ErrorAction SilentlyContinue)) {
    Write-Log "Installing Chocolatey..."
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
}

$chocoApps = @("notepadplusplus", "vlc")
foreach ($app in $chocoApps) {
    Write-Log "Installing $app via Chocolatey..."
    choco install $app -y --no-progress
}

# Custom installers
Write-Log "Installing custom applications..."

# Example: Custom EXE
$customApps = @(
    @{Path="C:\BitOSDT\Installers\MyApp.exe"; Args="/S /SILENT"; Type="exe"},
    @{Path="C:\BitOSDT\Installers\MyMSI.msi"; Args="/qn /norestart"; Type="msi"}
)

foreach ($app in $customApps) {
    Write-Log "Installing $($app.Path)..."
    if ($app.Type -eq "msi") {
        Start-Process "msiexec" -ArgumentList "/i `"$($app.Path)`" $($app.Args)" -Wait -NoNewWindow
    } else {
        Start-Process $app.Path -ArgumentList $app.Args -Wait -NoNewWindow
    }
}

Write-Log "Application installation complete."
```

---

### 4.4 Autopilot Configuration (`config/autopilot_generator.rs`)

```rust
pub struct AutopilotGenerator;

pub struct AutopilotProfile {
    pub tenant_id: String,
    pub aad_app_id: String,
    pub deployment_profile: DeploymentProfile,
    pub group_tag: Option<String>,
    pub assigned_user: Option<String>,
}

pub struct DeploymentProfile {
    pub profile_name: String,
    pub device_name_template: String,
    pub language: String,
    pub skip_keyboard: bool,
    pub skip_privacy: bool,
    pub user_driven: bool,  // vs self-deploying
}

impl AutopilotGenerator {
    /// Generate AutopilotConfigurationFile.json
    pub fn generate_profile(&self, profile: &AutopilotProfile) -> BitOSDTResult<String>;
    
    /// Generate provisioning package for offline enrollment
    pub fn generate_ppkg(&self, profile: &AutopilotProfile) -> BitOSDTResult<PathBuf>;
}
```

**Generated AutopilotConfigurationFile.json:**
```json
{
    "CloudAssignedTenantId": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
    "CloudAssignedTenantDomain": "contoso.onmicrosoft.com",
    "CloudAssignedOobeConfig": 1310,
    "CloudAssignedDomainJoinMethod": 0,
    "CloudAssignedLanguage": "en-US",
    "CloudAssignedDeviceName": "PC-%SERIAL%",
    "ZtdCorrelationId": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
    "CloudAssignedAadServerData": "{...}",
    "CloudAssignedForcedEnrollment": 1
}
```

---

### 4.5 Post-Install Task System (`tasks/task_runner.rs`)

```rust
pub struct TaskRunner;

pub enum PostInstallTask {
    RunWindowsUpdate(WindowsUpdateConfig),
    InstallApplications(Vec<AppInstallTask>),
    JoinDomain(DomainJoinConfig),
    CreateUser(UserAccount),
    RunScript(CustomScript),
    SetRegistry(RegistryMod),
    CopyFiles(FileCopyTask),
    EnableFeature(WindowsFeature),
    Reboot(RebootConfig),
}

pub struct TaskSequence {
    pub tasks: Vec<PostInstallTask>,
    pub on_failure: FailureAction,
    pub log_path: PathBuf,
}

pub enum FailureAction {
    Continue,
    Abort,
    Retry(u32),
}

impl TaskRunner {
    /// Generate SetupComplete.cmd that orchestrates all tasks
    pub fn generate_setup_complete(&self, sequence: &TaskSequence) -> String;
    
    /// Generate individual task scripts
    pub fn generate_task_scripts(&self, sequence: &TaskSequence) -> Vec<(String, String)>;
}
```

**Generated SetupComplete.cmd:**
```batch
@echo off
REM BitOSDT Post-Installation Tasks
REM Generated: 2026-02-04T12:00:00Z

set LOGFILE=C:\BitOSDT\Logs\setup-complete.log
set SCRIPTDIR=C:\BitOSDT\Tasks

echo %DATE% %TIME% - Starting BitOSDT post-install tasks >> %LOGFILE%

REM Task 1: Create local user
echo %DATE% %TIME% - Creating local user >> %LOGFILE%
powershell.exe -ExecutionPolicy Bypass -File "%SCRIPTDIR%\create-user.ps1" >> %LOGFILE% 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo %DATE% %TIME% - WARNING: User creation failed >> %LOGFILE%
)

REM Task 2: Join domain
echo %DATE% %TIME% - Joining domain >> %LOGFILE%
powershell.exe -ExecutionPolicy Bypass -File "%SCRIPTDIR%\join-domain.ps1" >> %LOGFILE% 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo %DATE% %TIME% - WARNING: Domain join failed >> %LOGFILE%
)

REM Task 3: Install applications
echo %DATE% %TIME% - Installing applications >> %LOGFILE%
powershell.exe -ExecutionPolicy Bypass -File "%SCRIPTDIR%\install-apps.ps1" >> %LOGFILE% 2>&1

REM Task 4: Run Windows Update
echo %DATE% %TIME% - Running Windows Update >> %LOGFILE%
powershell.exe -ExecutionPolicy Bypass -File "%SCRIPTDIR%\run-updates.ps1" >> %LOGFILE% 2>&1

REM Cleanup
echo %DATE% %TIME% - Cleaning up >> %LOGFILE%
del /q "C:\BitOSDT\Installers\*.*" 2>nul

REM Signal completion
echo %DATE% %TIME% - BitOSDT setup complete >> %LOGFILE%
echo COMPLETE > C:\BitOSDT\setup-complete.flag

REM Optional: Reboot if required
REM shutdown /r /t 60 /c "BitOSDT setup complete. Rebooting..."
```

---

### 4.6 Full ISO Builder (`build/image_preparer.rs`)

```rust
pub struct ImagePreparer {
    workspace_path: PathBuf,
    winpe_builder: WinPEBuilder,
    iso_creator: IsoCreator,
}

pub struct FullIsoConfig {
    pub name: String,
    pub windows_wim: PathBuf,
    pub unattend_xml: String,
    pub autopilot_json: Option<String>,
    pub drivers: Option<PathBuf>,
    pub tasks: TaskSequence,
    pub output_path: PathBuf,
}

impl ImagePreparer {
    /// Build complete bootable ISO
    pub fn build_full_iso(&self, config: &FullIsoConfig) -> BitOSDTResult<PathBuf>;
    
    /// Prepare WinPE with BitOSDT embedded
    fn prepare_winpe(&self, workspace: &Path) -> BitOSDTResult<PathBuf>;
    
    /// Customize startnet.cmd for auto-deployment
    fn customize_startnet(&self, winpe_mount: &Path) -> BitOSDTResult<()>;
    
    /// Embed configuration files
    fn embed_config(&self, winpe_mount: &Path, config: &FullIsoConfig) -> BitOSDTResult<()>;
    
    /// Create final ISO with both boot.wim and install.wim
    fn create_iso(&self, workspace: &Path, output: &Path) -> BitOSDTResult<()>;
}
```

**Custom startnet.cmd:**
```batch
wpeinit
X:\BitOSDT\bitosdt.exe deploy --auto --config X:\BitOSDT\config.json
```

---

### 4.7 Lightweight ISO Builder (`build/lightweight_builder.rs`)

```rust
pub struct LightweightBuilder {
    workspace_path: PathBuf,
    winpe_builder: WinPEBuilder,
}

pub struct LightweightConfig {
    pub name: String,
    pub download_source: DownloadSource,
    pub fallback_sources: Vec<DownloadSource>,
    pub embedded_config: ImageConfiguration,
    pub network_config: NetworkConfig,
    pub output_path: PathBuf,
}

pub enum DownloadSource {
    MicrosoftCdn { esd_id: String },
    HttpServer { base_url: String, wim_path: String },
    SmbShare { unc_path: String, credentials: Option<Credentials> },
    BitOsdtServer { server_url: String, image_id: Uuid },
}

pub struct NetworkConfig {
    pub dhcp: bool,
    pub static_ip: Option<IpConfig>,
    pub wifi: Option<WifiConfig>,
    pub proxy: Option<ProxyConfig>,
}

impl LightweightBuilder {
    /// Build lightweight network-boot ISO
    pub fn build_lightweight_iso(&self, config: &LightweightConfig) -> BitOSDTResult<PathBuf>;
    
    /// Add network drivers to WinPE (essential for download)
    fn add_network_drivers(&self, winpe_mount: &Path) -> BitOSDTResult<()>;
    
    /// Embed download configuration
    fn embed_download_config(&self, winpe_mount: &Path, config: &LightweightConfig) -> BitOSDTResult<()>;
    
    /// Generate network-aware startnet.cmd
    fn generate_network_startnet(&self) -> String;
}
```

**Network-aware startnet.cmd:**
```batch
wpeinit

REM Initialize network
wpeutil InitializeNetwork
ping -n 5 127.0.0.1 > nul

REM Start BitOSDT in network mode
X:\BitOSDT\bitosdt.exe deploy --auto --network --config X:\BitOSDT\config.json

REM If network fails, show error
if %ERRORLEVEL% NEQ 0 (
    echo Network deployment failed. Check network connection.
    cmd /k
)
```

---

## 5. UI Specification

### 5.1 Dashboard (Main Screen)

```
┌─────────────────────────────────────────────────────────────────────┐
│  BitOSDT 2.0                                      [Settings] [Help] │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Welcome to BitOSDT 2.0                                             │
│   Windows Deployment Made Simple                                     │
│                                                                      │
├────────────────────────┬─────────────────────────────────────────────┤
│  Quick Actions         │  Recent Images                              │
│  ──────────────────    │  ──────────────                              │
│                        │                                              │
│  [📥 Download Windows] │  📦 Windows 11 Pro 24H2       Ready  [▶]   │
│                        │     Created: Feb 4, 2026                     │
│  [🖼️ Create Image    ] │                                              │
│                        │  📦 Windows 10 Enterprise     Building...   │
│  [💾 Build Full ISO  ] │     Progress: 65%                           │
│                        │                                              │
│  [🌐 Build Light ISO ] │  📦 Lightweight Deploy Kit    Ready  [▶]   │
│                        │     Network boot image                       │
│                        │                                              │
├────────────────────────┼─────────────────────────────────────────────┤
│  System Status         │  Statistics                                  │
│  ──────────────────    │  ──────────                                  │
│                        │                                              │
│  ✅ Windows ADK: Found │  Total Images: 12                           │
│  ✅ Database: OK       │  Total Deployments: 156                     │
│  ✅ Storage: 234 GB    │  Success Rate: 98.7%                        │
│                        │                                              │
└────────────────────────┴─────────────────────────────────────────────┘
```

### 5.2 Image Creation Wizard (6 Steps)

**Step 1: OS Selection**
```
┌─────────────────────────────────────────────────────────────────────┐
│  Create New Image                                    Step 1 of 6   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Select Operating System                                             │
│  ═══════════════════════                                             │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  ○  Windows 10                                                │    │
│  │     └─ 22H2 (Build 19045)                                    │    │
│  │     └─ 21H2 (Build 19044)                                    │    │
│  │                                                               │    │
│  │  ●  Windows 11                                                │    │
│  │     └─ ● 24H2 (Build 26100) - Latest                         │    │
│  │     └─ ○ 23H2 (Build 22631)                                  │    │
│  │     └─ ○ 22H2 (Build 22621)                                  │    │
│  │                                                               │    │
│  │  ○  Windows Server                                            │    │
│  │     └─ 2025 Preview                                          │    │
│  │     └─ 2022                                                   │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Architecture: [x64 ▼]    Language: [English (United States) ▼]     │
│                                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                               [Cancel]  [Next →]    │
└─────────────────────────────────────────────────────────────────────┘
```

**Step 2: Edition & License**
```
┌─────────────────────────────────────────────────────────────────────┐
│  Create New Image                                    Step 2 of 6   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Select Edition & License                                            │
│  ════════════════════════                                            │
│                                                                      │
│  Edition:                                                            │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  ○  Home                                                      │    │
│  │  ●  Pro                                                       │    │
│  │  ○  Enterprise                                                │    │
│  │  ○  Education                                                 │    │
│  │  ○  Pro for Workstations                                     │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Activation:                                                         │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  ●  Generic Key (No activation)                              │    │
│  │  ○  Retail Product Key                                       │    │
│  │  ○  Volume License (KMS)                                     │    │
│  │  ○  OEM BIOS Key (Auto-activate)                             │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Product Key (optional): [_________________________]                 │
│                                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                         [← Back]  [Cancel]  [Next →]│
└─────────────────────────────────────────────────────────────────────┘
```

**Step 3: OOBE & User Settings**
```
┌─────────────────────────────────────────────────────────────────────┐
│  Create New Image                                    Step 3 of 6   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  OOBE & User Configuration                                           │
│  ══════════════════════════                                          │
│                                                                      │
│  Skip OOBE Screens:                                                  │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  [✓] Skip region selection (Use: English (United States))   │    │
│  │  [✓] Skip keyboard layout                                    │    │
│  │  [✓] Skip privacy settings (Accept all)                     │    │
│  │  [✓] Skip network setup (Allow offline account)             │    │
│  │  [✓] Skip Microsoft account sign-in                         │    │
│  │  [✓] Skip Cortana setup                                     │    │
│  │  [✓] Auto-accept EULA                                       │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Create Local User:                                                  │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  [✓] Create default user account                            │    │
│  │                                                              │    │
│  │  Username:     [Admin________________]                       │    │
│  │  Password:     [••••••••_____________]                       │    │
│  │  Confirm:      [••••••••_____________]                       │    │
│  │                                                              │    │
│  │  [✓] Administrator account                                  │    │
│  │  [✓] Auto-login (first boot only)                           │    │
│  │  [ ] Password never expires                                 │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Computer Name: [DESKTOP-%RANDOM%________]  (Supports variables)    │
│                                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                         [← Back]  [Cancel]  [Next →]│
└─────────────────────────────────────────────────────────────────────┘
```

**Step 4: Applications**
```
┌─────────────────────────────────────────────────────────────────────┐
│  Create New Image                                    Step 4 of 6   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Install Applications                                                │
│  ════════════════════                                                │
│                                                                      │
│  [Winget] [Chocolatey] [Custom]                                     │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  Winget Packages:                              [+ Add]       │    │
│  │  ─────────────────────────────────────────────────────────  │    │
│  │  📦 Microsoft.VisualStudioCode           [✓]     [🗑️]        │    │
│  │  📦 Google.Chrome                         [✓]     [🗑️]        │    │
│  │  📦 7zip.7zip                             [✓]     [🗑️]        │    │
│  │  📦 Mozilla.Firefox                       [ ]     [🗑️]        │    │
│  │                                                              │    │
│  │  Search: [Search winget packages...____] [Search]            │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  Custom Installers:                           [+ Add]        │    │
│  │  ─────────────────────────────────────────────────────────  │    │
│  │  📄 MyCorpApp.msi                                [🗑️]        │    │
│  │     Path: C:\Installers\MyCorpApp.msi                        │    │
│  │     Args: /qn /norestart                                     │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                         [← Back]  [Cancel]  [Next →]│
└─────────────────────────────────────────────────────────────────────┘
```

**Step 5: Advanced Configuration**
```
┌─────────────────────────────────────────────────────────────────────┐
│  Create New Image                                    Step 5 of 6   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Advanced Configuration                                              │
│  ══════════════════════                                              │
│                                                                      │
│  ┌─── Domain Join ─────────────────────────────────────────────┐    │
│  │  [ ] Join Active Directory Domain                           │    │
│  │                                                              │    │
│  │  Domain:   [corp.contoso.com___________]                    │    │
│  │  Username: [domainadmin________________]                    │    │
│  │  Password: [••••••••___________________]                    │    │
│  │  OU Path:  [OU=Computers,DC=corp,DC=..._] (optional)       │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌─── Autopilot ───────────────────────────────────────────────┐    │
│  │  [ ] Configure Windows Autopilot                            │    │
│  │                                                              │    │
│  │  Tenant ID: [xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx]          │    │
│  │  Profile:   [Select or upload JSON...______] [Browse]       │    │
│  │  Group Tag: [Production__________________] (optional)       │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌─── Windows Update ──────────────────────────────────────────┐    │
│  │  [✓] Run Windows Update after installation                  │    │
│  │  [✓] Include driver updates                                 │    │
│  │  [ ] Specific KBs only: [________________]                  │    │
│  │  Timeout: [60] minutes                                      │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                         [← Back]  [Cancel]  [Next →]│
└─────────────────────────────────────────────────────────────────────┘
```

**Step 6: Review & Build**
```
┌─────────────────────────────────────────────────────────────────────┐
│  Create New Image                                    Step 6 of 6   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Review Configuration                                                │
│  ════════════════════                                                │
│                                                                      │
│  Image Name: [Windows 11 Pro 24H2 - Corp Standard___]               │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  Operating System                                            │    │
│  │  ─────────────────                                           │    │
│  │  • Windows 11 Pro 24H2 (Build 26100)                        │    │
│  │  • Architecture: x64                                         │    │
│  │  • Language: English (United States)                         │    │
│  │                                                              │    │
│  │  OOBE Configuration                                          │    │
│  │  ─────────────────                                           │    │
│  │  • All OOBE screens will be skipped                         │    │
│  │  • Local user "Admin" will be created (Administrator)       │    │
│  │  • Auto-login enabled for first boot                        │    │
│  │                                                              │    │
│  │  Applications (4)                                            │    │
│  │  ────────────────                                            │    │
│  │  • Microsoft.VisualStudioCode (winget)                      │    │
│  │  • Google.Chrome (winget)                                   │    │
│  │  • 7zip.7zip (winget)                                       │    │
│  │  • MyCorpApp.msi (custom)                                   │    │
│  │                                                              │    │
│  │  Post-Install Tasks                                          │    │
│  │  ─────────────────                                           │    │
│  │  • Windows Update: Enabled (with drivers)                   │    │
│  │  • Domain Join: corp.contoso.com                            │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Output Type:                                                        │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  ● Full ISO (Self-contained, ~6-8 GB)                       │    │
│  │  ○ Lightweight ISO (Network download, ~500 MB)              │    │
│  │  ○ USB Drive (Direct write)                                 │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Output Path: [C:\BitOSDT\Output\___________] [Browse]              │
│                                                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                         [← Back]  [Cancel]  [Build] │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 6. Data Models

### 6.1 Extended Image Configuration Model

```rust
/// Complete image configuration for building
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfiguration {
    /// Basic image info
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    
    /// OS Selection
    pub os_config: OsConfiguration,
    
    /// License & Edition
    pub license_config: LicenseConfiguration,
    
    /// OOBE Skip Configuration
    pub oobe_config: OobeConfiguration,
    
    /// User Accounts
    pub user_config: UserConfiguration,
    
    /// Applications to Install
    pub app_config: AppConfiguration,
    
    /// Domain Join Settings
    pub domain_config: Option<DomainConfiguration>,
    
    /// Autopilot Settings
    pub autopilot_config: Option<AutopilotConfiguration>,
    
    /// Windows Update Settings
    pub update_config: UpdateConfiguration,
    
    /// Driver Preferences
    pub driver_config: DriverConfiguration,
    
    /// Output Settings
    pub output_config: OutputConfiguration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsConfiguration {
    pub os_type: OsType,
    pub version: String,        // "24H2", "23H2", etc.
    pub build: String,          // "26100", "22631", etc.
    pub architecture: Architecture,
    pub language: String,       // "en-US", "de-DE", etc.
    pub esd_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseConfiguration {
    pub edition: WindowsEdition,
    pub activation_type: ActivationType,
    pub product_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WindowsEdition {
    Home,
    Pro,
    ProWorkstations,
    Enterprise,
    Education,
    EnterpriseLTSC,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OobeConfiguration {
    pub skip_region: bool,
    pub skip_keyboard: bool,
    pub skip_privacy: bool,
    pub skip_network: bool,
    pub skip_microsoft_account: bool,
    pub skip_cortana: bool,
    pub auto_accept_eula: bool,
    pub timezone: String,
    pub input_locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfiguration {
    pub create_user: bool,
    pub users: Vec<LocalUserAccount>,
    pub computer_name_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalUserAccount {
    pub username: String,
    pub password_encrypted: String,
    pub is_admin: bool,
    pub auto_login: bool,
    pub password_never_expires: bool,
    pub require_password_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfiguration {
    pub winget_packages: Vec<WingetPackage>,
    pub chocolatey_packages: Vec<ChocolateyPackage>,
    pub custom_installers: Vec<CustomInstaller>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WingetPackage {
    pub package_id: String,
    pub version: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChocolateyPackage {
    pub package_name: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInstaller {
    pub name: String,
    pub path: String,           // Local path or URL
    pub silent_args: String,
    pub installer_type: InstallerType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainConfiguration {
    pub domain_name: String,
    pub username: String,
    pub password_encrypted: String,
    pub ou_path: Option<String>,
    pub fallback_to_workgroup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfiguration {
    pub tenant_id: String,
    pub profile_json: String,
    pub group_tag: Option<String>,
    pub assigned_user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfiguration {
    pub run_windows_update: bool,
    pub include_drivers: bool,
    pub specific_kbs: Vec<String>,
    pub timeout_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverConfiguration {
    pub use_driverpacks: bool,
    pub use_clouddriver_post_install: bool,
    pub embed_drivers_in_winpe: bool,
    pub custom_driver_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfiguration {
    pub output_type: OutputType,
    pub output_path: PathBuf,
    pub usb_device: Option<String>,
    
    // Lightweight ISO specific
    pub download_source: Option<DownloadSourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputType {
    FullIso,
    LightweightIso,
    UsbDrive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSourceConfig {
    pub source_type: DownloadSourceType,
    pub url: String,
    pub credentials: Option<NetworkCredentials>,
    pub fallback_to_microsoft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadSourceType {
    MicrosoftCdn,
    HttpServer,
    SmbShare,
    BitOsdtServer,
}
```

---

## 7. Testing Plan

### 7.1 Unit Tests

| Module | Test | Status |
|--------|------|--------|
| `esd_downloader` | Download progress tracking | [ ] |
| `esd_downloader` | Resume interrupted download | [ ] |
| `esd_downloader` | Hash validation | [ ] |
| `unattend_generator` | Generate valid XML | [ ] |
| `unattend_generator` | All OOBE options | [ ] |
| `unattend_generator` | Domain join section | [ ] |
| `app_installer` | Winget command generation | [ ] |
| `app_installer` | Chocolatey command generation | [ ] |
| `app_installer` | Custom installer handling | [ ] |
| `autopilot_generator` | Valid JSON output | [ ] |
| `task_runner` | SetupComplete.cmd generation | [ ] |
| `image_preparer` | ISO structure validation | [ ] |
| `lightweight_builder` | Network config embedding | [ ] |

### 7.2 Integration Tests

| Test Scenario | VM Type | Expected Result | Status |
|---------------|---------|-----------------|--------|
| Full ISO boot + install | Hyper-V Gen2 | Windows installs, all config applied | [ ] |
| Full ISO boot + install | VMware | Windows installs, all config applied | [ ] |
| Lightweight ISO + HTTP download | Hyper-V | Downloads WIM, installs successfully | [ ] |
| OOBE skip verification | Any VM | No OOBE prompts shown | [ ] |
| User creation | Any VM | User exists with correct privileges | [ ] |
| Domain join | Domain-joined VM | Computer in correct OU | [ ] |
| App installation (winget) | Any VM | Apps installed and working | [ ] |
| App installation (choco) | Any VM | Apps installed and working | [ ] |
| Windows Update | Any VM | Updates installed | [ ] |
| Autopilot enrollment | Intune tenant | Device appears in Intune | [ ] |

### 7.3 Physical Hardware Tests

| Hardware | Test | Status |
|----------|------|--------|
| Dell Latitude | Full deployment via USB | [ ] |
| HP EliteBook | Full deployment via ISO | [ ] |
| Lenovo ThinkPad | Lightweight deployment | [ ] |
| Microsoft Surface | Full deployment + drivers | [ ] |

---

## 8. Implementation Checklist

### Phase 1: Core Infrastructure (Week 1-2)
- [ ] Create `download/` module directory structure
- [ ] Implement `EsdDownloader` struct and methods
- [ ] Implement download progress tracking
- [ ] Implement download resume support
- [ ] Implement SHA256 validation
- [ ] Implement ESD→WIM conversion via DISM
- [ ] Add unit tests for download module

### Phase 2: Configuration Generators (Week 2-3)
- [ ] Create `config/` module directory structure
- [ ] Implement `UnattendGenerator` struct
- [ ] Generate OOBE section (all skip options)
- [ ] Generate user accounts section
- [ ] Generate domain join section
- [ ] Generate regional settings section
- [ ] Implement `AutopilotGenerator` struct
- [ ] Generate AutopilotConfigurationFile.json
- [ ] Add unit tests for config generators

### Phase 3: Task System (Week 3-4)
- [ ] Create `tasks/` module directory structure
- [ ] Implement `AppInstaller` struct
- [ ] Generate winget install commands
- [ ] Generate chocolatey install commands
- [ ] Generate custom installer commands
- [ ] Implement `TaskRunner` struct
- [ ] Generate SetupComplete.cmd
- [ ] Generate individual task PowerShell scripts
- [ ] Implement `DomainJoin` script generator
- [ ] Implement `UserCreator` script generator
- [ ] Implement `WindowsUpdate` script generator
- [ ] Add unit tests for task system

### Phase 4: Full ISO Builder (Week 4-5)
- [ ] Enhance `image_preparer.rs` with full pipeline
- [ ] Implement WinPE customization
- [ ] Implement startnet.cmd modification
- [ ] Implement BitOSDT binary embedding
- [ ] Implement config file embedding
- [ ] Implement driver embedding (optional)
- [ ] Enhance `iso_creator.rs` for UEFI/BIOS hybrid
- [ ] Test ISO creation and boot
- [ ] Add integration tests

### Phase 5: Lightweight ISO Builder (Week 5-6)
- [ ] Create `lightweight_builder.rs`
- [ ] Add network driver injection to WinPE
- [ ] Implement download source configuration
- [ ] Generate network-aware startnet.cmd
- [ ] Implement download logic in deployment binary
- [ ] Implement fallback sources
- [ ] Test network boot and download
- [ ] Add integration tests

### Phase 6: UI Implementation (Week 6-8)
- [ ] Set up React project structure in `cargo/src/`
- [ ] Implement Dashboard component
- [ ] Implement ImageList component
- [ ] Implement Step1_OSSelection component
- [ ] Implement Step2_Edition component
- [ ] Implement Step3_OOBE component
- [ ] Implement Step4_Applications component
- [ ] Implement Step5_Configuration component
- [ ] Implement Step6_Review component
- [ ] Implement ProgressView component
- [ ] Create `useImageCreation` hook
- [ ] Style all components with Tailwind CSS

### Phase 7: Tauri IPC Commands (Week 7-8)
- [ ] Enhance `commands.rs` with all handlers
- [ ] `list_os_versions` command
- [ ] `download_esd` command with progress
- [ ] `create_image_config` command
- [ ] `build_full_iso` command with progress
- [ ] `build_lightweight_iso` command
- [ ] `write_to_usb` command
- [ ] `get_build_progress` command
- [ ] `cancel_build` command
- [ ] Implement state management

### Phase 8: Testing & Documentation (Week 8-9)
- [ ] Run all unit tests
- [ ] Run all integration tests
- [ ] Test on physical hardware
- [ ] Write user documentation
- [ ] Write API documentation
- [ ] Create troubleshooting guide
- [ ] Performance optimization
- [ ] Bug fixes

---

## Appendix A: Generic Product Keys (For Testing)

| Edition | Generic Key |
|---------|-------------|
| Windows 11/10 Home | YTMG3-N6DKC-DKB77-7M9GH-8HVX7 |
| Windows 11/10 Pro | VK7JG-NPHTM-C97JM-9MPGT-3V66T |
| Windows 11/10 Enterprise | XGVPP-NMH47-7TTHJ-W3FW7-8HV2C |
| Windows 11/10 Education | YNMGQ-8RYV3-4PGQ3-C8XTP-7CFBY |
| Windows Server 2022 Standard | VDYBN-27WPP-V4HQT-9VMD4-VMK7H |
| Windows Server 2022 Datacenter | WX4NM-KYWYW-QJJR4-XV3QB-6VM33 |

---

## Appendix B: DISM Commands Reference

```powershell
# Export ESD to WIM
dism /Export-Image /SourceImageFile:install.esd /SourceIndex:6 /DestinationImageFile:install.wim /Compress:max /CheckIntegrity

# Mount WIM for modification
dism /Mount-Wim /WimFile:boot.wim /Index:1 /MountDir:C:\mount

# Add drivers to mounted image
dism /Image:C:\mount /Add-Driver /Driver:C:\drivers /Recurse

# Add packages to WinPE
dism /Image:C:\mount /Add-Package /PackagePath:WinPE-WMI.cab

# Unmount and commit changes
dism /Unmount-Wim /MountDir:C:\mount /Commit

# Apply WIM to disk
dism /Apply-Image /ImageFile:install.wim /Index:6 /ApplyDir:W:\

# Get WIM info
dism /Get-WimInfo /WimFile:install.wim
```

---

## Appendix C: oscdimg Commands Reference

```powershell
# Create UEFI-bootable ISO
oscdimg -m -o -u2 -udfver102 -bootdata:2#p0,e,bC:\winpe\boot\etfsboot.com#pEF,e,bC:\winpe\efi\microsoft\boot\efisys.bin C:\winpe C:\output\winpe.iso

# Create BIOS-bootable ISO
oscdimg -n -bC:\winpe\boot\etfsboot.com C:\winpe C:\output\winpe.iso

# Create hybrid BIOS/UEFI ISO
oscdimg -m -o -u2 -udfver102 -bootdata:2#p0,e,bboot\etfsboot.com#pEF,e,befi\microsoft\boot\efisys.bin C:\winpe C:\output\winpe.iso
```

---

*Document Version: 1.0*
*Last Updated: February 4, 2026*
*Author: BitOSDT Development Team*
