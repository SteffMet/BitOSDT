use crate::core::errors::{BitOSDTError, BitOSDTResult};
use std::path::Path;
#[cfg(not(target_os = "windows"))]
use std::process::Command;
use tracing::info;
#[cfg(target_os = "windows")]
use tracing::warn;

pub struct UsbWriter;

impl UsbWriter {
    /// Write ISO to USB drive
    pub fn write_iso_to_usb(iso_path: &Path, target_disk: &str) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            Self::write_windows(iso_path, target_disk)
        }

        #[cfg(not(target_os = "windows"))]
        {
            Self::write_linux(iso_path, target_disk)
        }
    }

    #[cfg(target_os = "windows")]
    fn write_windows(iso_path: &Path, target_disk: &str) -> BitOSDTResult<()> {
        // Use dd or a Windows tool like Rufus CLI or diskpart
        info!("Writing {} to disk {}...", iso_path.display(), target_disk);

        // For Windows, we'd typically use a tool like dd for Windows or Win32DiskImager
        // This is a placeholder for the actual implementation
        warn!("USB writing on Windows requires administrative privileges");
        warn!("Target disk: {}", target_disk);
        warn!("ISO: {}", iso_path.display());

        Err(BitOSDTError::WinPE(
            "USB writing on Windows not yet implemented".to_string(),
        ))
    }

    #[cfg(not(target_os = "windows"))]
    fn write_linux(iso_path: &Path, target_disk: &str) -> BitOSDTResult<()> {
        // Validate target disk
        if !target_disk.starts_with("/dev/") {
            return Err(BitOSDTError::InvalidInput(
                "Target must be a device path (e.g., /dev/sdb)".to_string(),
            ));
        }

        // Safety check: ensure it's not a system disk
        if Self::is_system_disk(target_disk)? {
            return Err(BitOSDTError::InvalidInput(
                "Refusing to write to system disk".to_string(),
            ));
        }

        info!("Writing {} to {}...", iso_path.display(), target_disk);
        info!("This may take several minutes...");

        // Use dd to write ISO
        let input_arg = format!("if={}", iso_path.to_string_lossy());
        let output_arg = format!("of={}", target_disk);
        let output = Command::new("dd")
            .args([
                input_arg.as_str(),
                output_arg.as_str(),
                "bs=4M",
                "status=progress",
                "conv=fsync",
            ])
            .output()
            .map_err(BitOSDTError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BitOSDTError::WinPE(format!("dd failed: {}", stderr)));
        }

        info!("USB write complete: {}", target_disk);
        Ok(())
    }

    /// Check if disk is a system disk
    #[cfg(not(target_os = "windows"))]
    fn is_system_disk(disk: &str) -> BitOSDTResult<bool> {
        // Get mounted root device
        let output = Command::new("findmnt")
            .args(["-n", "-o", "SOURCE", "/"])
            .output()?;

        if output.status.success() {
            let root_device = String::from_utf8_lossy(&output.stdout).trim().to_string();

            // Check if target disk contains root device
            if root_device.starts_with(disk) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// List available USB devices
    pub fn list_usb_devices() -> BitOSDTResult<Vec<UsbDevice>> {
        #[allow(unused_mut)]
        let mut devices = Vec::new();

        #[cfg(not(target_os = "windows"))]
        {
            // List block devices
            let output = Command::new("lsblk")
                .args(["-d", "-o", "NAME,SIZE,TYPE,VENDOR,MODEL", "-n", "-p"])
                .output()?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);

                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let path = parts[0];
                        let size = parts.get(1).unwrap_or(&"Unknown");

                        // Only include USB and disk devices
                        if path.starts_with("/dev/sd") || path.starts_with("/dev/nvme") {
                            devices.push(UsbDevice {
                                path: path.to_string(),
                                size: size.to_string(),
                                model: parts.get(4).unwrap_or(&"Unknown").to_string(),
                                vendor: parts.get(3).unwrap_or(&"Unknown").to_string(),
                            });
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // On Windows, use wmic or PowerShell to list disks
            warn!("USB device listing on Windows not yet implemented");
        }

        Ok(devices)
    }
}

#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub path: String,
    pub size: String,
    pub model: String,
    pub vendor: String,
}

impl std::fmt::Display for UsbDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} - {} {})",
            self.path, self.size, self.vendor, self.model
        )
    }
}
