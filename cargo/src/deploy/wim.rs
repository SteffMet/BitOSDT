#[cfg(not(target_os = "windows"))]
use crate::build::linux_support::{apply_wim_image, export_image_to_wim, read_wim_info};
#[cfg(target_os = "windows")]
use crate::core::adk::resolve_adk_paths_from_env;
#[cfg(target_os = "windows")]
use crate::core::errors::BitOSDTError;
use crate::core::errors::BitOSDTResult;
#[cfg(target_os = "windows")]
use crate::core::windows_tools::{
    dism_path_arg, format_process_failure, resolve_dism_exe, run_dism, run_dism_streaming_with_role,
};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::time::Instant;
use tracing::info;
#[cfg(not(target_os = "windows"))]
use tracing::warn;

pub struct WimManager;

impl Default for WimManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WimManager {
    #[cfg(any(test, target_os = "windows"))]
    fn parse_dism_progress_percent(line: &str) -> Option<u8> {
        let percent_index = line.find('%')?;
        let bytes = line.as_bytes();
        let mut start = percent_index;

        while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
            start -= 1;
        }

        let token = line[start..percent_index].trim().trim_end_matches('.');
        let integer = token.split('.').next()?.trim().parse::<u8>().ok()?;
        Some(integer.min(100))
    }

    pub fn new() -> Self {
        Self
    }

    /// Apply WIM image to target directory
    pub fn apply_wim(
        &self,
        wim_path: &Path,
        image_index: u32,
        target_path: &Path,
        progress_callback: Option<&dyn Fn(u64, u64)>,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            self.apply_wim_windows(wim_path, image_index, target_path, progress_callback)
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.apply_wim_linux(wim_path, image_index, target_path, progress_callback)
        }
    }

    #[cfg(target_os = "windows")]
    fn apply_wim_windows(
        &self,
        wim_path: &Path,
        image_index: u32,
        target_path: &Path,
        _progress_callback: Option<&dyn Fn(u64, u64)>,
    ) -> BitOSDTResult<()> {
        info!(
            "Applying WIM: {:?} [Index {}] -> {:?}",
            wim_path, image_index, target_path
        );

        if !wim_path.exists() {
            return Err(BitOSDTError::Deployment(format!(
                "WIM file not found: {:?}",
                wim_path
            )));
        }

        // Ensure target exists
        std::fs::create_dir_all(target_path)?;

        // Apply WIM using DISM
        let adk_paths = resolve_adk_paths_from_env(std::env::consts::ARCH);
        let args = vec![
            "/Apply-Image".to_string(),
            dism_path_arg("/ImageFile", wim_path),
            format!("/Index:{}", image_index),
            dism_path_arg("/ApplyDir", target_path),
        ];

        let apply_started_at = Instant::now();
        let output = run_dism(&args, adk_paths.as_ref())
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to apply WIM: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BitOSDTError::Deployment(format!(
                "DISM apply failed: {}",
                stderr
            )));
        }

        info!(
            "WIM applied successfully to {:?} in {:.2}s",
            target_path,
            apply_started_at.elapsed().as_secs_f64()
        );
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn apply_wim_linux(
        &self,
        wim_path: &Path,
        image_index: u32,
        target_path: &Path,
        _progress_callback: Option<&dyn Fn(u64, u64)>,
    ) -> BitOSDTResult<()> {
        info!(
            "Applying WIM on Linux via wimlib-imagex: {:?} [{}] -> {:?}",
            wim_path, image_index, target_path
        );
        apply_wim_image(wim_path, image_index, target_path)
    }

    /// Get information about WIM file
    pub fn get_wim_info(&self, wim_path: &Path) -> BitOSDTResult<WimInfo> {
        #[cfg(target_os = "windows")]
        {
            self.get_wim_info_windows(wim_path)
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.get_wim_info_linux(wim_path)
        }
    }

    #[cfg(target_os = "windows")]
    fn get_wim_info_windows(&self, wim_path: &Path) -> BitOSDTResult<WimInfo> {
        let adk_paths = resolve_adk_paths_from_env(std::env::consts::ARCH);
        let args = vec![
            "/Get-WimInfo".to_string(),
            dism_path_arg("/WimFile", wim_path),
        ];

        let output = run_dism(&args, adk_paths.as_ref())
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to get WIM info: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BitOSDTError::Deployment(format!(
                "DISM get info failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse DISM output
        let mut images = Vec::new();
        let mut index = String::new();
        let mut name = String::new();
        let size = 0u64;

        for line in stdout.lines() {
            let line = line.trim();

            if line.starts_with("Index :") {
                if !index.is_empty() {
                    images.push(WimImage {
                        index: index.parse().unwrap_or(1),
                        name: name.clone(),
                        description: String::new(),
                        size_bytes: size,
                    });
                }
                index = line.split(':').nth(1).unwrap_or("1").trim().to_string();
            } else if line.starts_with("Name :") {
                name = line
                    .split(':')
                    .nth(1)
                    .unwrap_or("Unknown")
                    .trim()
                    .to_string();
            }
        }

        // Add last image
        if !index.is_empty() {
            images.push(WimImage {
                index: index.parse().unwrap_or(1),
                name,
                description: String::new(),
                size_bytes: size,
            });
        }

        Ok(WimInfo {
            path: wim_path.to_path_buf(),
            images,
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn get_wim_info_linux(&self, wim_path: &Path) -> BitOSDTResult<WimInfo> {
        read_wim_info(wim_path)
    }

    /// Find image index by name in WIM
    pub fn find_image_index(&self, wim_info: &WimInfo, image_name: &str) -> Option<u32> {
        for image in &wim_info.images {
            if image
                .name
                .to_lowercase()
                .contains(&image_name.to_lowercase())
            {
                return Some(image.index);
            }
        }
        None
    }

    /// Capture partition to WIM
    pub fn capture_wim(
        &self,
        source_path: &Path,
        wim_path: &Path,
        name: &str,
        description: &str,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!("Capturing WIM: {:?} -> {:?}", source_path, wim_path);
            let adk_paths = resolve_adk_paths_from_env(std::env::consts::ARCH);

            let args = vec![
                "/Capture-Image".to_string(),
                dism_path_arg("/ImageFile", wim_path),
                dism_path_arg("/CaptureDir", source_path),
                format!("/Name:{}", name),
                format!("/Description:{}", description),
                "/Compress:max".to_string(),
            ];

            let output = run_dism(&args, adk_paths.as_ref())
                .map_err(|e| BitOSDTError::Deployment(format!("Failed to capture WIM: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::Deployment(format!(
                    "DISM capture failed: {}",
                    stderr
                )));
            }

            info!("WIM captured successfully: {:?}", wim_path);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (name, description);
            warn!("WIM capture simulated on Linux");
            info!("Would capture: {:?} -> {:?}", source_path, wim_path);
        }

        Ok(())
    }

    /// Export image from one WIM to another
    pub fn export_wim(
        &self,
        source_wim: &Path,
        source_index: u32,
        dest_wim: &Path,
    ) -> BitOSDTResult<()> {
        self.export_wim_with_progress(source_wim, source_index, dest_wim, "wim-export", |_, _| {})
    }

    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    pub fn export_wim_with_progress<F>(
        &self,
        source_wim: &Path,
        source_index: u32,
        dest_wim: &Path,
        role: &str,
        mut progress_callback: F,
    ) -> BitOSDTResult<()>
    where
        F: FnMut(u8, String),
    {
        #[cfg(target_os = "windows")]
        {
            info!(
                "Exporting WIM: {:?}[{}] -> {:?}",
                source_wim, source_index, dest_wim
            );
            if source_wim != dest_wim && dest_wim.exists() {
                std::fs::remove_file(dest_wim)?;
            }
            let adk_paths = resolve_adk_paths_from_env(std::env::consts::ARCH);

            let args = vec![
                "/Export-Image".to_string(),
                dism_path_arg("/SourceImageFile", source_wim),
                format!("/SourceIndex:{}", source_index),
                dism_path_arg("/DestinationImageFile", dest_wim),
                "/Compress:max".to_string(),
            ];

            let mut last_progress = 5u8;
            progress_callback(last_progress, "Starting DISM image export...".to_string());
            let output = run_dism_streaming_with_role(&args, adk_paths.as_ref(), role, |line| {
                let message = if let Some(percent) = Self::parse_dism_progress_percent(&line) {
                    let scaled = 5 + ((percent as u32 * 90 / 100) as u8);
                    last_progress = last_progress.max(scaled.min(95));
                    format!("DISM export progress: {}", line)
                } else {
                    format!("DISM export output: {}", line)
                };
                progress_callback(last_progress, message);
            })
            .map_err(|e| BitOSDTError::Deployment(format!("Failed to export WIM: {}", e)))?;

            if !output.status.success() {
                let dism_exe = resolve_dism_exe(adk_paths.as_ref());
                return Err(BitOSDTError::Deployment(format!(
                    "DISM export failed: {}",
                    format_process_failure(&dism_exe, &args, &output)
                )));
            }

            progress_callback(100, "DISM image export complete.".to_string());
            info!("WIM exported successfully");
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (&role, &progress_callback);
            info!(
                "Exporting WIM on Linux via wimlib-imagex: {:?}[{}] -> {:?}",
                source_wim, source_index, dest_wim
            );
            if source_wim != dest_wim && dest_wim.exists() {
                std::fs::remove_file(dest_wim)?;
            }
            export_image_to_wim(source_wim, Some(source_index), dest_wim)?;
        }

        Ok(())
    }

    /// Split WIM into smaller parts
    pub fn split_wim(
        &self,
        source_wim: &Path,
        dest_wim: &Path,
        file_size_mb: u32,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!(
                "Splitting WIM: {:?} -> {:?} ({}MB chunks)",
                source_wim, dest_wim, file_size_mb
            );
            let adk_paths = resolve_adk_paths_from_env(std::env::consts::ARCH);

            let args = vec![
                "/Split-Image".to_string(),
                dism_path_arg("/ImageFile", source_wim),
                dism_path_arg("/SWMFile", dest_wim),
                format!("/FileSize:{}", file_size_mb),
            ];

            let output = run_dism(&args, adk_paths.as_ref())
                .map_err(|e| BitOSDTError::Deployment(format!("Failed to split WIM: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BitOSDTError::Deployment(format!(
                    "DISM split failed: {}",
                    stderr
                )));
            }

            info!("WIM split successfully");
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (source_wim, dest_wim, file_size_mb);
            warn!("WIM split simulated on Linux");
        }

        Ok(())
    }
}

