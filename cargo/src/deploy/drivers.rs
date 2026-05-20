use crate::catalog::xml_parser::XmlParser;
use crate::core::database::Database;
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::models::{
    DriverPack, HardwareInfo, RuntimeDriverConfig, RuntimeDriverContext,
    RuntimeDriverFailurePolicy, RuntimeDriverManifest,
};
use crate::utils::download::{DownloadManager, ProgressCallback};
use chrono::Utc;
use reqwest::Client;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub struct DriverManager {
    db: Database,
    download_manager: DownloadManager,
    driver_cache_dir: PathBuf,
}

impl DriverManager {
    pub async fn new(db: Database, cache_dir: PathBuf) -> BitOSDTResult<Self> {
        tokio::fs::create_dir_all(&cache_dir).await?;

        let download_manager = DownloadManager::new(cache_dir.clone()).await?;

        Ok(Self {
            db,
            download_manager,
            driver_cache_dir: cache_dir,
        })
    }

    /// Find matching DriverPack for detected hardware
    pub fn find_matching_driverpack(
        &self,
        hardware: &HardwareInfo,
        os_version: &str,
    ) -> BitOSDTResult<Option<DriverPack>> {
        info!(
            "Searching for DriverPack: {} {} - OS: {}",
            hardware.manufacturer, hardware.model, os_version
        );

        // Normalize manufacturer name
        let manufacturer = Self::normalize_manufacturer(&hardware.manufacturer);

        // Get all driverpacks for this manufacturer
        let driverpacks = self.db.get_driverpacks_by_manufacturer(&manufacturer)?;

        if driverpacks.is_empty() {
            warn!("No DriverPacks found for manufacturer: {}", manufacturer);
            return Ok(None);
        }

        info!(
            "Found {} DriverPacks for {}",
            driverpacks.len(),
            manufacturer
        );

        Ok(Self::find_best_match_from_candidates(
            hardware,
            os_version,
            &driverpacks,
        ))
    }

    pub fn find_best_match_from_candidates(
        hardware: &HardwareInfo,
        os_version: &str,
        driverpacks: &[DriverPack],
    ) -> Option<DriverPack> {
        let mut best_match: Option<(DriverPack, u32)> = None;

        for dp in driverpacks {
            let score = Self::calculate_match_score(dp, hardware, os_version);

            if score > 0 {
                info!(
                    "DriverPack candidate: {} {} (score: {})",
                    dp.name, dp.os_version, score
                );

                match &best_match {
                    Some((_, best_score)) if score <= *best_score => {}
                    _ => best_match = Some((dp.clone(), score)),
                }
            }
        }

        best_match.map(|(dp, _)| dp)
    }

    /// Calculate match score between driverpack and hardware
    fn calculate_match_score(
        driverpack: &DriverPack,
        hardware: &HardwareInfo,
        target_os_version: &str,
    ) -> u32 {
        let mut score: u32 = 0;

        // OS version match (highest priority)
        if driverpack.os_version.to_lowercase() == target_os_version.to_lowercase() {
            score += 100;
        } else if (driverpack.os_version.starts_with("W10") && target_os_version.starts_with("10"))
            || (driverpack.os_version.starts_with("W11") && target_os_version.starts_with("11"))
        {
            score += 50;
        }

        // Product match
        if !driverpack.product.is_empty() {
            let hw_product_lower = hardware.product.to_lowercase();
            let dp_product_lower = driverpack.product.to_lowercase();

            if hw_product_lower == dp_product_lower {
                score += 50;
            } else if hw_product_lower.contains(&dp_product_lower) {
                score += 30;
            } else if dp_product_lower.contains(&hw_product_lower) {
                score += 20;
            }
        }

        // Model match
        if !driverpack.model.is_empty() {
            let hw_model_lower = hardware.model.to_lowercase();
            let dp_model_lower = driverpack.model.to_lowercase();

            if hw_model_lower == dp_model_lower {
                score += 40;
            } else if hw_model_lower.contains(&dp_model_lower) {
                score += 25;
            } else if dp_model_lower.contains(&hw_model_lower) {
                score += 15;
            }
        }

        score
    }

