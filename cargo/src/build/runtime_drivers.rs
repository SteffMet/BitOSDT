use crate::build::WinPEBuilder;
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::{DriverPack, RuntimeDriverContext, RuntimeDriverPolicy};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RuntimeDriverAssetConfig {
    pub policy: RuntimeDriverPolicy,
    pub context: RuntimeDriverContext,
    pub catalog: Vec<DriverPack>,
    pub cache_source: Option<PathBuf>,
}

pub fn stage_runtime_driver_assets(
    mount_dir: &Path,
    winpe_builder: &WinPEBuilder,
    config: &RuntimeDriverAssetConfig,
) -> BitOSDTResult<()> {
    if !config.policy.enabled {
        return Ok(());
    }

    let catalog_path = context_path(
        &config.context.embedded_catalog_path,
        r"X:\BitOSDT\Config\driverpacks.json",
    )?;
    let catalog_mount_path = winpe_mount_path(mount_dir, &catalog_path);
    if let Some(parent) = catalog_mount_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &catalog_mount_path,
        serde_json::to_string_pretty(&config.catalog)?,
    )?;

    if let Some(cache_source) = config.cache_source.as_ref().filter(|path| path.is_dir()) {
        let staged_cache_path =
            context_path(&config.context.staged_cache_path, r"X:\BitOSDT\DriverCache")?;
        let destination = winpe_mount_path(mount_dir, &staged_cache_path);
        fs::create_dir_all(&destination)?;
        copy_directory_contents(cache_source, &destination)?;

        if config.policy.bundle_common_boot_drivers {
            if let Some(common_source) = resolve_common_boot_driver_source(cache_source) {
                let common_dest = context_path(
                    &config.context.common_boot_driver_directory,
                    r"X:\BitOSDT\DriverCache\common-boot",
                )?;
                let destination = winpe_mount_path(mount_dir, &common_dest);
                fs::create_dir_all(&destination)?;
                copy_directory_contents(&common_source, &destination)?;
                winpe_builder.add_drivers(mount_dir, &common_source)?;
            }
        }
    }

    Ok(())
}

fn context_path(path: &Option<PathBuf>, fallback: &str) -> BitOSDTResult<PathBuf> {
    let resolved = path
        .clone()
        .unwrap_or_else(|| PathBuf::from(fallback))
        .components()
        .collect::<PathBuf>();

    if resolved.as_os_str().is_empty() {
        return Err(BitOSDTError::Validation(
            "Runtime driver context path resolved to an empty value".to_string(),
        ));
    }

    Ok(resolved)
}

fn winpe_mount_path(mount_dir: &Path, winpe_path: &Path) -> PathBuf {
    let mut relative = PathBuf::new();
    let normalized = winpe_path.to_string_lossy().replace('\\', "/");
    let trimmed = normalized
        .strip_prefix("X:/")
        .or_else(|| normalized.strip_prefix("x:/"))
        .unwrap_or(&normalized)
        .trim_start_matches('/');

    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            relative.push("..");
            continue;
        }
        relative.push(segment);
    }
    mount_dir.join(relative)
}

fn resolve_common_boot_driver_source(cache_source: &Path) -> Option<PathBuf> {
    let candidates = [
        "common-boot",
        "common_boot",
        "boot",
        "base",
        "boot-critical",
    ];

    candidates
        .iter()
        .map(|segment| cache_source.join(segment))
        .find(|path| path.is_dir())
}

fn copy_directory_contents(source: &Path, destination: &Path) -> BitOSDTResult<()> {
    fs::create_dir_all(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_directory_contents(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winpe_mount_path_maps_x_drive_to_mount_directory() {
        let mount = PathBuf::from(r"C:\mount");
        let path = winpe_mount_path(&mount, Path::new(r"X:\BitOSDT\Config\driverpacks.json"));
        assert_eq!(
            path,
            mount
                .join("BitOSDT")
                .join("Config")
                .join("driverpacks.json")
        );
    }
}