fn normalize_image_name_for_matching(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut previous_was_space = true;

    for character in name.chars() {
        let mapped = if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            ' '
        };

        if mapped == ' ' {
            if !previous_was_space {
                normalized.push(mapped);
            }
            previous_was_space = true;
        } else {
            normalized.push(mapped);
            previous_was_space = false;
        }
    }

    normalized.trim().to_string()
}

fn contains_image_phrase(normalized_name: &str, phrase: &str) -> bool {
    let padded_name = format!(" {} ", normalized_name);
    let padded_phrase = format!(" {} ", phrase);
    padded_name.contains(&padded_phrase)
}

fn requested_edition_match_score(normalized_name: &str, requested_edition: &str) -> Option<u32> {
    match requested_edition.trim().to_ascii_lowercase().as_str() {
        "home" => {
            if !contains_image_phrase(normalized_name, "home") {
                return None;
            }

            let mut score = 0;
            if contains_image_phrase(normalized_name, "single language") {
                score += 10;
            }
            if contains_image_phrase(normalized_name, "n") {
                score += 1;
            }
            Some(score)
        }
        "pro" => {
            let is_pro = contains_image_phrase(normalized_name, "pro")
                || contains_image_phrase(normalized_name, "professional");
            let excluded = contains_image_phrase(normalized_name, "education")
                || contains_image_phrase(normalized_name, "workstation")
                || contains_image_phrase(normalized_name, "workstations");
            if !is_pro || excluded {
                return None;
            }

            let mut score = 0;
            if contains_image_phrase(normalized_name, "professional") {
                score += 1;
            }
            if contains_image_phrase(normalized_name, "n") {
                score += 2;
            }
            Some(score)
        }
        "enterprise" => {
            if !contains_image_phrase(normalized_name, "enterprise")
                || contains_image_phrase(normalized_name, "iot")
            {
                return None;
            }

            let mut score = 0;
            if contains_image_phrase(normalized_name, "n") {
                score += 1;
            }
            Some(score)
        }
        "education" => {
            if !contains_image_phrase(normalized_name, "education")
                || contains_image_phrase(normalized_name, "pro education")
            {
                return None;
            }

            let mut score = 0;
            if contains_image_phrase(normalized_name, "n") {
                score += 1;
            }
            Some(score)
        }
        _ => None,
    }
}