    /// Download and extract a DriverPack
    pub async fn download_driverpack(
        &self,
        driverpack: &DriverPack,
        progress_callback: Option<ProgressCallback>,
    ) -> BitOSDTResult<PathBuf> {
        let cache_path = self.driver_cache_dir.join(&driverpack.filename);

        if cache_path.exists() {
            info!("DriverPack already cached: {}", cache_path.display());
            return Ok(cache_path);
        }

        info!(
            "Downloading DriverPack: {} from {}",
            driverpack.name, driverpack.url
        );

        let downloaded_path = self
            .download_manager
            .download_file(&driverpack.url, &driverpack.filename, progress_callback)
            .await?;

        if !driverpack.hash_md5.is_empty() {
            info!("Verifying MD5 hash...");
            match Self::verify_md5(&downloaded_path, &driverpack.hash_md5).await {
                Ok(true) => info!("Hash verification passed"),
                Ok(false) => warn!("Hash verification failed, but continuing"),
                Err(e) => warn!("Could not verify hash: {}", e),
            }
        }

        Ok(downloaded_path)
    }

    pub async fn download_driverpack_to_cache(
        cache_dir: &Path,
        driverpack: &DriverPack,
        progress_callback: Option<ProgressCallback>,
    ) -> BitOSDTResult<PathBuf> {
        tokio::fs::create_dir_all(cache_dir).await?;
        let cache_path = cache_dir.join(&driverpack.filename);
        let download_manager = DownloadManager::new(cache_dir.to_path_buf()).await?;

        // Check if already cached
        if cache_path.exists() {
            info!("DriverPack already cached: {}", cache_path.display());
            return Ok(cache_path);
        }

        info!(
            "Downloading DriverPack: {} from {}",
            driverpack.name, driverpack.url
        );

        // Download the file
        let downloaded_path = download_manager
            .download_file(&driverpack.url, &driverpack.filename, progress_callback)
            .await?;

        // Verify hash if available
        if !driverpack.hash_md5.is_empty() {
            info!("Verifying MD5 hash...");
            match Self::verify_md5(&downloaded_path, &driverpack.hash_md5).await {
                Ok(true) => info!("Hash verification passed"),
                Ok(false) => {
                    warn!("Hash verification failed, but continuing");
                }
                Err(e) => warn!("Could not verify hash: {}", e),
            }
        }

        Ok(downloaded_path)
    }

    pub async fn fetch_live_catalog(manufacturer: &str) -> BitOSDTResult<Vec<DriverPack>> {
        let normalized = Self::normalize_manufacturer(manufacturer);
        let url = format!(
            "https://raw.githubusercontent.com/OSDeploy/OSD/master/Catalogs/DriverPack/{}.xml",
            normalized
        );

        let response = reqwest::get(&url).await.map_err(|e| {
            BitOSDTError::Network(format!("Failed to fetch {} catalog: {}", normalized, e))
        })?;

        if !response.status().is_success() {
            return Err(BitOSDTError::CatalogSync(format!(
                "HTTP {} while fetching {} catalog",
                response.status(),
                normalized
            )));
        }

        let xml = response.text().await.map_err(|e| {
            BitOSDTError::CatalogSync(format!("Failed to read {} catalog: {}", normalized, e))
        })?;

        let parsed = XmlParser::parse_driverpack_catalog(&xml)?;
        Ok(parsed
            .into_iter()
            .map(|item| item.to_driverpack(&normalized))
            .collect())
    }

    /// Extract DriverPack to destination directory
    pub async fn extract_driverpack(
        &self,
        archive_path: &Path,
        dest_dir: &Path,
    ) -> BitOSDTResult<PathBuf> {
        Self::extract_driverpack_archive(archive_path, dest_dir).await?;
        Ok(dest_dir.to_path_buf())
    }

    pub async fn extract_driverpack_archive(
        archive_path: &Path,
        dest_dir: &Path,
    ) -> BitOSDTResult<()> {
        tokio::fs::create_dir_all(dest_dir).await?;

        let extension = archive_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        info!(
            "Extracting {} to {}",
            archive_path.display(),
            dest_dir.display()
        );

        match extension.as_str() {
            "cab" => Self::extract_cab(archive_path, dest_dir).await?,
            "zip" => Self::extract_zip(archive_path, dest_dir).await?,
            "7z" => Self::extract_7z(archive_path, dest_dir).await?,
            _ => {
                return Err(BitOSDTError::Driver(format!(
                    "Unsupported archive format: {}",
                    extension
                )))
            }
        }

        Ok(())
    }

    /// Get extraction directory for a DriverPack
    pub fn get_extraction_dir(&self, driverpack: &DriverPack) -> PathBuf {
        self.driver_cache_dir.join("extracted").join(&driverpack.id)
    }

