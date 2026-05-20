# BitOSDT 2.0 - Hardware Detection Specification

## Overview

Hardware detection is the foundation of the deployment system. It identifies the target device's manufacturer, model, and specifications to enable proper driver selection and deployment configuration.

## Requirements

- Detect manufacturer (Dell, HP, Lenovo, Microsoft, etc.)
- Detect model name/number
- Identify product SKU (for DriverPack matching)
- Detect architecture (x64, ARM64)
- Identify form factor (laptop, desktop, server, tablet)
- Detect VM vs physical hardware
- Gather hardware specifications (CPU, RAM, disk)

## WMI Classes and Properties

### Primary Detection Classes

#### 1. Win32_ComputerSystem
**Purpose:** Core system information
```rust
pub struct ComputerSystemInfo {
    pub manufacturer: String,       // "Dell Inc.", "HP", "Lenovo"
    pub model: String,              // "Latitude 5520", "EliteBook 840 G8"
    pub system_type: String,        // "x64-based PC"
    pub total_physical_memory: u64, // In bytes
    pub domain: String,             // Domain name (if joined)
    pub workgroup: String,          // Workgroup name
    pub part_of_domain: bool,
}
```

**WMI Query:**
```powershell
Get-CimInstance -ClassName Win32_ComputerSystem
```

#### 2. Win32_ComputerSystemProduct
**Purpose:** Product identification (especially for Lenovo)
```rust
pub struct ComputerSystemProductInfo {
    pub version: String,            // Lenovo model (e.g., "ThinkPad T14 Gen 2")
    pub identifying_number: String, // Serial number
    pub uuid: String,               // System UUID
}
```

**WMI Query:**
```powershell
Get-CimInstance -ClassName Win32_ComputerSystemProduct
```

#### 3. Win32_BaseBoard
**Purpose:** Baseboard/motherboard info (especially for HP)
```rust
pub struct BaseBoardInfo {
    pub manufacturer: String,
    pub product: String,            // HP Product ID (e.g., "8CD1")
    pub serial_number: String,
    pub version: String,
}
```

**WMI Query:**
```powershell
Get-CimInstance -ClassName Win32_BaseBoard
```

#### 4. CIM_ComputerSystem (Modern Replacement)
**Purpose:** Enhanced system info with SKU
```rust
pub struct CimComputerSystemInfo {
    pub system_sku_number: String,  // Dell SKU (e.g., "0A5D")
    pub pc_system_type: u16,        // 0=Unspecified, 1=Desktop, 2=Mobile, etc.
}
```

**WMI Query:**
```powershell
Get-CimInstance -ClassName CIM_ComputerSystem
```

### Supporting Classes

#### 5. Win32_BIOS
**Purpose:** BIOS information and serial number
```rust
pub struct BiosInfo {
    pub manufacturer: String,
    pub version: String,
    pub serial_number: String,
    pub release_date: String,
    pub smbios_version: String,
}
```

#### 6. Win32_SystemEnclosure
**Purpose:** Chassis type for form factor detection
```rust
pub struct SystemEnclosureInfo {
    pub chassis_types: Vec<u16>,    // Array of chassis type codes
}
```

**Chassis Type Mapping:**
```rust
pub fn get_form_factor(chassis_types: &[u16]) -> FormFactor {
    for &chassis in chassis_types {
        match chassis {
            8 | 9 | 10 | 11 | 12 | 14 | 18 | 21 => return FormFactor::Laptop,
            3 | 4 | 5 | 6 | 7 | 15 | 16 => return FormFactor::Desktop,
            23 => return FormFactor::Server,
            34 | 35 | 36 => return FormFactor::SmallFormFactor,
            13 | 31 | 32 | 30 => return FormFactor::Tablet,
            _ => continue,
        }
    }
    FormFactor::Unknown
}
```

#### 7. Win32_Processor
**Purpose:** CPU information
```rust
pub struct ProcessorInfo {
    pub name: String,               // "Intel(R) Core(TM) i7-1185G7"
    pub manufacturer: String,
    pub number_of_cores: u32,
    pub number_of_logical_processors: u32,
    pub architecture: u16,          // 0=x86, 1=MIPS, 2=Alpha, 3=PowerPC, 6=IA64, 9=x64
    pub max_clock_speed: u32,       // In MHz
}
```

#### 8. Win32_DiskDrive
**Purpose:** Physical disk information
```rust
pub struct DiskDriveInfo {
    pub model: String,
    pub size: u64,                  // In bytes
    pub media_type: String,         // "Fixed hard disk media", "Removable media"
    pub interface_type: String,     // "IDE", "SCSI", "USB"
}
```

#### 9. Win32_NetworkAdapter
**Purpose:** Network adapters (for MAC address, connectivity)
```rust
pub struct NetworkAdapterInfo {
    pub name: String,
    pub mac_address: String,
    pub adapter_type: String,
    pub speed: u64,                 // In bits per second
}
```

