use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::models::{
    BiosInfo, CpuInfo, DiskInfo, FormFactor, HardwareInfo, MemoryInfo, NetworkAdapterInfo, TpmInfo,
};
#[cfg(target_os = "windows")]
use serde_json::Value;
use std::process::Command;
#[cfg(not(target_os = "windows"))]
use tracing::info;
use uuid::Uuid;

pub struct HardwareDetector;

impl Default for HardwareDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareDetector {
    pub fn new() -> Self {
        Self
    }

    /// Detect all hardware information
    pub fn detect_all(&self) -> BitOSDTResult<HardwareInfo> {
        #[cfg(target_os = "windows")]
        {
            self.detect_windows()
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.detect_linux()
        }
    }

    #[cfg(target_os = "windows")]
    fn detect_windows(&self) -> BitOSDTResult<HardwareInfo> {
        let info = HardwareInfo {
            manufacturer: self.get_wmi_string("Win32_ComputerSystem", "Manufacturer")?,
            model: self.get_wmi_string("Win32_ComputerSystem", "Model")?,
            product: self
                .get_wmi_string("Win32_ComputerSystemProduct", "Name")
                .unwrap_or_else(|_| "Unknown".to_string()),
            serial_number: self.get_wmi_string("Win32_Bios", "SerialNumber")?,
            uuid: self
                .get_wmi_string("Win32_ComputerSystemProduct", "UUID")
                .unwrap_or_else(|_| Uuid::new_v4().to_string()),
            architecture: self.detect_architecture(),
            form_factor: self.detect_form_factor()?,
            is_vm: self.detect_vm(),
            cpu: self.detect_cpu()?,
            memory: self.detect_memory()?,
            disks: self.detect_disks()?,
            network_adapters: self.detect_network_adapters()?,
            bios: self.detect_bios()?,
            chassis_type: self
                .get_wmi_u16("Win32_SystemEnclosure", "ChassisTypes")
                .ok(),
            has_battery: self.detect_battery(),
            tpm: self.detect_tpm().ok(),
        };

        Ok(info)
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_linux(&self) -> BitOSDTResult<HardwareInfo> {
        info!("Running Linux hardware detection");

        let dmi_dir = std::path::Path::new("/sys/class/dmi/id");
        let read_dmi = |file: &str| -> String {
            std::fs::read_to_string(dmi_dir.join(file))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Unknown".to_string())
        };

        let uuid = {
            let detected = read_dmi("product_uuid");
            if detected == "Unknown" {
                Uuid::new_v4().to_string()
            } else {
                detected
            }
        };

        Ok(HardwareInfo {
            manufacturer: read_dmi("sys_vendor"),
            model: read_dmi("product_name"),
            product: read_dmi("product_version"),
            serial_number: read_dmi("product_serial"),
            uuid,
            architecture: self.detect_architecture(),
            form_factor: self.detect_form_factor()?,
            is_vm: self.detect_vm(),
            cpu: self.detect_cpu()?,
            memory: self.detect_memory()?,
            disks: self.detect_disks()?,
            network_adapters: self.detect_network_adapters()?,
            bios: self.detect_bios()?,
            chassis_type: None,
            has_battery: self.detect_battery(),
            tpm: self.detect_tpm().ok(),
        })
    }

    #[cfg(target_os = "windows")]
    fn get_wmi_string(&self, class: &str, property: &str) -> BitOSDTResult<String> {
        let class_escaped = class.replace('\'', "''");
        let property_escaped = property.replace('\'', "''");
        let script = format!(
            "$obj = Get-CimInstance -ClassName '{class_name}' | Select-Object -First 1; \
             if ($null -eq $obj) {{ exit 1 }}; \
             $v = $obj.'{property_name}'; \
             if ($v -is [Array]) {{ $v = $v[0] }}; \
             if ($null -eq $v) {{ exit 1 }}; \
             [Console]::Out.Write($v.ToString())",
            class_name = class_escaped,
            property_name = property_escaped
        );
        Self::run_powershell(&script)
    }

    #[cfg(target_os = "windows")]
    fn get_wmi_u16(&self, class: &str, property: &str) -> BitOSDTResult<u16> {
        let raw = self.get_wmi_string(class, property)?;
        raw.trim().parse::<u16>().map_err(|e| {
            BitOSDTError::HardwareDetection(format!(
                "Failed to parse WMI value {}.{}='{}' as u16: {}",
                class, property, raw, e
            ))
        })
    }

    #[cfg(target_os = "windows")]
    fn run_powershell(script: &str) -> BitOSDTResult<String> {
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
            .map_err(|e| {
                BitOSDTError::HardwareDetection(format!("Failed to execute PowerShell: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(BitOSDTError::HardwareDetection(format!(
                "PowerShell query failed: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[cfg(target_os = "windows")]
    fn parse_json_rows(raw: &str) -> BitOSDTResult<Vec<Value>> {
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }

        let parsed: Value = serde_json::from_str(raw).map_err(|e| {
            BitOSDTError::HardwareDetection(format!(
                "Failed to parse PowerShell JSON output: {}",
                e
            ))
        })?;

        match parsed {
            Value::Array(arr) => Ok(arr),
            Value::Object(_) => Ok(vec![parsed]),
            Value::Null => Ok(Vec::new()),
            _ => Err(BitOSDTError::HardwareDetection(
                "Unexpected JSON shape from PowerShell query".to_string(),
            )),
        }
    }

    #[cfg(target_os = "windows")]
    fn json_string(value: &Value, key: &str, default: &str) -> String {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default.to_string())
    }

    #[cfg(target_os = "windows")]
    fn json_u64(value: &Value, key: &str, default: u64) -> u64 {
        value
            .get(key)
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
            })
            .unwrap_or(default)
    }

    #[cfg(target_os = "windows")]
    fn json_u32(value: &Value, key: &str, default: u32) -> u32 {
        Self::json_u64(value, key, default as u64) as u32
    }

    #[cfg(target_os = "windows")]
    fn json_bool(value: &Value, key: &str, default: bool) -> bool {
        value.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    #[cfg(target_os = "windows")]
    fn detect_architecture(&self) -> crate::core::models::Architecture {
        use std::env;
        match env::consts::ARCH {
            "x86_64" => crate::core::models::Architecture::X64,
            "aarch64" => crate::core::models::Architecture::Arm64,
            _ => crate::core::models::Architecture::X64,
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_architecture(&self) -> crate::core::models::Architecture {
        use std::env;
        match env::consts::ARCH {
            "x86_64" => crate::core::models::Architecture::X64,
            "aarch64" => crate::core::models::Architecture::Arm64,
            _ => crate::core::models::Architecture::X64,
        }
    }

    #[cfg(target_os = "windows")]
    fn detect_form_factor(&self) -> BitOSDTResult<FormFactor> {
        // Use ChassisTypes from Win32_SystemEnclosure
        // 3 = Desktop, 9 = Laptop, 10 = Notebook, 11 = Hand Held, etc.
        match self.get_wmi_u16("Win32_SystemEnclosure", "ChassisTypes") {
            Ok(chassis_type) => {
                let form_factor = match chassis_type {
                    3 | 4 | 5 | 6 | 7 => FormFactor::Desktop,
                    8 | 9 | 10 | 11 | 12 | 14 | 30 | 31 => FormFactor::Laptop,
                    15 | 16 | 17 | 18 | 19 | 20 | 21 | 22 | 23 => FormFactor::Server,
                    13 => FormFactor::SmallFormFactor,
                    _ => FormFactor::Unknown,
                };
                Ok(form_factor)
            }
            Err(_) => {
                // Fallback: check for battery
                if self.detect_battery() {
                    Ok(FormFactor::Laptop)
                } else {
                    Ok(FormFactor::Desktop)
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_form_factor(&self) -> BitOSDTResult<FormFactor> {
        // On Linux, check systemd chassis type
        if let Ok(output) = Command::new("hostnamectl").arg("chassis").output() {
            let chassis = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase();
            match chassis.as_str() {
                "laptop" | "notebook" | "convertible" | "detachable" => {
                    return Ok(FormFactor::Laptop);
                }
                "desktop" | "workstation" => {
                    return Ok(FormFactor::Desktop);
                }
                "server" => {
                    return Ok(FormFactor::Server);
                }
                _ => {}
            }
        }

        // Check for battery
        if std::path::Path::new("/sys/class/power_supply/BAT0").exists() {
            Ok(FormFactor::Laptop)
        } else {
            Ok(FormFactor::Desktop)
        }
    }

    fn detect_vm(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            // Check for common VM indicators
            if let Ok(model) = self.get_wmi_string("Win32_ComputerSystem", "Model") {
                let model_lower = model.to_lowercase();
                model_lower.contains("virtual")
                    || model_lower.contains("vmware")
                    || model_lower.contains("hyper-v")
                    || model_lower.contains("xen")
                    || model_lower.contains("kvm")
            } else {
                false
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Check for hypervisor flag in CPU
            if let Ok(contents) = std::fs::read_to_string("/proc/cpuinfo") {
                contents.to_lowercase().contains("hypervisor")
            } else {
                false
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn detect_cpu(&self) -> BitOSDTResult<CpuInfo> {
        Ok(CpuInfo {
            name: self.get_wmi_string("Win32_Processor", "Name")?,
            manufacturer: self.get_wmi_string("Win32_Processor", "Manufacturer")?,
            cores: self.get_wmi_u16("Win32_Processor", "NumberOfCores")? as u32,
            logical_processors: self.get_wmi_u16("Win32_Processor", "NumberOfLogicalProcessors")?
                as u32,
            max_speed_mhz: self.get_wmi_u16("Win32_Processor", "MaxClockSpeed")? as u32,
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_cpu(&self) -> BitOSDTResult<CpuInfo> {
        // Parse /proc/cpuinfo on Linux
        let contents = std::fs::read_to_string("/proc/cpuinfo").map_err(|e| {
            BitOSDTError::HardwareDetection(format!("Failed to read CPU info: {}", e))
        })?;

        let mut name = "Unknown".to_string();
        let mut manufacturer = "Unknown".to_string();
        let mut cores = 1u32;
        let mut logical_processors = 1u32;

        for line in contents.lines() {
            if line.starts_with("model name") {
                if let Some(idx) = line.find(':') {
                    name = line[idx + 1..].trim().to_string();
                }
            } else if line.starts_with("vendor_id") {
                if let Some(idx) = line.find(':') {
                    manufacturer = line[idx + 1..].trim().to_string();
                }
            } else if line.starts_with("cpu cores") {
                if let Some(idx) = line.find(':') {
                    cores = line[idx + 1..].trim().parse().unwrap_or(1);
                }
            } else if line.starts_with("processor") {
                if let Some(idx) = line.find(':') {
                    if let Ok(proc_num) = line[idx + 1..].trim().parse::<u32>() {
                        logical_processors = proc_num + 1;
                    }
                }
            }
        }

        Ok(CpuInfo {
            name,
            manufacturer,
            cores,
            logical_processors,
            max_speed_mhz: 0, // Would need /proc/cpuinfo or turbostat
        })
    }

    #[cfg(target_os = "windows")]
    fn detect_memory(&self) -> BitOSDTResult<MemoryInfo> {
        let total_bytes: u64 = self
            .get_wmi_string("Win32_ComputerSystem", "TotalPhysicalMemory")?
            .parse()
            .map_err(|_| BitOSDTError::HardwareDetection("Failed to parse memory".to_string()))?;

        Ok(MemoryInfo {
            total_bytes,
            total_gb: total_bytes as f64 / 1_073_741_824.0,
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_memory(&self) -> BitOSDTResult<MemoryInfo> {
        let contents = std::fs::read_to_string("/proc/meminfo").map_err(|e| {
            BitOSDTError::HardwareDetection(format!("Failed to read memory info: {}", e))
        })?;

        let mut total_bytes: u64 = 0;

        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                // Format: MemTotal:       16256732 kB
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        total_bytes = kb * 1024;
                    }
                }
                break;
            }
        }

        Ok(MemoryInfo {
            total_bytes,
            total_gb: total_bytes as f64 / 1_073_741_824.0,
        })
    }

    #[cfg(target_os = "windows")]
    fn detect_disks(&self) -> BitOSDTResult<Vec<DiskInfo>> {
        let script = "Get-CimInstance -ClassName Win32_DiskDrive | \
            Select-Object Index,Model,Size,MediaType,InterfaceType | \
            ConvertTo-Json -Compress";
        let raw = Self::run_powershell(script)?;
        let rows = Self::parse_json_rows(&raw)?;
        let mut disks = Vec::new();

        for row in rows {
            let size_bytes = Self::json_u64(&row, "Size", 0);
            disks.push(DiskInfo {
                index: Self::json_u32(&row, "Index", disks.len() as u32),
                model: Self::json_string(&row, "Model", "Unknown"),
                size_bytes,
                size_gb: size_bytes as f64 / 1_073_741_824.0,
                media_type: Self::json_string(&row, "MediaType", "Unknown"),
                interface_type: Self::json_string(&row, "InterfaceType", "Unknown"),
            });
        }

        Ok(disks)
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_disks(&self) -> BitOSDTResult<Vec<DiskInfo>> {
        use std::path::Path;

        let mut disks = Vec::new();
        let block_dir = Path::new("/sys/block");

        if let Ok(entries) = std::fs::read_dir(block_dir) {
            for (index, entry) in entries.flatten().enumerate() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Skip loop devices and RAM disks
                if name_str.starts_with("loop") || name_str.starts_with("ram") {
                    continue;
                }

                // Get size from /sys/block/{device}/size
                let size_path = entry.path().join("size");
                let size_bytes = std::fs::read_to_string(&size_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(|sectors| sectors * 512)
                    .unwrap_or(0);

                // Get model from /sys/block/{device}/device/model
                let model_path = entry.path().join("device/model");
                let model = std::fs::read_to_string(&model_path)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| name_str.to_string());

                // Determine media type (rotational vs SSD)
                let rotational_path = entry.path().join("queue/rotational");
                let is_rotational = std::fs::read_to_string(&rotational_path)
                    .ok()
                    .map(|s| s.trim() == "1")
                    .unwrap_or(true);

                disks.push(DiskInfo {
                    index: index as u32,
                    model,
                    size_bytes,
                    size_gb: size_bytes as f64 / 1_073_741_824.0,
                    media_type: if is_rotational {
                        "HDD".to_string()
                    } else {
                        "SSD".to_string()
                    },
                    interface_type: "SATA".to_string(),
                });
            }
        }

        Ok(disks)
    }

    #[cfg(target_os = "windows")]
    fn detect_network_adapters(&self) -> BitOSDTResult<Vec<NetworkAdapterInfo>> {
        let script =
            "Get-CimInstance -ClassName Win32_NetworkAdapter -Filter \"PhysicalAdapter = True\" | \
            Where-Object { $_.MACAddress -ne $null } | \
            Select-Object Name,MACAddress,AdapterType | \
            ConvertTo-Json -Compress";
        let raw = Self::run_powershell(script)?;
        let rows = Self::parse_json_rows(&raw)?;
        let mut adapters = Vec::new();

        for row in rows {
            adapters.push(NetworkAdapterInfo {
                name: Self::json_string(&row, "Name", "Unknown"),
                mac_address: Self::json_string(&row, "MACAddress", "00:00:00:00:00:00"),
                adapter_type: Self::json_string(&row, "AdapterType", "Unknown"),
            });
        }

        Ok(adapters)
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_network_adapters(&self) -> BitOSDTResult<Vec<NetworkAdapterInfo>> {
        use std::path::Path;

        let mut adapters = Vec::new();
        let net_dir = Path::new("/sys/class/net");

        if let Ok(entries) = std::fs::read_dir(net_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Skip loopback
                if name_str == "lo" {
                    continue;
                }

                // Get MAC address
                let addr_path = entry.path().join("address");
                let mac_address = std::fs::read_to_string(&addr_path)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "00:00:00:00:00:00".to_string());

                // Get type
                let type_path = entry.path().join("type");
                let adapter_type = std::fs::read_to_string(&type_path)
                    .ok()
                    .map(|s| {
                        match s.trim() {
                            "1" => "Ethernet",
                            "772" => "Loopback",
                            "803" => "IEEE80211",
                            _ => "Unknown",
                        }
                        .to_string()
                    })
                    .unwrap_or_else(|| "Unknown".to_string());

                adapters.push(NetworkAdapterInfo {
                    name: name_str.to_string(),
                    mac_address,
                    adapter_type,
                });
            }
        }

        Ok(adapters)
    }

    #[cfg(target_os = "windows")]
    fn detect_bios(&self) -> BitOSDTResult<BiosInfo> {
        Ok(BiosInfo {
            manufacturer: self.get_wmi_string("Win32_BIOS", "Manufacturer")?,
            version: self.get_wmi_string("Win32_BIOS", "Version")?,
            serial_number: self.get_wmi_string("Win32_BIOS", "SerialNumber")?,
            release_date: self.get_wmi_string("Win32_BIOS", "ReleaseDate")?,
            smbios_version: self.get_wmi_string("Win32_BIOS", "SMBIOSBIOSVersion")?,
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_bios(&self) -> BitOSDTResult<BiosInfo> {
        // Try to read from /sys/class/dmi
        let dmi_dir = std::path::Path::new("/sys/class/dmi/id");

        let read_dmi = |file: &str| -> String {
            std::fs::read_to_string(dmi_dir.join(file))
                .ok()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        };

        Ok(BiosInfo {
            manufacturer: read_dmi("bios_vendor"),
            version: read_dmi("bios_version"),
            serial_number: read_dmi("product_serial"),
            release_date: read_dmi("bios_date"),
            smbios_version: read_dmi("bios_version"),
        })
    }

    #[cfg(target_os = "windows")]
    fn detect_battery(&self) -> bool {
        let script = "(@(Get-CimInstance -ClassName Win32_Battery).Count).ToString()";
        Self::run_powershell(script)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(|count| count > 0)
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_battery(&self) -> bool {
        std::path::Path::new("/sys/class/power_supply/BAT0").exists()
    }

    #[cfg(target_os = "windows")]
    fn detect_tpm(&self) -> BitOSDTResult<TpmInfo> {
        let script = "Get-CimInstance -Namespace 'root\\cimv2\\Security\\MicrosoftTpm' -ClassName Win32_Tpm | \
            Select-Object -First 1 IsActivated_InitialValue,IsEnabled_InitialValue,IsOwned_InitialValue,SpecVersion | \
            ConvertTo-Json -Compress";
        let raw = Self::run_powershell(script)?;
        let rows = Self::parse_json_rows(&raw)?;
        let first = rows
            .into_iter()
            .next()
            .ok_or_else(|| BitOSDTError::HardwareDetection("No TPM device found".to_string()))?;

        Ok(TpmInfo {
            is_activated_initial_value: Self::json_bool(&first, "IsActivated_InitialValue", false),
            is_enabled_initial_value: Self::json_bool(&first, "IsEnabled_InitialValue", false),
            is_owned_initial_value: Self::json_bool(&first, "IsOwned_InitialValue", false),
            spec_version: Self::json_string(&first, "SpecVersion", "Unknown"),
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_tpm(&self) -> BitOSDTResult<TpmInfo> {
        Err(BitOSDTError::NotImplemented(
            "TPM not available on Linux".to_string(),
        ))
    }
}