pub fn resolve_requested_edition_image<'a>(
    images: &'a [WimImage],
    requested_edition: &str,
) -> Option<&'a WimImage> {
    images
        .iter()
        .filter_map(|image| {
            let normalized_name = normalize_image_name_for_matching(&image.name);
            requested_edition_match_score(&normalized_name, requested_edition)
                .map(|score| (score, image.index, image))
        })
        .min_by_key(|(score, index, _)| (*score, *index))
        .map(|(_, _, image)| image)
}

pub fn describe_available_images(images: &[WimImage]) -> String {
    if images.is_empty() {
        return "<none>".to_string();
    }

    images
        .iter()
        .map(|image| format!("{}: {}", image.index, image.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// WIM file information
#[derive(Debug, Clone)]
pub struct WimInfo {
    pub path: PathBuf,
    pub images: Vec<WimImage>,
}

/// Individual image within a WIM
#[derive(Debug, Clone)]
pub struct WimImage {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::{describe_available_images, resolve_requested_edition_image, WimImage};

    fn images() -> Vec<WimImage> {
        vec![
            WimImage {
                index: 1,
                name: "Windows 11 Home".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 2,
                name: "Windows 11 Home Single Language".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 3,
                name: "Windows 11 Pro".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 4,
                name: "Windows 11 Pro N".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 5,
                name: "Windows 11 Pro Education".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 6,
                name: "Windows 11 Pro for Workstations".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 7,
                name: "Windows 11 Education".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 8,
                name: "Windows 11 Enterprise".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 9,
                name: "Windows 11 Enterprise N".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
            WimImage {
                index: 10,
                name: "Windows 11 IoT Enterprise".to_string(),
                description: String::new(),
                size_bytes: 0,
            },
        ]
    }

    #[test]
    fn resolve_requested_edition_image_prefers_standard_home_over_single_language() {
        let images = images();
        let resolved = resolve_requested_edition_image(&images, "Home").expect("home match");
        assert_eq!(resolved.index, 1);
        assert_eq!(resolved.name, "Windows 11 Home");
    }

    #[test]
    fn resolve_requested_edition_image_allows_single_language_when_only_home_variant() {
        let images = [WimImage {
            index: 2,
            name: "Windows 11 Home Single Language".to_string(),
            description: String::new(),
            size_bytes: 0,
        }];
        let resolved =
            resolve_requested_edition_image(&images, "Home").expect("home single language match");

        assert_eq!(resolved.index, 2);
    }

    #[test]
    fn resolve_requested_edition_image_excludes_pro_education_and_workstations() {
        let images = images();
        let resolved = resolve_requested_edition_image(&images, "Pro").expect("pro match");
        assert_eq!(resolved.index, 3);
        assert_eq!(resolved.name, "Windows 11 Pro");
    }

    #[test]
    fn resolve_requested_edition_image_excludes_iot_enterprise() {
        let images = images();
        let resolved =
            resolve_requested_edition_image(&images, "Enterprise").expect("enterprise match");
        assert_eq!(resolved.index, 8);
        assert_eq!(resolved.name, "Windows 11 Enterprise");
    }

    #[test]
    fn resolve_requested_edition_image_excludes_pro_education_for_education_requests() {
        let images = images();
        let resolved =
            resolve_requested_edition_image(&images, "Education").expect("education match");
        assert_eq!(resolved.index, 7);
        assert_eq!(resolved.name, "Windows 11 Education");
    }

    #[test]
    fn resolve_requested_edition_image_returns_none_when_requested_edition_is_missing() {
        let images = [WimImage {
            index: 1,
            name: "Windows 11 Enterprise".to_string(),
            description: String::new(),
            size_bytes: 0,
        }];
        let resolved = resolve_requested_edition_image(&images, "Home");

        assert!(resolved.is_none());
    }

    #[test]
    fn describe_available_images_lists_index_and_name() {
        let description = describe_available_images(&images()[0..2]);
        assert!(description.contains("1: Windows 11 Home"));
        assert!(description.contains("2: Windows 11 Home Single Language"));
    }
}