#### 10. Win32_Battery
**Purpose:** Battery detection (for laptop identification)
```rust
pub struct BatteryInfo {
    pub name: String,
    pub estimated_charge_remaining: u16,  // Percentage
    pub battery_status: u16,              // 1=Discharging, 2=Charging, etc.
}
```

#### 11. Win32_PnPEntity
**Purpose:** PnP devices (for device errors)
```rust
pub struct PnpEntityInfo {
    pub name: String,
    pub device_id: String,
    pub status: String,
    pub config_manager_error_code: u32,
}
```

#### 12. Win32_Tpm
**Purpose:** TPM information (for Autopilot/BitLocker)
```rust
pub struct TpmInfo {
    pub is_activated_initial_value: bool,
    pub is_enabled_initial_value: bool,
    pub is_owned_initial_value: bool,
    pub spec_version: String,       // "2.0"
}
```

## Manufacturer Normalization

### Normalization Logic

```rust
pub fn normalize_manufacturer(manufacturer: &str) -> String {
    let lower = manufacturer.to_lowercase();
    
    if lower.contains("dell") {
        "Dell".to_string()
    } else if lower.contains("lenovo") {
        "Lenovo".to_string()
    } else if lower.contains("hewlett") || lower.contains("packard") || manufacturer == "HP" {
        "HP".to_string()
    } else if lower.contains("microsoft") {
        "Microsoft".to_string()
    } else if lower.contains("panasonic") {
        "Panasonic".to_string()
    } else if lower.contains("to be filled") || manufacturer.is_empty() {
        "OEM".to_string()
    } else {
        manufacturer.to_string()
    }
}
```

### Model Extraction by Manufacturer

#### Dell
- **Model:** `Win32_ComputerSystem.Model`
- **Product:** `CIM_ComputerSystem.SystemSKUNumber`
- **Example:** Model="Latitude 5520", Product="0A5D"

#### HP
- **Model:** `Win32_ComputerSystem.Model`
- **Product:** `Win32_BaseBoard.Product`
- **Example:** Model="HP EliteBook 840 G8", Product="8CD1"

#### Lenovo
- **Model:** `Win32_ComputerSystemProduct.Version`
- **Product:** First 4 characters of `Win32_ComputerSystem.Model`
- **Example:** Model="ThinkPad T14 Gen 2", Product="20WE"

#### Microsoft Surface
- **Model:** `Win32_ComputerSystem.Model`
- **Product:** `CIM_ComputerSystem.SystemSKUNumber`
- **Example:** Model="Surface Pro 8", Product="1234"

## VM Detection

### Detection Methods

```rust
pub fn detect_vm() -> bool {
    let checks = vec![
        // Check BIOS information
        check_bios_vm_indicators(),
        
        // Check computer model
        check_model_vm_indicators(),
        
        // Check manufacturer
        check_manufacturer_vm_indicators(),
    ];
    
    checks.iter().any(|c| *c)
}

fn check_bios_vm_indicators() -> bool {
    let bios_indicators = [
        "vmware", "virtual machine", "virtualbox", "xen", "hyper-v",
        "hyperv", "kvm", "qemu", "parallels", "bhyve",
    ];
    
    let bios_info = get_bios_info();
    let combined = format!(
        "{} {} {}",
        bios_info.manufacturer.to_lowercase(),
        bios_info.version.to_lowercase(),
        bios_info.serial_number.to_lowercase()
    );
    
    bios_indicators.iter().any(|indicator| combined.contains(indicator))
}

fn check_model_vm_indicators() -> bool {
    let model_indicators = [
        "vmware", "virtual machine", "virtualbox", "xen", "hyper-v",
        "hyperv", "kvm", "qemu", "parallels", "bhyve",
        "gce", "google compute engine", "amazon ec2", "azure",
        "bochs", "openstack", "ovirt", "rhev", "kubevirt",
    ];
    
    let computer_info = get_computer_system_info();
    let model = computer_info.model.to_lowercase();
    
    model_indicators.iter().any(|indicator| model.contains(indicator))
}

fn check_manufacturer_vm_indicators() -> bool {
    let vm_manufacturers = [
        "vmware, inc.",
        "microsoft corporation",  // Hyper-V
        "xen",
        "innotek gmbh",           // VirtualBox
        "parallels",
    ];
    
    let computer_info = get_computer_system_info();
    let manufacturer = computer_info.manufacturer.to_lowercase();
    
    vm_manufacturers.iter().any(|m| manufacturer == *m)
}
```

## Data Structures

