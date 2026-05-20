#[cfg(not(target_os = "windows"))]
use crate::build::linux_support::{export_image_to_wim, read_wim_info};
use crate::core::adk::{resolve_adk_paths, AdkPaths};
use crate::core::errors::{BitOSDTError, BitOSDTResult};
#[cfg(target_os = "windows")]
use crate::core::windows_tools::{
    dism_path_arg, format_process_failure, resolve_dism_exe, run_dism, run_dism_streaming,
};
use crate::download::{DownloadProgress, DownloadStatus, HashValidator, ProgressTracker};
use futures::StreamExt;
use reqwest::Client;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

/// ESD Downloader for Microsoft Windows ESD files
pub struct EsdDownloader {
    client: Client,
    download_path: PathBuf,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    adk_paths: Option<AdkPaths>,
    cancelled: Arc<AtomicBool>,
}

/// Information about an ESD file to download
#[derive(Debug, Clone)]
pub struct EsdInfo {
    pub id: String,
    pub display_name: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub language: String,
    pub architecture: String,
    pub version: String,
    pub build: String,
}

/// Result of ESD to WIM conversion
#[derive(Debug, Clone)]
pub struct WimConversionResult {
    pub wim_path: PathBuf,
    pub editions: Vec<WimEdition>,
}

#[derive(Debug, Clone)]
pub struct WimEdition {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
}

impl EsdDownloader {
    pub fn new(download_path: PathBuf) -> BitOSDTResult<Self> {
        let adk_paths = resolve_adk_paths(None, std::env::consts::ARCH);
        Self::new_with_adk(download_path, adk_paths)
    }

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

