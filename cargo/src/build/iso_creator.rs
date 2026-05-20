use crate::core::adk::{resolve_adk_paths, AdkPaths};
use crate::core::errors::{BitOSDTError, BitOSDTResult};
#[cfg(target_os = "windows")]
use crate::core::run_tracked_command_streaming;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

pub struct IsoCreator;

impl IsoCreator {
    /// Create bootable ISO from WinPE directory
    pub fn create_iso(source_dir: &Path, output_iso: &Path, label: &str) -> BitOSDTResult<()> {
        let adk_paths = resolve_adk_paths(None, std::env::consts::ARCH);
        Self::create_iso_with_adk(source_dir, output_iso, label, adk_paths.as_ref())
    }

    pub fn create_iso_with_adk(
        source_dir: &Path,
        output_iso: &Path,
        label: &str,
        adk_paths: Option<&AdkPaths>,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            return Self::create_iso_windows(source_dir, output_iso, label, adk_paths);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = adk_paths;
            Self::create_iso_linux(source_dir, output_iso, label)
        }
    }

    #[cfg(target_os = "windows")]
    fn create_iso_windows(
        source_dir: &Path,
        output_iso: &Path,
        label: &str,
        adk_paths: Option<&AdkPaths>,
    ) -> BitOSDTResult<()> {
        let adk_paths = adk_paths.ok_or_else(|| {
            BitOSDTError::WinPE("Windows ADK paths were not resolved".to_string())
        })?;
        let oscdimg = adk_paths
            .oscdimg_exe
            .exists()
            .then_some(adk_paths.oscdimg_exe.clone())
            .ok_or_else(|| {
                BitOSDTError::WinPE("oscdimg.exe not found in detected ADK".to_string())
            })?;
        let bios_boot_sector = Self::resolve_bios_boot_sector(source_dir, adk_paths)?;
        let efi_boot_sector = Self::resolve_efi_boot_sector(source_dir, adk_paths)?;

        info!(
            "Creating ISO with oscdimg using BIOS boot sector {:?} and UEFI boot sector {:?}...",
            bios_boot_sector, efi_boot_sector
        );

        let bcd_bios = source_dir.join("boot").join("bcd");
        if bcd_bios.exists() {
            let _ = Command::new("bcdedit")
                .args([
                    "/store",
                    bcd_bios.to_str().unwrap(),
                    "/set",
                    "{default}",
                    "bootuxdisabled",
                    "on",
                ])
                .output();
        }

        let bcd_efi = source_dir
            .join("efi")
            .join("microsoft")
            .join("boot")
            .join("bcd");
        if bcd_efi.exists() {
            let _ = Command::new("bcdedit")
                .args([
                    "/store",
                    bcd_efi.to_str().unwrap(),
                    "/set",
                    "{default}",
                    "bootuxdisabled",
                    "on",
                ])
                .output();
        }

        let args = vec![
            "-m".to_string(),
            "-o".to_string(),
            "-u2".to_string(),
            "-udfver102".to_string(),
            format!("-l{}", label),
            format!(
                "-bootdata:2#p0,e,b{}#pEF,e,b{}",
                bios_boot_sector.display(),
                efi_boot_sector.display()
            ),
            source_dir.display().to_string(),
            output_iso.display().to_string(),
        ];
        let mut command = Command::new(&oscdimg);
        command.args(&args);
        let output = run_tracked_command_streaming(command, &oscdimg, &args, "iso-create", |_| {})
            .map_err(|e| BitOSDTError::WinPE(format!("Failed to run oscdimg: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Err(BitOSDTError::WinPE(format!(
                "oscdimg failed (exit={:?}, stdout={}, stderr={})",
                output.status.code(),
                if stdout.is_empty() {
                    "<empty>"
                } else {
                    &stdout
                },
                if stderr.is_empty() {
                    "<empty>"
                } else {
                    &stderr
                }
            )));
        }

        info!("ISO created: {:?}", output_iso);
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn resolve_first_existing_path(
        candidates: Vec<PathBuf>,
        label: &str,
    ) -> BitOSDTResult<PathBuf> {
        if let Some(found) = candidates.iter().find(|p| p.exists()).cloned() {
            return Ok(found);
        }

        let searched = candidates
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        Err(BitOSDTError::WinPE(format!(
            "Required boot file '{}' was not found. Searched:\n{}",
            label, searched
        )))
    }

    #[cfg(target_os = "windows")]
    fn resolve_bios_boot_sector(source_dir: &Path, adk_paths: &AdkPaths) -> BitOSDTResult<PathBuf> {
        let source_parent = source_dir.parent().unwrap_or(source_dir);
        Self::resolve_first_existing_path(
            vec![
                source_dir.join("etfsboot.com"),
                source_dir.join("boot").join("etfsboot.com"),
                source_dir.join("Boot").join("etfsboot.com"),
                source_parent.join("bootbins").join("etfsboot.com"),
                adk_paths
                    .winpe_root
                    .join("Media")
                    .join("Boot")
                    .join("etfsboot.com"),
                adk_paths
                    .winpe_root
                    .join("media")
                    .join("boot")
                    .join("etfsboot.com"),
            ],
            "etfsboot.com",
        )
    }

    #[cfg(target_os = "windows")]
    fn resolve_efi_boot_sector(source_dir: &Path, adk_paths: &AdkPaths) -> BitOSDTResult<PathBuf> {
        let source_parent = source_dir.parent().unwrap_or(source_dir);
        Self::resolve_first_existing_path(
            vec![
                source_dir
                    .join("efi")
                    .join("microsoft")
                    .join("boot")
                    .join("efisys.bin"),
                source_dir
                    .join("EFI")
                    .join("Microsoft")
                    .join("Boot")
                    .join("efisys.bin"),
                source_parent.join("bootbins").join("efisys.bin"),
                adk_paths
                    .winpe_root
                    .join("Media")
                    .join("EFI")
                    .join("Microsoft")
                    .join("Boot")
                    .join("efisys.bin"),
                adk_paths.winpe_root.join("fwfiles").join("efisys.bin"),
            ],
            "efisys.bin",
        )
    }

    #[cfg(not(target_os = "windows"))]
    fn create_iso_linux(source_dir: &Path, output_iso: &Path, label: &str) -> BitOSDTResult<()> {
        info!("Creating ISO with xorriso...");

        let xorriso_check = Command::new("which").arg("xorriso").output()?;

        if xorriso_check.status.success() {
            let output = Command::new("xorriso")
                .args([
                    // Treat xorriso WARNING/SORRY/MISHAP conditions as non-fatal so
                    // Linux builds succeed when the ISO is still produced successfully.
                    "-return_with",
                    "FAILURE",
                    "32",
                    "-as",
                    "mkisofs",
                    "-iso-level",
                    "3",
                    "-full-iso9660-filenames",
                    "-volid",
                    label,
                    "-eltorito-boot",
                    "boot/etfsboot.com",
                    "-no-emul-boot",
                    "-boot-load-size",
                    "8",
                    "-boot-info-table",
                    "-eltorito-alt-boot",
                    "-e",
                    "efi/microsoft/boot/efisys.bin",
                    "-no-emul-boot",
                    "-isohybrid-gpt-basdat",
                    "-o",
                ])
                .arg(output_iso)
                .arg(source_dir)
                .output()
                .map_err(|e| BitOSDTError::WinPE(format!("Failed to run xorriso: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::WinPE(format!("xorriso failed: {}", stderr)));
            }
        } else {
            return Err(BitOSDTError::NotImplemented(
                "Linux ISO creation requires xorriso to be installed".to_string(),
            ));
        }

        info!("ISO created: {:?}", output_iso);
        Ok(())
    }
}