### Complete HardwareInfo Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    // Identification
    pub manufacturer: String,           // Normalized manufacturer name
    pub model: String,                  // Model name
    pub product: String,                // Product/SKU for DriverPack matching
    pub serial_number: String,          // Device serial number
    pub uuid: String,                   // System UUID
    
    // Classification
    pub architecture: Architecture,     // x64, ARM64
    pub form_factor: FormFactor,        // Laptop, Desktop, Server, Tablet
    pub is_vm: bool,                    // Virtual machine detection
    
    // Specifications
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub network_adapters: Vec<NetworkAdapterInfo>,
    
    // Platform details
    pub bios: BiosInfo,
    pub chassis_type: Option<u16>,
    pub has_battery: bool,
    pub tpm: Option<TpmInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Architecture {
    X86,
    X64,
    ARM64,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

## Implementation in Rust

### Using Windows-rs Crate

```rust
use windows::Win32::System::Wmi::*;
use windows::Win32::System::Com::*;

pub struct HardwareDetector;

impl HardwareDetector {
    pub fn new() -> Self {
        // Initialize WMI
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok();
        }
        Self
    }
    
    pub fn detect_all(&self) -> Result<HardwareInfo, HardwareError> {
        Ok(HardwareInfo {
            manufacturer: self.get_manufacturer()?,
            model: self.get_model()?,
            product: self.get_product()?,
            // ... other fields
        })
    }
    
    fn get_manufacturer(&self) -> Result<String, HardwareError> {
        // Query Win32_ComputerSystem
        let wmi = WmiConnection::new()?;
        let result: Vec<ComputerSystem> = wmi.query("SELECT * FROM Win32_ComputerSystem")?;
        
        if let Some(cs) = result.first() {
            Ok(normalize_manufacturer(&cs.manufacturer))
        } else {
            Err(HardwareError::WmiQueryFailed)
        }
    }
    
    // ... other detection methods
}
```

### Alternative: Using wmi-rs Crate

```rust
use wmi::*;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_ComputerSystem")]
struct Win32ComputerSystem {
    manufacturer: String,
    model: String,
    #[serde(rename = "TotalPhysicalMemory")]
    total_physical_memory: u64,
}

pub fn detect_hardware() -> Result<HardwareInfo, Box<dyn std::error::Error>> {
    let wmi_con = WMIConnection::new(COMLibrary::new()?)?;
    
    let results: Vec<Win32ComputerSystem> = wmi_con.query()?;
    
    if let Some(cs) = results.first() {
        Ok(HardwareInfo {
            manufacturer: normalize_manufacturer(&cs.manufacturer),
            model: cs.model.clone(),
            // ...
        })
    } else {
        Err("No computer system found".into())
    }
}
```

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum HardwareError {
    #[error("WMI query failed: {0}")]
    WmiQueryFailed(String),
    
    #[error("Failed to connect to WMI: {0}")]
    WmiConnectionFailed(String),
    
    #[error("Required WMI class not available: {0}")]
    ClassNotAvailable(String),
    
    #[error("Invalid hardware data: {0}")]
    InvalidData(String),
    
    #[error("COM initialization failed: {0}")]
    ComInitializationFailed(String),
}
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_manufacturer_normalization() {
        assert_eq!(normalize_manufacturer("Dell Inc."), "Dell");
        assert_eq!(normalize_manufacturer("HP"), "HP");
        assert_eq!(normalize_manufacturer("LENOVO"), "Lenovo");
        assert_eq!(normalize_manufacturer("Microsoft Corporation"), "Microsoft");
    }
    
    #[test]
    fn test_form_factor_detection() {
        assert_eq!(get_form_factor(&[10]), FormFactor::Laptop);  // Notebook
        assert_eq!(get_form_factor(&[3]), FormFactor::Desktop); // Desktop
        assert_eq!(get_form_factor(&[23]), FormFactor::Server); // Rack Mount
    }
    
    #[test]
    fn test_vm_detection() {
        // These should be detected as VMs
        assert!(is_vm_indicated("VMware Virtual Platform"));
        assert!(is_vm_indicated("VirtualBox"));
        assert!(is_vm_indicated("Microsoft Hyper-V"));
        
        // These should not
        assert!(!is_vm_indicated("Dell Latitude 5520"));
        assert!(!is_vm_indicated("HP EliteBook 840 G8"));
    }
}
```

### Integration Tests

```rust
#[test]
#[ignore = "Requires Windows with WMI"]
fn test_full_hardware_detection() {
    let detector = HardwareDetector::new();
    let info = detector.detect_all().expect("Should detect hardware");
    
    assert!(!info.manufacturer.is_empty());
    assert!(!info.model.is_empty());
    assert!(info.memory.total_bytes > 0);
}
```

## Performance Considerations

- **Parallel Queries:** Use async/await to query multiple WMI classes concurrently
- **Caching:** Cache hardware info for the duration of deployment
- **Lazy Loading:** Only query expensive properties when needed

## Cross-Platform Notes

- **Windows:** Full WMI support via windows-rs or wmi-rs
- **Linux:** No direct equivalent; use alternative detection methods
  - dmidecode for SMBIOS data
  - /sys/class for hardware info
  - lshw for comprehensive listing