    pub fn new_with_adk(
        download_path: PathBuf,
        adk_paths: Option<AdkPaths>,
    ) -> BitOSDTResult<Self> {
        // Create download directory if it doesn't exist
        fs::create_dir_all(&download_path)?;

        let client = Client::builder()
            .user_agent("BitOSDT/2.0")
            .timeout(std::time::Duration::from_secs(3600)) // 1 hour timeout for large files
            .tcp_nodelay(true) // Disable Nagle's algorithm for better throughput
            .pool_max_idle_per_host(10) // Connection pooling for better performance
            .build()
            .map_err(|e| BitOSDTError::Network(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            download_path,
            adk_paths,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Get file size via HEAD request before downloading
    pub async fn get_file_size(&self, url: &str) -> BitOSDTResult<u64> {
        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|e| BitOSDTError::Network(format!("HEAD request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(BitOSDTError::Network(format!(
                "HEAD request failed with status: {}",
                response.status()
            )));
        }

        Ok(response.content_length().unwrap_or(0))
    }

    /// Download ESD file from URL with progress tracking
    pub async fn download_esd<F>(
        &self,
        esd_info: &EsdInfo,
        mut progress_callback: F,
    ) -> BitOSDTResult<PathBuf>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        self.cancelled.store(false, Ordering::SeqCst);

        let filename = Self::extract_filename(&esd_info.url, &esd_info.id);
        let output_path = self.download_path.join(&filename);
        let partial_path = self.download_path.join(format!("{}.partial", filename));

        info!(
            "Downloading ESD: {} -> {:?}",
            esd_info.display_name, output_path
        );

        // Get actual file size if not provided
        let total_size = if esd_info.size_bytes > 0 {
            esd_info.size_bytes
        } else {
            // Try to get file size via HEAD request
            match self.get_file_size(&esd_info.url).await {
                Ok(size) if size > 0 => {
                    info!("Retrieved file size via HEAD: {} bytes", size);
                    size
                }
                _ => {
                    warn!("Could not determine file size, progress will be estimated");
                    0
                }
            }
        };

        // Check if already downloaded and valid
        if output_path.exists() {
            if let Some(ref expected_hash) = esd_info.sha256 {
                if HashValidator::validate_sha256(&output_path, expected_hash)? {
                    info!("ESD already downloaded and verified: {:?}", output_path);
                    let mut progress = DownloadProgress::new(esd_info.size_bytes);
                    progress.bytes_downloaded = esd_info.size_bytes;
                    progress.percent = 100.0;
                    progress.status = DownloadStatus::Completed;
                    progress_callback(progress);
                    return Ok(output_path);
                } else {
                    warn!("Existing file hash mismatch, re-downloading");
                    fs::remove_file(&output_path)?;
                }
            } else {
                // No hash to verify, assume it's valid
                info!(
                    "ESD already exists (no hash verification): {:?}",
                    output_path
                );
                return Ok(output_path);
            }
        }

        // Check for partial download to resume
        let resume_from = if partial_path.exists() {
            let metadata = fs::metadata(&partial_path)?;
            Some(metadata.len())
        } else {
            None
        };

        let mut progress = DownloadProgress::new(total_size);
        progress.status = DownloadStatus::Downloading;

        // Build request with Range header for resume
        let mut request = self.client.get(&esd_info.url);
        if let Some(bytes) = resume_from {
            info!("Resuming download from byte {}", bytes);
            request = request.header("Range", format!("bytes={}-", bytes));
            progress.bytes_downloaded = bytes;
        }

        let response = request
            .send()
            .await
            .map_err(|e| BitOSDTError::Network(format!("Failed to start download: {}", e)))?;

        if !response.status().is_success() && response.status().as_u16() != 206 {
            return Err(BitOSDTError::Network(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        // Get content length for progress (may differ for partial content)
        let content_length = response.content_length().unwrap_or(0);
        if resume_from.is_none() && content_length > 0 {
            progress.total_bytes = content_length;
        }

        // Open file for writing (append for resume)
        let mut file = if resume_from.is_some() {
            OpenOptions::new().append(true).open(&partial_path)?
        } else {
            File::create(&partial_path)?
        };

        let mut tracker = ProgressTracker::new();
        tracker.start();

        // Download in chunks
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            // Check for cancellation
            if self.cancelled.load(Ordering::SeqCst) {
                info!("Download cancelled by user");
                progress.status = DownloadStatus::Cancelled;
                progress_callback(progress);
                return Err(BitOSDTError::Cancelled);
            }

            let chunk = chunk_result
                .map_err(|e| BitOSDTError::Network(format!("Failed to read chunk: {}", e)))?;

            file.write_all(&chunk)?;

            progress.bytes_downloaded += chunk.len() as u64;
            let bytes = progress.bytes_downloaded;
            tracker.update(&mut progress, bytes);
            progress_callback(progress.clone());
        }

        file.flush()?;
        drop(file);

        // Verify hash if available
        if let Some(ref expected_hash) = esd_info.sha256 {
            info!("Verifying download hash...");
            if !HashValidator::validate_sha256(&partial_path, expected_hash)? {
                fs::remove_file(&partial_path)?;
                return Err(BitOSDTError::Validation(
                    "Downloaded file hash does not match expected value".to_string(),
                ));
            }
        }

        // Rename partial to final
        fs::rename(&partial_path, &output_path)?;

        progress.status = DownloadStatus::Completed;
        progress_callback(progress);

        info!("Download completed: {:?}", output_path);
        Ok(output_path)
    }

    /// Resume an interrupted download
    pub async fn resume_download<F>(
        &self,
        esd_info: &EsdInfo,
        progress_callback: F,
    ) -> BitOSDTResult<PathBuf>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        // The download_esd function already handles resume automatically
        self.download_esd(esd_info, progress_callback).await
    }

    /// Cancel the current download
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Convert ESD to WIM using DISM
    pub fn convert_esd_to_wim<F>(
        &self,
        esd_path: &Path,
        output_wim: &Path,
        image_index: Option<u32>,
        progress_callback: F,
    ) -> BitOSDTResult<WimConversionResult>
    where
        F: FnMut(u8, String),
    {
        #[cfg(target_os = "windows")]
        {
            self.convert_esd_to_wim_windows(esd_path, output_wim, image_index, progress_callback)
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.convert_esd_to_wim_mock(esd_path, output_wim, image_index, progress_callback)
        }
    }

    #[cfg(target_os = "windows")]
    fn convert_esd_to_wim_windows<F>(
        &self,
        esd_path: &Path,
        output_wim: &Path,
        image_index: Option<u32>,
        mut progress_callback: F,
    ) -> BitOSDTResult<WimConversionResult>
    where
        F: FnMut(u8, String),
    {
        info!("Converting ESD to WIM: {:?} -> {:?}", esd_path, output_wim);

        // First, get ESD info to find available editions
        progress_callback(5, "Analyzing ESD file...".to_string());
        let editions = self.get_esd_editions(esd_path)?;

        if editions.is_empty() {
            return Err(BitOSDTError::WinPE(
                "No editions found in ESD file".to_string(),
            ));
        }

        // Determine which index to export
        let index = image_index.unwrap_or_else(|| {
            // Default to Pro if available, otherwise first edition
            editions
                .iter()
                .find(|e| {
                    e.name.to_lowercase().contains("pro")
                        && !e.name.to_lowercase().contains("education")
                })
                .map(|e| e.index)
                .unwrap_or(editions[0].index)
        });

        progress_callback(10, format!("Exporting edition index {}...", index));

        // Create parent directory for output
        if let Some(parent) = output_wim.parent() {
            fs::create_dir_all(parent)?;
        }

        // Run DISM to export image
        let args = vec![
            "/Export-Image".to_string(),
            "/English".to_string(),
            dism_path_arg("/SourceImageFile", esd_path),
            format!("/SourceIndex:{}", index),
            dism_path_arg("/DestinationImageFile", output_wim),
            "/Compress:max".to_string(),
            "/CheckIntegrity".to_string(),
        ];

        let mut last_progress = 10u8;
        let output = run_dism_streaming(&args, self.adk_paths.as_ref(), |line| {
            let message = if let Some(percent) = Self::parse_dism_progress_percent(&line) {
                let scaled = 10 + ((percent as u32 * 85 / 100) as u8);
                last_progress = last_progress.max(scaled.min(95));
                format!("DISM export progress: {}", line)
            } else {
                format!("DISM export output: {}", line)
            };
            progress_callback(last_progress, message);
        })?;

        if !output.status.success() {
            let dism_exe = resolve_dism_exe(self.adk_paths.as_ref());
            return Err(BitOSDTError::WinPE(format!(
                "DISM export failed: {}",
                format_process_failure(&dism_exe, &args, &output)
            )));
        }

        progress_callback(100, "Conversion complete".to_string());

        info!("ESD to WIM conversion completed: {:?}", output_wim);

        Ok(WimConversionResult {
            wim_path: output_wim.to_path_buf(),
            editions,
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn convert_esd_to_wim_mock<F>(
        &self,
        esd_path: &Path,
        output_wim: &Path,
        image_index: Option<u32>,
        mut progress_callback: F,
    ) -> BitOSDTResult<WimConversionResult>
    where
        F: FnMut(u8, String),
    {
        warn!("Converting ESD to WIM on Linux via wimlib-imagex");
        progress_callback(5, "Analyzing ESD file...".to_string());
        let editions = self.get_esd_editions_mock(esd_path)?;
        let index = image_index.unwrap_or_else(|| {
            editions
                .iter()
                .find(|edition| {
                    edition.name.to_ascii_lowercase().contains("pro")
                        && !edition.name.to_ascii_lowercase().contains("education")
                })
                .map(|edition| edition.index)
                .unwrap_or(1)
        });
        progress_callback(25, format!("Exporting edition index {}...", index));
        export_image_to_wim(esd_path, Some(index), output_wim)?;
        progress_callback(100, "Conversion complete".to_string());

        Ok(WimConversionResult {
            wim_path: output_wim.to_path_buf(),
            editions,
        })
    }

    /// Get list of editions/images in an ESD file
    pub fn get_esd_editions(&self, esd_path: &Path) -> BitOSDTResult<Vec<WimEdition>> {
        #[cfg(target_os = "windows")]
        {
            self.get_esd_editions_windows(esd_path)
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.get_esd_editions_mock(esd_path)
        }
    }

    #[cfg(target_os = "windows")]
    fn get_esd_editions_windows(&self, esd_path: &Path) -> BitOSDTResult<Vec<WimEdition>> {
        let args = vec![
            "/Get-WimInfo".to_string(),
            "/English".to_string(),
            dism_path_arg("/WimFile", esd_path),
        ];

        let output = run_dism(&args, self.adk_paths.as_ref())?;

        if !output.status.success() {
            let dism_exe = resolve_dism_exe(self.adk_paths.as_ref());
            return Err(BitOSDTError::WinPE(format!(
                "DISM get-wiminfo failed: {}",
                format_process_failure(&dism_exe, &args, &output)
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_dism_wiminfo(&stdout)
    }

    #[cfg(not(target_os = "windows"))]
    fn get_esd_editions_mock(&self, esd_path: &Path) -> BitOSDTResult<Vec<WimEdition>> {
        let info = read_wim_info(esd_path)?;
        Ok(info
            .images
            .into_iter()
            .map(|image| WimEdition {
                index: image.index,
                name: image.name,
                description: image.description,
                size_bytes: image.size_bytes,
            })
            .collect())
    }

    /// Parse DISM /Get-WimInfo output
    #[cfg(any(test, target_os = "windows"))]
    fn parse_dism_wiminfo(output: &str) -> BitOSDTResult<Vec<WimEdition>> {
        let mut editions = Vec::new();
        let mut current_index: Option<u32> = None;
        let mut current_name: Option<String> = None;
        let mut current_desc: Option<String> = None;
        let mut current_size: u64 = 0;

        for line in output.lines() {
            let line = line.trim();

            if line.starts_with("Index :") || line.starts_with("Index:") {
                // Save previous edition if exists
                if let (Some(idx), Some(name)) = (current_index, current_name.take()) {
                    editions.push(WimEdition {
                        index: idx,
                        name,
                        description: current_desc.take().unwrap_or_default(),
                        size_bytes: current_size,
                    });
                }

                // Parse new index
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    current_index = parts[1].trim().parse().ok();
                }
                current_size = 0;
            } else if line.starts_with("Name :") || line.starts_with("Name:") {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() >= 2 {
                    current_name = Some(parts[1].trim().to_string());
                }
            } else if line.starts_with("Description :") || line.starts_with("Description:") {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() >= 2 {
                    current_desc = Some(parts[1].trim().to_string());
                }
            } else if line.starts_with("Size :") || line.starts_with("Size:") {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() >= 2 {
                    // Parse size like "4,123,456,789 bytes"
                    let size_str = parts[1].trim().replace(",", "").replace(" bytes", "");
                    current_size = size_str.parse().unwrap_or(0);
                }
            }
        }

        // Don't forget the last edition
        if let (Some(idx), Some(name)) = (current_index, current_name) {
            editions.push(WimEdition {
                index: idx,
                name,
                description: current_desc.unwrap_or_default(),
                size_bytes: current_size,
            });
        }

        Ok(editions)
    }

    /// Extract filename from URL or generate from ID
    fn extract_filename(url: &str, id: &str) -> String {
        url.split('/')
            .next_back()
            .filter(|s| s.contains('.'))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}.esd", id))
    }

    /// Get the download path
    pub fn download_path(&self) -> &Path {
        &self.download_path
    }

    /// Validate that the downloaded ESD contains the requested edition.
    /// Returns an error if the ESD doesn't contain the expected edition.
    pub fn validate_esd_contains_edition(
        esd_path: &Path,
        expected_edition: &str,
    ) -> BitOSDTResult<Vec<WimEdition>> {
        // Create a temporary downloader just for inspection
        let temp_dir = esd_path.parent().unwrap_or(Path::new("."));
        let downloader = EsdDownloader::new(temp_dir.to_path_buf())?;
        let editions = downloader.get_esd_editions(esd_path)?;

        if editions.is_empty() {
            return Err(BitOSDTError::Validation(
                "Downloaded ESD contains no editions".to_string(),
            ));
        }

        let expected_lower = expected_edition.to_lowercase();
        let found = editions.iter().any(|e| {
            let name_lower = e.name.to_lowercase();
            let desc_lower = e.description.to_lowercase();
            // Match edition name in the edition field (e.g., "Enterprise" matches "Windows 11 Enterprise")
            name_lower.contains(&expected_lower) || desc_lower.contains(&expected_lower)
        });

        if !found {
            let available: Vec<&str> = editions.iter().map(|e| e.name.as_str()).collect();
            return Err(BitOSDTError::Validation(format!(
                "Downloaded ESD does not contain the '{}' edition. Available editions: {}. \
                     This typically means the wrong channel (Retail vs Volume) was downloaded. \
                     Select the correct edition and try again.",
                expected_edition,
                available.join(", ")
            )));
        }

        Ok(editions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename() {
        assert_eq!(
            EsdDownloader::extract_filename("https://example.com/path/to/win11.esd", "fallback"),
            "win11.esd"
        );

        assert_eq!(
            EsdDownloader::extract_filename("https://example.com/path/to/", "fallback"),
            "fallback.esd"
        );
    }

    #[test]
    fn test_parse_dism_wiminfo() {
        let output = r#"
Deployment Image Servicing and Management tool
Version: 10.0.26100.1

Details for image : C:\temp\install.esd

Index : 1
Name : Windows 11 Home
Description : Windows 11 Home
Size : 15,123,456,789 bytes

Index : 2
Name : Windows 11 Pro
Description : Windows 11 Pro
Size : 15,234,567,890 bytes
"#;

        let editions = EsdDownloader::parse_dism_wiminfo(output).unwrap();
        assert_eq!(editions.len(), 2);
        assert_eq!(editions[0].index, 1);
        assert_eq!(editions[0].name, "Windows 11 Home");
        assert_eq!(editions[1].index, 2);
        assert_eq!(editions[1].name, "Windows 11 Pro");
    }

    #[test]
    fn test_parse_dism_progress_percent() {
        assert_eq!(
            EsdDownloader::parse_dism_progress_percent("10.0%"),
            Some(10)
        );
        assert_eq!(
            EsdDownloader::parse_dism_progress_percent("[=================42.3%=================]"),
            Some(42)
        );
        assert_eq!(
            EsdDownloader::parse_dism_progress_percent("No percentage here"),
            None
        );
    }
}
