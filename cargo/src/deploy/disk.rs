use crate::core::errors::{BitOSDTError, BitOSDTResult};
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::process::Command;
use tracing::{info, warn};

/// Represents a disk partition
#[derive(Debug, Clone)]
pub struct Partition {
    pub number: u32,
    pub size_bytes: u64,
    pub filesystem: String,
    pub label: String,
    pub drive_letter: Option<String>,
    pub is_boot: bool,
    pub disk_index: u32,
}

/// Disk manager for partitioning and formatting
pub struct DiskManager {
    disk_index: u32,
}

impl DiskManager {
    pub fn new(disk_index: u32) -> Self {
        Self { disk_index }
    }

    /// Initialize disk (clean and prepare for Windows)
    pub fn initialize_disk(&self, uefi: bool) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            self.initialize_disk_windows(uefi)
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.initialize_disk_linux(uefi)
        }
    }

    #[cfg(target_os = "windows")]
    fn initialize_disk_windows(&self, uefi: bool) -> BitOSDTResult<()> {
        info!("Initializing disk {} (UEFI: {})", self.disk_index, uefi);

        // Clean the disk
        self.run_diskpart(&format!(
            "select disk {}\nclean\nconvert {}",
            self.disk_index,
            if uefi { "gpt" } else { "mbr" }
        ))?;

        // Create partitions
        if uefi {
            // UEFI layout: EFI System, MSR, Windows, Recovery
            self.create_uefi_partitions()?;
        } else {
            // BIOS layout: System Reserved, Windows
            self.create_bios_partitions()?;
        }

        info!("Disk {} initialized successfully", self.disk_index);
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn initialize_disk_linux(&self, uefi: bool) -> BitOSDTResult<()> {
        // For development/testing - simulate on Linux
        warn!("Disk initialization simulated on Linux");
        info!(
            "Would initialize disk {} with {} partitioning",
            self.disk_index,
            if uefi { "GPT/UEFI" } else { "MBR/BIOS" }
        );
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn create_uefi_partitions(&self) -> BitOSDTResult<()> {
        // EFI System Partition (100MB)
        self.run_diskpart(&format!(
            "select disk {}\n\
             create partition efi size=100\n\
             format fs=fat32 quick label=\"System\"\n\
             assign letter=S",
            self.disk_index
        ))?;

        // Microsoft Reserved Partition (16MB)
        self.run_diskpart(&format!(
            "select disk {}\n\
             create partition msr size=16",
            self.disk_index
        ))?;

        // Windows partition (remaining space - 500MB for recovery)
        // First create with all space, then shrink to make room for recovery at the end
        self.run_diskpart(&format!(
            "select disk {}\n\
             create partition primary\n\
             shrink desired=500 minimum=500\n\
             format fs=ntfs quick label=\"Windows\"\n\
             assign letter=W",
            self.disk_index
        ))?;

        // Recovery partition (use remaining 500MB at end)
        self.run_diskpart(&format!(
            "select disk {}\n\
             create partition primary\n\
             format fs=ntfs quick label=\"Recovery\"\n\
             set id=de94bba4-06d1-4d40-a16a-bfd50179d6ac\n\
             gpt attributes=0x8000000000000001",
            self.disk_index
        ))?;

        // Verify that critical partitions have correct drive letters
        self.verify_uefi_partition_letters()?;

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn create_bios_partitions(&self) -> BitOSDTResult<()> {
        // System Reserved partition (350MB)
        self.run_diskpart(&format!(
            "select disk {}\n\
             create partition primary size=350\n\
             format fs=ntfs quick label=\"System\"\n\
             assign letter=S\n\
             active",
            self.disk_index
        ))?;

        // Windows partition (remaining space)
        self.run_diskpart(&format!(
            "select disk {}\n\
             create partition primary\n\
             format fs=ntfs quick label=\"Windows\"\n\
             assign letter=W",
            self.disk_index
        ))?;

        Ok(())
    }

    /// Verify that UEFI partition drive letters are correctly assigned
    #[cfg(target_os = "windows")]
    fn verify_uefi_partition_letters(&self) -> BitOSDTResult<()> {
        info!("Verifying UEFI partition drive letters...");

        // Check if S: (EFI) and W: (Windows) exist
        let efi_path = PathBuf::from(r"S:\");
        let windows_path = PathBuf::from(r"W:\");

        if !efi_path.exists() {
            return Err(BitOSDTError::Deployment(
                "EFI System Partition (S:) not found after creation. Boot files will not be accessible.".to_string()
            ));
        }

        if !windows_path.exists() {
            return Err(BitOSDTError::Deployment(
                "Windows partition (W:) not found after creation. Cannot proceed with deployment."
                    .to_string(),
            ));
        }

        info!("UEFI partition verification successful: S: (EFI) and W: (Windows) are accessible");
        Ok(())
    }

    /// Run diskpart script
    #[cfg(target_os = "windows")]
    fn run_diskpart(&self, script: &str) -> BitOSDTResult<()> {
        use std::io::Write;

        // Create temporary script file
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join("bitosdt_diskpart.txt");

        let mut file = std::fs::File::create(&script_path)?;
        file.write_all(script.as_bytes())?;
        drop(file);

        info!("Running diskpart script: {}", script.replace('\n', "; "));

        // Execute diskpart
        let output = Command::new("diskpart")
            .arg("/s")
            .arg(&script_path)
            .output()
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to run diskpart: {}", e)))?;

        // Clean up script
        let _ = std::fs::remove_file(&script_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BitOSDTError::Deployment(format!(
                "Diskpart failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    /// Get partition information
    pub fn get_partitions(&self) -> BitOSDTResult<Vec<Partition>> {
        #[cfg(target_os = "windows")]
        {
            self.get_partitions_windows()
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.get_partitions_linux()
        }
    }

    #[cfg(target_os = "windows")]
    fn get_partitions_windows(&self) -> BitOSDTResult<Vec<Partition>> {
        // Use diskpart to list partitions
        let output = Command::new("diskpart")
            .args(&["/s", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(
                        format!("select disk {}\nlist partition\nexit\n", self.disk_index)
                            .as_bytes(),
                    )?;
                }
                child.wait_with_output()
            })
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to list partitions: {}", e)))?;

        // Parse output (simplified)
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut partitions = Vec::new();

        for line in stdout.lines() {
            // Parse partition info from diskpart output
            // Format: Partition ###  Type              Size     Offset
            if line.trim().starts_with("Partition") {
                // Simplified parsing - would need full implementation
                if let Some(num_str) = line.split_whitespace().nth(1) {
                    if let Ok(num) = num_str.parse::<u32>() {
                        partitions.push(Partition {
                            number: num,
                            size_bytes: 0,
                            filesystem: "Unknown".to_string(),
                            label: String::new(),
                            drive_letter: None,
                            is_boot: false,
                            disk_index: self.disk_index,
                        });
                    }
                }
            }
        }

        Ok(partitions)
    }

    #[cfg(not(target_os = "windows"))]
    fn get_partitions_linux(&self) -> BitOSDTResult<Vec<Partition>> {
        // Mock partitions for development
        Ok(vec![
            Partition {
                number: 1,
                size_bytes: 104_857_600, // 100MB
                filesystem: "FAT32".to_string(),
                label: "System".to_string(),
                drive_letter: Some("S".to_string()),
                is_boot: true,
                disk_index: self.disk_index,
            },
            Partition {
                number: 3,
                size_bytes: 100_000_000_000, // ~100GB
                filesystem: "NTFS".to_string(),
                label: "Windows".to_string(),
                drive_letter: Some("W".to_string()),
                is_boot: false,
                disk_index: self.disk_index,
            },
        ])
    }

    /// Wipe disk securely
    pub fn wipe_disk(&self, secure: bool) -> BitOSDTResult<()> {
        if secure {
            info!(
                "Secure wiping disk {} - this may take a while...",
                self.disk_index
            );
            // Would implement DoD 5220.22-M or similar
            warn!("Secure wipe not yet implemented");
        } else {
            info!("Quick wiping disk {}...", self.disk_index);
        }

        #[cfg(target_os = "windows")]
        {
            self.run_diskpart(&format!("select disk {}\nclean all", self.disk_index))?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            warn!("Disk wipe simulated on Linux");
        }

        Ok(())
    }

    /// Get Windows partition path
    pub fn get_windows_partition(&self) -> BitOSDTResult<PathBuf> {
        let partitions = self.get_partitions()?;

        for part in partitions {
            if part.label == "Windows" || part.label == "OS" {
                if let Some(letter) = part.drive_letter {
                    return Ok(PathBuf::from(format!("{}:\\", letter)));
                }
            }
        }

        Err(BitOSDTError::Deployment(
            "Windows partition not found".to_string(),
        ))
    }

    /// Verify partitions after initialization (public interface)
    pub fn verify_partitions(&self, uefi: bool) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            if uefi {
                self.verify_uefi_partition_letters()
            } else {
                // For BIOS, verify S: (System Reserved) and W: (Windows)
                let system_path = PathBuf::from(r"S:\");
                let windows_path = PathBuf::from(r"W:\");

                if !system_path.exists() {
                    return Err(BitOSDTError::Deployment(
                        "System Reserved partition (S:) not found after creation.".to_string(),
                    ));
                }

                if !windows_path.exists() {
                    return Err(BitOSDTError::Deployment(
                        "Windows partition (W:) not found after creation.".to_string(),
                    ));
                }

                info!("BIOS partition verification successful: S: (System) and W: (Windows) are accessible");
                Ok(())
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = uefi;
            warn!("Partition verification simulated on Linux");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_struct() {
        let part = Partition {
            number: 1,
            size_bytes: 104_857_600,
            filesystem: "FAT32".to_string(),
            label: "System".to_string(),
            drive_letter: Some("S".to_string()),
            is_boot: true,
            disk_index: 0,
        };
        assert_eq!(part.number, 1);
        assert_eq!(part.size_bytes, 104_857_600);
        assert!(part.is_boot);
        assert_eq!(part.drive_letter.as_deref(), Some("S"));
    }

    #[test]
    fn test_disk_manager_creation() {
        let dm = DiskManager::new(0);
        assert_eq!(dm.disk_index, 0);

        let dm2 = DiskManager::new(3);
        assert_eq!(dm2.disk_index, 3);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_get_partitions_linux_mock() {
        let dm = DiskManager::new(0);
        let partitions = dm.get_partitions().unwrap();

        // Linux mock returns 2 partitions
        assert_eq!(partitions.len(), 2);

        // First partition should be the boot/System partition
        assert_eq!(partitions[0].label, "System");
        assert!(partitions[0].is_boot);
        assert_eq!(partitions[0].filesystem, "FAT32");

        // Second partition should be Windows
        assert_eq!(partitions[1].label, "Windows");
        assert!(!partitions[1].is_boot);
        assert_eq!(partitions[1].filesystem, "NTFS");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_get_windows_partition_linux() {
        let dm = DiskManager::new(0);
        let win_path = dm.get_windows_partition().unwrap();
        assert_eq!(win_path, PathBuf::from("W:\\"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_initialize_disk_linux() {
        let dm = DiskManager::new(0);
        // Linux mock should succeed for both UEFI and BIOS
        assert!(dm.initialize_disk(true).is_ok());
        assert!(dm.initialize_disk(false).is_ok());
    }
}