    /// Check if DriverPack is already extracted
    pub async fn is_driverpack_extracted(&self, driverpack: &DriverPack) -> bool {
        let extract_dir = self.get_extraction_dir(driverpack);
        extract_dir.exists() && Self::has_inf_files(&extract_dir).await.unwrap_or(false)
    }

    /// Normalize manufacturer name for matching
    pub fn normalize_manufacturer(manufacturer: &str) -> String {
        let lower = manufacturer.to_lowercase();

        // Map common manufacturer variations
        if lower.contains("dell") {
            "Dell".to_string()
        } else if lower.contains("hp") || lower.contains("hewlett") {
            "HP".to_string()
        } else if lower.contains("lenovo") {
            "Lenovo".to_string()
        } else if lower.contains("microsoft") || lower.contains("surface") {
            "Microsoft".to_string()
        } else {
            manufacturer.to_string()
        }
    }

    /// Verify MD5 hash of file
    async fn verify_md5(file_path: &Path, expected_hash: &str) -> BitOSDTResult<bool> {
        use tokio::fs::File;
        use tokio::io::AsyncReadExt;

        let mut file = File::open(file_path).await?;
        let mut hasher = md5::Context::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.consume(&buffer[..bytes_read]);
        }

        let result = hasher.compute();
        let computed_hash = format!("{:x}", result);

