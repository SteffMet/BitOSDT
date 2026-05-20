#[cfg(target_os = "windows")]
use crate::core::errors::BitOSDTError;
use crate::core::errors::BitOSDTResult;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::process::Command;
use tracing::{info, warn};

pub struct BootManager;

impl Default for BootManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BootManager {
    pub fn new() -> Self {
        Self
    }

    /// Configure bootloader for deployed Windows
    pub fn configure_bootloader(
        &self,
        windows_partition: &Path,
        system_partition: &Path,
        uefi: bool,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            if uefi {
                self.configure_uefi_bootloader(windows_partition, system_partition)
            } else {
                self.configure_bios_bootloader(windows_partition, system_partition)
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("Bootloader configuration simulated on Linux");
            info!(
                "Would configure {} bootloader",
                if uefi { "UEFI" } else { "BIOS" }
            );
            info!("  Windows: {:?}", windows_partition);
            info!("  System: {:?}", system_partition);
            Ok(())
        }
    }

    #[cfg(target_os = "windows")]
    fn configure_uefi_bootloader(
        &self,
        windows_partition: &Path,
        system_partition: &Path,
    ) -> BitOSDTResult<()> {
        info!("Configuring UEFI bootloader...");

        // Pre-flight validation: Verify system partition exists and is accessible
        if !system_partition.exists() {
            return Err(BitOSDTError::Deployment(format!(
                "System partition {:?} does not exist or is not accessible. Cannot configure UEFI bootloader.",
                system_partition
            )));
        }

        info!(
            "Pre-flight check: System partition {:?} is accessible",
            system_partition
        );

        // Use bcdboot to configure UEFI bootloader
        let output = Command::new("bcdboot")
            .arg(windows_partition)
            .arg("/s")
            .arg(system_partition)
            .arg("/f")
            .arg("UEFI")
            .output()
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to run bcdboot: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BitOSDTError::Deployment(format!(
                "bcdboot failed: {}",
                stderr
            )));
        }

        // Post-flight validation: Verify boot files were created
        let boot_file = system_partition.join(r"EFI\Microsoft\Boot\bootmgfw.efi");
        if boot_file.exists() {
            info!("Post-flight check: Boot loader file {:?} exists", boot_file);
        } else {
            warn!("Post-flight check: Boot loader file {:?} not found - bootloader may not be configured correctly", boot_file);
        }

        info!("UEFI bootloader configured successfully");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn configure_bios_bootloader(
        &self,
        windows_partition: &Path,
        system_partition: &Path,
    ) -> BitOSDTResult<()> {
        info!("Configuring BIOS bootloader...");

        // Pre-flight validation: Verify system partition exists and is accessible
        if !system_partition.exists() {
            return Err(BitOSDTError::Deployment(format!(
                "System Reserved partition {:?} does not exist or is not accessible. Cannot configure BIOS bootloader.",
                system_partition
            )));
        }

        info!(
            "Pre-flight check: System partition {:?} is accessible",
            system_partition
        );

        // Use bcdboot to configure BIOS bootloader
        let output = Command::new("bcdboot")
            .arg(windows_partition)
            .arg("/s")
            .arg(system_partition)
            .arg("/f")
            .arg("BIOS")
            .output()
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to run bcdboot: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BitOSDTError::Deployment(format!(
                "bcdboot failed: {}",
                stderr
            )));
        }

        // Post-flight validation: Verify boot files were created
        let boot_file = system_partition.join(r"Boot\BCD");
        if boot_file.exists() {
            info!("Post-flight check: BCD file {:?} exists", boot_file);
        } else {
            warn!("Post-flight check: BCD file {:?} not found - bootloader may not be configured correctly", boot_file);
        }

        // Also mark system partition as active
        self.mark_partition_active(system_partition)?;

        info!("BIOS bootloader configured successfully");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn mark_partition_active(&self, partition_path: &Path) -> BitOSDTResult<()> {
        // Get drive letter from path
        let drive_letter = partition_path
            .to_str()
            .and_then(|s| s.chars().next())
            .ok_or_else(|| BitOSDTError::Deployment("Invalid partition path".to_string()))?;

        // Use diskpart to mark active
        use std::io::Write;

        let script = format!("select volume {}\nactive\nexit\n", drive_letter);

        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("bitosdt_activate.txt");

        let mut file = std::fs::File::create(&script_path)?;
        file.write_all(script.as_bytes())?;
        drop(file);

        let output = Command::new("diskpart")
            .arg("/s")
            .arg(&script_path)
            .output()
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to mark active: {}", e)))?;

        let _ = std::fs::remove_file(&script_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to mark partition active: {}", stderr);
        }

        Ok(())
    }

    /// Repair bootloader on existing Windows installation
    pub fn repair_bootloader(&self, windows_partition: &Path) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!("Repairing bootloader...");

            // Rebuild BCD
            let output = Command::new("bcdboot")
                .arg(windows_partition)
                .arg("/s")
                .arg(windows_partition)
                .output()
                .map_err(|e| {
                    BitOSDTError::Deployment(format!("Failed to repair bootloader: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::Deployment(format!(
                    "Bootloader repair failed: {}",
                    stderr
                )));
            }

            info!("Bootloader repaired successfully");
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = windows_partition;
            warn!("Bootloader repair simulated on Linux");
        }

        Ok(())
    }

    /// Set default boot entry
    pub fn set_default_boot_entry(&self, identifier: &str) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!("Setting default boot entry: {}", identifier);

            let output = Command::new("bcdedit")
                .args(&["/default", identifier])
                .output()
                .map_err(|e| BitOSDTError::Deployment(format!("Failed to set default: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::Deployment(format!(
                    "bcdedit failed: {}",
                    stderr
                )));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("Setting default boot entry simulated: {}", identifier);
        }

        Ok(())
    }

    /// Set boot timeout
    pub fn set_boot_timeout(&self, seconds: u32) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!("Setting boot timeout: {} seconds", seconds);

            let output = Command::new("bcdedit")
                .args(&["/timeout", &seconds.to_string()])
                .output()
                .map_err(|e| BitOSDTError::Deployment(format!("Failed to set timeout: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::Deployment(format!(
                    "bcdedit timeout failed: {}",
                    stderr
                )));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("Setting boot timeout simulated: {} seconds", seconds);
        }

        Ok(())
    }

    /// List boot entries
    pub fn list_boot_entries(&self) -> BitOSDTResult<Vec<BootEntry>> {
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("bcdedit")
                .args(&["/enum", "all"])
                .output()
                .map_err(|e| BitOSDTError::Deployment(format!("Failed to list entries: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::Deployment(format!(
                    "bcdedit enum failed: {}",
                    stderr
                )));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(self.parse_bcd_entries(&stdout))
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Return mock entries for development
            Ok(vec![BootEntry {
                identifier: "{default}".to_string(),
                description: "Windows 11".to_string(),
                device: "partition=C:".to_string(),
                path: "\\Windows\\system32\\winload.efi".to_string(),
                is_default: true,
            }])
        }
    }

    #[cfg(any(test, target_os = "windows"))]
    fn parse_bcd_entries(&self, output: &str) -> Vec<BootEntry> {
        let mut entries = Vec::new();
        let mut current_entry: Option<BootEntry> = None;

        for line in output.lines() {
            let line = line.trim();

            if line.starts_with("identifier") {
                if let Some(entry) = current_entry.take() {
                    entries.push(entry);
                }

                if let Some(id) = line.split_whitespace().nth(1) {
                    current_entry = Some(BootEntry {
                        identifier: id.to_string(),
                        description: String::new(),
                        device: String::new(),
                        path: String::new(),
                        is_default: id == "{default}",
                    });
                }
            } else if line.starts_with("description") {
                if let Some(ref mut entry) = current_entry {
                    entry.description = line
                        .split_whitespace()
                        .skip(1)
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            } else if line.starts_with("device") {
                if let Some(ref mut entry) = current_entry {
                    entry.device = line
                        .split_whitespace()
                        .skip(1)
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            } else if line.starts_with("path") {
                if let Some(ref mut entry) = current_entry {
                    entry.path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                }
            }
        }

        // Add last entry
        if let Some(entry) = current_entry {
            entries.push(entry);
        }

        entries
    }

    /// Disable driver signature enforcement
    pub fn disable_driver_signature(&self) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!("Disabling driver signature enforcement...");

            let output = Command::new("bcdedit")
                .args(&["/set", "nointegritychecks", "on"])
                .output()
                .map_err(|e| {
                    BitOSDTError::Deployment(format!("Failed to disable signature: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Could not disable driver signature: {}", stderr);
            } else {
                info!("Driver signature enforcement disabled");
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("Driver signature disable simulated on Linux");
        }

        Ok(())
    }

    /// Enable test signing mode
    pub fn enable_test_signing(&self, enable: bool) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            let value = if enable { "on" } else { "off" };
            info!("Setting test signing: {}", value);

            let output = Command::new("bcdedit")
                .args(&["/set", "testsigning", value])
                .output()
                .map_err(|e| {
                    BitOSDTError::Deployment(format!("Failed to set test signing: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Could not set test signing: {}", stderr);
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!(
                "Test signing {} simulated",
                if enable { "enabled" } else { "disabled" }
            );
        }

        Ok(())
    }
}

/// Boot entry information
#[derive(Debug, Clone)]
pub struct BootEntry {
    pub identifier: String,
    pub description: String,
    pub device: String,
    pub path: String,
    pub is_default: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bcd_entries_multiple() {
        let bm = BootManager::new();
        let output = r#"
Windows Boot Manager
--------------------
identifier              {bootmgr}
device                  partition=\Device\HarddiskVolume1
description             Windows Boot Manager
path                    \EFI\Microsoft\Boot\bootmgfw.efi

Windows Boot Loader
-------------------
identifier              {default}
device                  partition=C:
description             Windows 11
path                    \Windows\system32\winload.efi

Windows Boot Loader
-------------------
identifier              {12345678-1234-1234-1234-123456789abc}
device                  partition=D:
description             Windows 10 Recovery
path                    \Windows\system32\winload.efi
"#;

        let entries = bm.parse_bcd_entries(output);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].identifier, "{bootmgr}");
        assert_eq!(entries[0].description, "Windows Boot Manager");
        assert!(!entries[0].is_default);

        assert_eq!(entries[1].identifier, "{default}");
        assert_eq!(entries[1].description, "Windows 11");
        assert_eq!(entries[1].device, "partition=C:");
        assert!(entries[1].is_default);

        assert_eq!(
            entries[2].identifier,
            "{12345678-1234-1234-1234-123456789abc}"
        );
        assert_eq!(entries[2].description, "Windows 10 Recovery");
        assert_eq!(entries[2].device, "partition=D:");
        assert!(!entries[2].is_default);
    }

    #[test]
    fn test_parse_bcd_entries_empty() {
        let bm = BootManager::new();
        let entries = bm.parse_bcd_entries("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_boot_manager_default() {
        let bm = BootManager::default();
        // Verify Default trait works
        let _ = format!("{:?}", bm.list_boot_entries());
    }
}