        Ok(computed_hash.to_lowercase() == expected_hash.to_lowercase())
    }

    /// Check if directory contains .inf files
    async fn has_inf_files(dir: &Path) -> BitOSDTResult<bool> {
        Self::has_inf_files_recursive(dir).await
    }

    /// Non-recursive async check for .inf files
    async fn has_inf_files_recursive(dir: &Path) -> BitOSDTResult<bool> {
        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext
                        .to_str()
                        .map(|e| e.eq_ignore_ascii_case("inf"))
                        .unwrap_or(false)
                    {
                        return Ok(true);
                    }
                }
            } else if path.is_dir() {
                // Use spawn for recursive call to avoid async recursion issues
                let found = Box::pin(Self::has_inf_files_recursive(&path)).await?;
                if found {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Extract CAB archive
    async fn extract_cab(archive: &Path, dest: &Path) -> BitOSDTResult<()> {
        // On Windows, use expand.exe
        // On Linux, use cabextract or implement in Rust

        #[cfg(target_os = "windows")]
        {
            let status = tokio::process::Command::new("expand")
                .arg(archive)
                .arg(dest)
                .arg("-f:*")
                .status()
                .await
                .map_err(|e| BitOSDTError::Driver(format!("Failed to run expand: {}", e)))?;

            if !status.success() {
                return Err(BitOSDTError::Driver("expand.exe failed".to_string()));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Try cabextract first
            let result = tokio::process::Command::new("cabextract")
                .arg("-d")
                .arg(dest)
                .arg(archive)
                .status()
                .await;

            if let Ok(status) = result {
                if status.success() {
                    return Ok(());
                }
            }

            // Fallback: check if 7z is available
            let status = tokio::process::Command::new("7z")
                .arg("x")
                .arg(format!("-o{}", dest.display()))
                .arg(archive)
                .status()
                .await
                .map_err(|e| BitOSDTError::Driver(format!("Failed to extract CAB: {}", e)))?;

            if !status.success() {
                return Err(BitOSDTError::Driver("7z extraction failed".to_string()));
            }
        }

        Ok(())
    }

    /// Extract ZIP archive using native Rust zip crate
    async fn extract_zip(archive: &Path, dest: &Path) -> BitOSDTResult<()> {
        let archive = archive.to_path_buf();
        let dest = dest.to_path_buf();

        // Run ZIP extraction on a blocking thread to avoid blocking the async runtime
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&archive)
                .map_err(|e| BitOSDTError::Driver(format!("Failed to open ZIP archive: {}", e)))?;
            let mut zip = zip::ZipArchive::new(file)
                .map_err(|e| BitOSDTError::Driver(format!("Failed to read ZIP archive: {}", e)))?;

            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| {
                    BitOSDTError::Driver(format!("Failed to read ZIP entry: {}", e))
                })?;

                let out_path = dest.join(entry.mangled_name());

                if entry.is_dir() {
                    std::fs::create_dir_all(&out_path)?;
                } else {
                    if let Some(parent) = out_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut outfile = std::fs::File::create(&out_path)?;
                    std::io::copy(&mut entry, &mut outfile)?;
                }
            }

            Ok::<(), BitOSDTError>(())
        })
        .await
        .map_err(|e| BitOSDTError::Driver(format!("ZIP extraction task failed: {}", e)))??;

        Ok(())
    }

    /// Extract 7z archive
    async fn extract_7z(archive: &Path, dest: &Path) -> BitOSDTResult<()> {
        // Requires 7z command line tool
        let status = tokio::process::Command::new("7z")
            .arg("x")
            .arg(format!("-o{}", dest.display()))
            .arg(archive)
            .status()
            .await
            .map_err(|e| BitOSDTError::Driver(format!("Failed to run 7z: {}", e)))?;

        if !status.success() {
            return Err(BitOSDTError::Driver("7z extraction failed".to_string()));
        }

        Ok(())
    }

    pub fn inject_offline_drivers(windows_path: &Path, driver_dir: &Path) -> BitOSDTResult<u32> {
        if !driver_dir.exists() {
            return Err(BitOSDTError::Driver(format!(
                "Driver directory not found: {}",
                driver_dir.display()
            )));
        }

        #[cfg(target_os = "windows")]
        {
            use crate::core::windows_tools::{dism_path_arg, run_dism};

            let args = vec![
                dism_path_arg("/Image", windows_path),
                "/Add-Driver".to_string(),
                dism_path_arg("/Driver", driver_dir),
                "/Recurse".to_string(),
            ];

            let output = run_dism(&args, None).map_err(|e| {
                BitOSDTError::Driver(format!(
                    "Failed to inject drivers into offline image {}: {}",
                    windows_path.display(),
                    e
                ))
            })?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !output.status.success() {
                warn!(
                    "Offline driver injection reported a non-zero exit for {}: {}",
                    driver_dir.display(),
                    stderr
                );
            }

            let installed = stdout
                .lines()
                .filter(|line| {
                    line.contains("driver package added")
                        || line.contains("driver package was successfully installed")
                })
                .count() as u32;

            return Ok(installed);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (windows_path, driver_dir);
            warn!("Offline driver injection requires Windows DISM - skipping in development mode");
            Ok(0)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedRuntimeDriverContext {
    pub embedded_catalog_path: PathBuf,
    pub staged_cache_path: PathBuf,
    pub cache_download_base_url: Option<String>,
    pub working_directory: PathBuf,
    pub resolved_manifest_path: PathBuf,
}

pub fn resolve_runtime_context(context: &RuntimeDriverContext) -> ResolvedRuntimeDriverContext {
    let defaults = RuntimeDriverContext::winpe_default();
    ResolvedRuntimeDriverContext {
        embedded_catalog_path: context
            .embedded_catalog_path
            .clone()
            .or(defaults.embedded_catalog_path)
            .unwrap_or_else(|| PathBuf::from(r"X:\BitOSDT\Config\driverpacks.json")),
        staged_cache_path: context
            .staged_cache_path
            .clone()
            .or(defaults.staged_cache_path)
            .unwrap_or_else(|| PathBuf::from(r"X:\BitOSDT\DriverCache")),
        cache_download_base_url: context
            .cache_download_base_url
            .clone()
            .or(defaults.cache_download_base_url),
        working_directory: context
            .working_directory
            .clone()
            .or(defaults.working_directory)
            .unwrap_or_else(|| PathBuf::from(r"X:\BitOSDT\DriverCache\working")),
        resolved_manifest_path: context
            .resolved_manifest_path
            .clone()
            .or(defaults.resolved_manifest_path)
            .unwrap_or_else(|| PathBuf::from(r"X:\BitOSDT\State\runtime-driver-resolution.json")),
    }
}

pub fn load_driverpack_catalog_snapshot(path: &Path) -> BitOSDTResult<Vec<DriverPack>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub async fn fetch_manufacturer_driverpacks_online(
    manufacturer: &str,
) -> BitOSDTResult<Vec<DriverPack>> {
    let normalized = DriverManager::normalize_manufacturer(manufacturer);
    let url = format!(
        "https://raw.githubusercontent.com/OSDeploy/OSD/master/Catalogs/DriverPack/{}.xml",
        normalized
    );
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| BitOSDTError::Network(e.to_string()))?;
    let response = client.get(&url).send().await.map_err(|e| {
        BitOSDTError::Network(format!("Failed to refresh {} catalog: {}", normalized, e))
    })?;

    if !response.status().is_success() {
        return Err(BitOSDTError::Network(format!(
            "Failed to refresh {} catalog: HTTP {}",
            normalized,
            response.status()
        )));
    }

    let xml = response.text().await.map_err(|e| {
        BitOSDTError::Network(format!("Failed to read {} catalog: {}", normalized, e))
    })?;
    Ok(XmlParser::parse_driverpack_catalog(&xml)?
        .into_iter()
        .map(|entry| entry.to_driverpack(&normalized))
        .collect())
}

pub async fn prepare_runtime_drivers(
    config: &RuntimeDriverConfig,
    windows_path: Option<&Path>,
) -> BitOSDTResult<RuntimeDriverManifest> {
    let hardware = crate::deploy::hardware::HardwareDetector::new().detect_all()?;
    let context = resolve_runtime_context(&config.runtime_driver_context);

    if !config.runtime_driver_policy.enabled {
        let manifest = RuntimeDriverManifest {
            hardware_manufacturer: hardware.manufacturer,
            hardware_model: hardware.model,
            os_version: config.os_version.clone(),
            matched_driverpack: None,
            archive_path: None,
            extracted_path: None,
            source: Some("disabled".to_string()),
            prepared: false,
            installed_count: 0,
            warnings: vec!["Runtime driver workflow disabled by policy".to_string()],
        };
        write_runtime_driver_manifest(&context, &manifest)?;
        return Ok(manifest);
    }

    let mut warnings = Vec::new();
    let mut source = Some("embedded_catalog".to_string());
    let mut used_online_catalog = false;
    let mut catalog = load_driverpack_catalog_snapshot(&context.embedded_catalog_path)?;
    let mut candidates = filter_catalog_for_manufacturer(&catalog, &hardware.manufacturer);
    let mut matched =
        DriverManager::find_best_match_from_candidates(&hardware, &config.os_version, &candidates);

    if matched.is_none() && config.runtime_driver_policy.refresh_catalog_online {
        match fetch_manufacturer_driverpacks_online(&hardware.manufacturer).await {
            Ok(fresh) => {
                used_online_catalog = true;
                source = Some("online_refresh".to_string());
                catalog = merge_catalogs(catalog, fresh);
                candidates = filter_catalog_for_manufacturer(&catalog, &hardware.manufacturer);
                matched = DriverManager::find_best_match_from_candidates(
                    &hardware,
                    &config.os_version,
                    &candidates,
                );
            }
            Err(err) => warnings.push(format!("Online catalog refresh failed: {}", err)),
        }
    }

    let mut manifest = RuntimeDriverManifest {
        hardware_manufacturer: hardware.manufacturer.clone(),
        hardware_model: hardware.model.clone(),
        os_version: config.os_version.clone(),
        matched_driverpack: matched.clone(),
        archive_path: None,
        extracted_path: None,
        source,
        prepared: false,
        installed_count: 0,
        warnings,
    };

    let Some(driverpack) = matched else {
        manifest
            .warnings
            .push("No matching DriverPack found for detected hardware".to_string());
        write_runtime_driver_manifest(&context, &manifest)?;
        return finalize_runtime_driver_manifest(manifest, &config.runtime_driver_policy);
    };

    fs::create_dir_all(&context.staged_cache_path)?;
    fs::create_dir_all(&context.working_directory)?;
    let archive_path = context.staged_cache_path.join(&driverpack.filename);
    if archive_path.exists() {
        manifest.archive_path = Some(archive_path.clone());
    } else {
        let download_manager = DownloadManager::new(context.staged_cache_path.clone()).await?;
        let primary_download_url = context
            .cache_download_base_url
            .as_ref()
            .map(|base| format!("{}/{}", base.trim_end_matches('/'), driverpack.filename))
            .unwrap_or_else(|| driverpack.url.clone());
        let fallback_download_url = if primary_download_url != driverpack.url {
            Some(driverpack.url.clone())
        } else {
            None
        };
        let download_result = match download_manager
            .download_file(&primary_download_url, &driverpack.filename, None)
            .await
        {
            Ok(downloaded) => Ok(downloaded),
            Err(primary_err) => {
                if let Some(fallback_url) = fallback_download_url.as_ref() {
                    warn!(
                        "Cache download failed for {} ({}). Falling back to vendor URL.",
                        driverpack.filename, primary_err
                    );
                    download_manager
                        .download_file(fallback_url, &driverpack.filename, None)
                        .await
                } else {
                    Err(primary_err)
                }
            }
        };
        match download_result {
            Ok(downloaded) => {
                manifest.archive_path = Some(downloaded.clone());
                if !driverpack.hash_md5.is_empty() {
                    match DriverManager::verify_md5(&downloaded, &driverpack.hash_md5).await {
                        Ok(true) => {}
                        Ok(false) => manifest.warnings.push(format!(
                            "MD5 verification failed for {}",
                            driverpack.filename
                        )),
                        Err(err) => manifest.warnings.push(format!(
                            "Unable to verify MD5 for {}: {}",
                            driverpack.filename, err
                        )),
                    }
                }
            }
            Err(err) => {
                manifest
                    .warnings
                    .push(format!("DriverPack download failed: {}", err));
                write_runtime_driver_manifest(&context, &manifest)?;
                return finalize_runtime_driver_manifest(manifest, &config.runtime_driver_policy);
            }
        }
    }

    let extracted_path = context
        .working_directory
        .join("extracted")
        .join(sanitize_runtime_path_segment(&driverpack.id));
    match DriverManager::extract_driverpack_archive(
        manifest
            .archive_path
            .as_deref()
            .unwrap_or_else(|| archive_path.as_path()),
        &extracted_path,
    )
    .await
    {
        Ok(()) => {
            manifest.prepared = true;
            manifest.extracted_path = Some(extracted_path.clone());
        }
        Err(err) => {
            manifest
                .warnings
                .push(format!("DriverPack extraction failed: {}", err));
            write_runtime_driver_manifest(&context, &manifest)?;
            return finalize_runtime_driver_manifest(manifest, &config.runtime_driver_policy);
        }
    }

    if let Some(windows_path) = windows_path {
        match DriverManager::inject_offline_drivers(windows_path, &extracted_path) {
            Ok(installed) => manifest.installed_count = installed,
            Err(err) => manifest
                .warnings
                .push(format!("Offline driver injection failed: {}", err)),
        }
    }

    if used_online_catalog {
        if let Some(parent) = context.embedded_catalog_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(serialized) = serde_json::to_string_pretty(&catalog) {
            let _ = fs::write(&context.embedded_catalog_path, serialized);
        }
    }

    write_runtime_driver_manifest(&context, &manifest)?;
    finalize_runtime_driver_manifest(manifest, &config.runtime_driver_policy)
}

pub fn write_runtime_driver_manifest(
    context: &ResolvedRuntimeDriverContext,
    manifest: &RuntimeDriverManifest,
) -> BitOSDTResult<()> {
    if let Some(parent) = context.resolved_manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut manifest = manifest.clone();
    manifest
        .warnings
        .push(format!("Manifest updated at {}", Utc::now().to_rfc3339()));
    fs::write(
        &context.resolved_manifest_path,
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

fn filter_catalog_for_manufacturer(catalog: &[DriverPack], manufacturer: &str) -> Vec<DriverPack> {
    let normalized = DriverManager::normalize_manufacturer(manufacturer);
    catalog
        .iter()
        .filter(|driverpack| driverpack.manufacturer.eq_ignore_ascii_case(&normalized))
        .cloned()
        .collect()
}

fn merge_catalogs(mut existing: Vec<DriverPack>, fresh: Vec<DriverPack>) -> Vec<DriverPack> {
    for entry in fresh {
        if let Some(current) = existing.iter_mut().find(|current| current.id == entry.id) {
            *current = entry;
        } else {
            existing.push(entry);
        }
    }
    existing
}

fn finalize_runtime_driver_manifest(
    manifest: RuntimeDriverManifest,
    policy: &crate::core::RuntimeDriverPolicy,
) -> BitOSDTResult<RuntimeDriverManifest> {
    let has_failure = !manifest.prepared
        && (manifest.matched_driverpack.is_some()
            || manifest
                .warnings
                .iter()
                .any(|warning| warning.to_ascii_lowercase().contains("failed")));
    if has_failure && policy.failure_policy == RuntimeDriverFailurePolicy::Fail {
        return Err(BitOSDTError::Driver(
            manifest
                .warnings
                .last()
                .cloned()
                .unwrap_or_else(|| "Runtime driver workflow failed".to_string()),
        ));
    }

    Ok(manifest)
}

fn sanitize_runtime_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => ch,
            _ => '_',
        })
        .collect()
}
