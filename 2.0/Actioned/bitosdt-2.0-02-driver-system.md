# BitOSDT 2.0 - Driver System Architecture

## Overview

The driver system provides automatic hardware detection and driver installation during Windows deployment. It focuses on reliability and resilience through a catalog sync architecture with local fallback.

## Key Architectural Decisions

1. **DriverPack Focus for v1.0** - Manufacturer driver packs are the primary driver source during WinPE deployment. They are reliable, well-tested, and don't require fragile API access.

2. **CloudDriver Deferred** - Microsoft Update Catalog querying is deferred to post-deployment. There is no official API, and web scraping is fragile for a critical boot path.

3. **JSON Catalog Format** - BitOSDT maintains its own JSON catalog format, synced from OSDCloud's XML catalogs. This provides:
   - Clean serde integration in Rust
   - Local cache for offline/resilient operation
   - Ability to extend with custom entries

4. **Sync + Cache + Fallback** - Catalog syncs periodically from OSDCloud. If sync fails, local cache is used. Application never depends on live network access during deployment.

---

## Driver Sources

### 1. DriverPack (Primary - WinPE Deployment)

Complete driver packs from hardware manufacturers, downloaded during WinPE deployment.

**Supported Manufacturers:**
- Dell (Latitude, OptiPlex, Precision)
- HP (EliteBook, ProBook, ProDesk, ZBook)
- Lenovo (ThinkPad, ThinkCentre, ThinkStation)
- Microsoft Surface
- (Extensible for others)

**How It Works:**
```
1. Hardware detected → "Dell, SKU: 0A5D"
2. Query local catalog → Find matching DriverPack entry
3. Download from Dell CDN → https://dl.dell.com/...
4. Extract and inject drivers into Windows image
```

### 2. CloudDriver (Deferred - Post-Deployment)

> **Note:** CloudDriver is deferred to v1.1 as a post-deployment feature.
> It will run after Windows first boot using Windows Update APIs.

Dynamic driver download from Microsoft Update Catalog based on detected hardware IDs.

**Why Deferred:**
- No official Microsoft REST API exists
- Web scraping is fragile and breaks when Microsoft changes their site
- WSUS COM objects don't work reliably in WinPE
- DriverPacks cover 90%+ of enterprise hardware

**Future Implementation (Post-Deployment):**
```rust
// Runs after Windows first boot, not in WinPE
pub struct CloudDriverManager {
    // Uses Windows Update Agent COM API
    // Only available in full Windows, not WinPE
}
```

---

## Catalog Architecture

### Sync + Cache + Fallback Pattern

```
┌─────────────────────────────────────────────────────────────────┐
│  Build System (Host OS)                                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────┐     ┌─────────────────┐                    │
│  │ OSDCloud GitHub │────▶│ Catalog Sync    │                    │
│  │ (XML Catalogs)  │     │ Service         │                    │
│  └─────────────────┘     └────────┬────────┘                    │
│                                   │                              │
│                          ┌────────▼────────┐                    │
│                          │ XML → JSON      │                    │
│                          │ Converter       │                    │
│                          └────────┬────────┘                    │
│                                   │                              │
│                          ┌────────▼────────┐                    │
│                          │ Local SQLite    │◀── Authoritative   │
│                          │ Database        │    Source          │
│                          └────────┬────────┘                    │
│                                   │                              │
│  ┌────────────────────────────────┼────────────────────────────┐│
│  │ Sync Success                   │           Sync Failure     ││
│  │ → Update DB                    │           → Use existing   ││
│  │ → Note timestamp               │           → Warn user      ││
│  └────────────────────────────────┴────────────────────────────┘│
│                                                                  │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  WinPE Deployment (Target Device)                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Catalog is EMBEDDED in boot media (JSON snapshot)              │
│  → No network dependency for catalog lookup                     │
│  → Downloads only the matched DriverPack                        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### OSDCloud Catalog Source

OSDCloud stores catalogs in XML format in their GitHub repository:

```
https://github.com/OSDeploy/OSD/tree/master/Catalogs/
├── CloudDriver/           # Microsoft Update driver mappings
└── DriverPack/            # Manufacturer driver packs
    ├── Dell.xml
    ├── HP.xml
    ├── Lenovo.xml
    └── Microsoft.xml
```

### BitOSDT JSON Catalog Format

BitOSDT converts to JSON and stores in local SQLite:

```json
{
  "id": "dell-latitude-5520-24h2",
  "manufacturer": "Dell",
  "product": "0A5D",
  "model": "Latitude 5520",
  "os": "Windows 11",
  "os_version": "24H2",
  "os_build": "26100",
  "architecture": "x64",
  "name": "Dell Latitude 5520 Win11 24H2 Driver Pack",
  "filename": "5520-win11-A04.cab",
  "url": "https://dl.dell.com/FOLDER09876543/1/5520-win11-A04.cab",
  "hash_md5": "ABC123DEF456...",
  "hash_sha256": "789XYZ...",
  "size_bytes": 524288000,
  "release_date": "2024-09-23",
  "catalog_version": "24.10.1",
  "last_synced": "2024-10-15T10:30:00Z"
}
```

---

## Data Structures

### DriverPack Model

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPack {
    pub id: String,
    pub manufacturer: String,
    pub product: String,              // SKU for matching (Dell/HP) or model prefix (Lenovo)
    pub model: String,                // Human-readable model name
    pub os: String,                   // "Windows 11"
    pub os_version: String,           // "24H2"
    pub os_build: Option<String>,     // "26100"
    pub architecture: Architecture,   // x64, arm64
    pub name: String,                 // Display name
    pub filename: String,             // Downloaded filename
    pub url: String,                  // Download URL
    pub hash_md5: String,             // Required for verification
    pub hash_sha256: Option<String>,  // Optional stronger hash
    pub size_bytes: Option<u64>,      // For progress display
    pub release_date: Option<String>, // ISO date
    pub catalog_version: String,      // Source catalog version
    pub last_synced: DateTime<Utc>,   // When we synced this entry
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    X64,
    Arm64,
}

impl Architecture {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        }
    }
}
```

### Catalog Sync Status

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSyncStatus {
    pub manufacturer: String,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_sync_success: bool,
    pub entry_count: u32,
    pub source_url: String,
    pub error_message: Option<String>,
}
```

---

## Catalog Sync Service

### XML Parser for OSDCloud Format

```rust
use quick_xml::de::from_str;
use serde::Deserialize;

// OSDCloud XML structure
#[derive(Debug, Deserialize)]
#[serde(rename = "DriverPack")]
struct OsdCloudDriverPackXml {
    #[serde(rename = "Product")]
    product: String,
    #[serde(rename = "Model")]
    model: Option<String>,
    #[serde(rename = "OS")]
    os: String,
    #[serde(rename = "OSVersion")]
    os_version: String,
    #[serde(rename = "Url")]
    url: String,
    #[serde(rename = "FileName")]
    filename: String,
    #[serde(rename = "Hash")]
    hash: String,
    #[serde(rename = "ReleaseDate")]
    release_date: Option<String>,
}

impl OsdCloudDriverPackXml {
    fn to_driverpack(&self, manufacturer: &str) -> DriverPack {
        DriverPack {
            id: format!("{}-{}-{}",
                manufacturer.to_lowercase(),
                self.product.to_lowercase(),
                self.os_version.to_lowercase()
            ),
            manufacturer: manufacturer.to_string(),
            product: self.product.clone(),
            model: self.model.clone().unwrap_or_default(),
            os: self.os.clone(),
            os_version: self.os_version.clone(),
            os_build: None,
            architecture: Architecture::X64, // Parsed from filename/os
            name: format!("{} {} {} Driver Pack", manufacturer, self.product, self.os_version),
            filename: self.filename.clone(),
            url: self.url.clone(),
            hash_md5: self.hash.clone(),
            hash_sha256: None,
            size_bytes: None,
            release_date: self.release_date.clone(),
            catalog_version: "synced".to_string(),
            last_synced: Utc::now(),
        }
    }
}
```

### Sync Service

```rust
pub struct CatalogSyncService {
    client: reqwest::Client,
    db: Database,
}

impl CatalogSyncService {
    const OSDCLOUD_CATALOG_BASE: &'static str =
        "https://raw.githubusercontent.com/OSDeploy/OSD/master/Catalogs/DriverPack";

    const MANUFACTURERS: &'static [&'static str] = &["Dell", "HP", "Lenovo", "Microsoft"];

    pub async fn sync_all(&self) -> Result<Vec<CatalogSyncStatus>> {
        let mut results = Vec::new();

        for manufacturer in Self::MANUFACTURERS {
            let status = self.sync_manufacturer(manufacturer).await;
            results.push(status);
        }

        Ok(results)
    }

    pub async fn sync_manufacturer(&self, manufacturer: &str) -> CatalogSyncStatus {
        let url = format!("{}/{}.xml", Self::OSDCLOUD_CATALOG_BASE, manufacturer);

        info!("Syncing {} catalog from {}", manufacturer, url);

        match self.fetch_and_parse(&url, manufacturer).await {
            Ok(driverpacks) => {
                let count = driverpacks.len() as u32;

                // Merge into database
                if let Err(e) = self.db.upsert_driverpacks(&driverpacks) {
                    return CatalogSyncStatus {
                        manufacturer: manufacturer.to_string(),
                        last_sync: Some(Utc::now()),
                        last_sync_success: false,
                        entry_count: 0,
                        source_url: url,
                        error_message: Some(format!("Database error: {}", e)),
                    };
                }

                info!("Synced {} entries for {}", count, manufacturer);

                CatalogSyncStatus {
                    manufacturer: manufacturer.to_string(),
                    last_sync: Some(Utc::now()),
                    last_sync_success: true,
                    entry_count: count,
                    source_url: url,
                    error_message: None,
                }
            }
            Err(e) => {
                warn!("Failed to sync {} catalog: {}", manufacturer, e);

                // Get existing count from DB
                let existing_count = self.db.count_driverpacks(manufacturer).unwrap_or(0);

                CatalogSyncStatus {
                    manufacturer: manufacturer.to_string(),
                    last_sync: Some(Utc::now()),
                    last_sync_success: false,
                    entry_count: existing_count,
                    source_url: url,
                    error_message: Some(e.to_string()),
                }
            }
        }
    }

    async fn fetch_and_parse(
        &self,
        url: &str,
        manufacturer: &str,
    ) -> Result<Vec<DriverPack>> {
        let response = self.client
            .get(url)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(DriverError::CatalogFetchFailed(
                format!("HTTP {}", response.status())
            ));
        }

        let xml_content = response.text().await?;

        // Parse XML catalog
        let xml_packs: Vec<OsdCloudDriverPackXml> = from_str(&xml_content)
            .map_err(|e| DriverError::CatalogFetchFailed(
                format!("XML parse error: {}", e)
            ))?;

        // Convert to our format
        let driverpacks: Vec<DriverPack> = xml_packs
            .into_iter()
            .map(|x| x.to_driverpack(manufacturer))
            .collect();

        Ok(driverpacks)
    }
}
```

### Catalog Query (Local Database)

```rust
pub struct CatalogManager {
    db: Database,
}

impl CatalogManager {
    /// Query local database for DriverPacks - never hits network
    pub fn get_driverpacks(&self, manufacturer: &str) -> Result<Vec<DriverPack>> {
        self.db.get_driverpacks_by_manufacturer(manufacturer)
    }

    /// Get all manufacturers with catalog data
    pub fn get_manufacturers(&self) -> Result<Vec<String>> {
        self.db.get_distinct_manufacturers()
    }

    /// Get sync status for all catalogs
    pub fn get_sync_status(&self) -> Result<Vec<CatalogSyncStatus>> {
        self.db.get_catalog_sync_status()
    }

    /// Export catalog snapshot for embedding in WinPE
    pub fn export_catalog_snapshot(&self) -> Result<String> {
        let all_packs = self.db.get_all_driverpacks()?;
        Ok(serde_json::to_string_pretty(&all_packs)?)
    }
}
```

---

## Driver Matching Algorithm

### Hardware to DriverPack Matching

```rust
#[derive(Debug, Clone)]
pub struct DriverMatch {
    pub driverpack: DriverPack,
    pub confidence: MatchConfidence,
    pub match_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchConfidence {
    Exact,      // Product SKU matches exactly
    High,       // Model name matches
    Medium,     // Manufacturer + OS version matches
    Low,        // Only manufacturer matches
    None,       // No match
}

pub fn find_matching_driverpack(
    hardware: &HardwareInfo,
    driverpacks: &[DriverPack],
    os_version: &str,
) -> Option<DriverMatch> {
    let mut best_match: Option<DriverMatch> = None;

    for dp in driverpacks {
        // Must match manufacturer and architecture
        if dp.manufacturer != hardware.manufacturer {
            continue;
        }
        if dp.architecture != hardware.architecture {
            continue;
        }

        // Prefer matching OS version
        let os_matches = dp.os_version == os_version;

        // Calculate match confidence
        let (confidence, reason) = match hardware.manufacturer.as_str() {
            "Dell" => {
                if dp.product == hardware.product {
                    (MatchConfidence::Exact, format!("SKU match: {}", hardware.product))
                } else if dp.model.to_lowercase().contains(&hardware.model.to_lowercase()) {
                    (MatchConfidence::High, format!("Model match: {}", hardware.model))
                } else {
                    continue;
                }
            }
            "HP" => {
                if dp.product == hardware.product {
                    (MatchConfidence::Exact, format!("BaseBoard match: {}", hardware.product))
                } else if dp.model.to_lowercase().contains(&hardware.model.to_lowercase()) {
                    (MatchConfidence::High, format!("Model match: {}", hardware.model))
                } else {
                    continue;
                }
            }
            "Lenovo" => {
                // Lenovo uses first 4 characters of model
                let model_prefix = &hardware.model[..4.min(hardware.model.len())];
                if dp.product.starts_with(model_prefix) {
                    (MatchConfidence::Exact, format!("Model prefix match: {}", model_prefix))
                } else if dp.model.to_lowercase().contains(&hardware.model.to_lowercase()) {
                    (MatchConfidence::High, format!("Model name match: {}", hardware.model))
                } else {
                    continue;
                }
            }
            "Microsoft" => {
                if dp.product == hardware.product {
                    (MatchConfidence::Exact, format!("Surface SKU match: {}", hardware.product))
                } else if dp.model == hardware.model {
                    (MatchConfidence::High, format!("Surface model match: {}", hardware.model))
                } else {
                    continue;
                }
            }
            _ => {
                if dp.model == hardware.model {
                    (MatchConfidence::High, format!("Model match: {}", hardware.model))
                } else {
                    continue;
                }
            }
        };

        // Downgrade confidence if OS version doesn't match
        let final_confidence = if os_matches {
            confidence
        } else {
            match confidence {
                MatchConfidence::Exact => MatchConfidence::High,
                MatchConfidence::High => MatchConfidence::Medium,
                _ => MatchConfidence::Low,
            }
        };

        let candidate = DriverMatch {
            driverpack: dp.clone(),
            confidence: final_confidence,
            match_reason: reason,
        };

        // Keep best match
        match &best_match {
            None => best_match = Some(candidate),
            Some(current) if candidate.confidence > current.confidence => {
                best_match = Some(candidate)
            }
            _ => {}
        }
    }

    // Only return if confidence is at least Medium
    best_match.filter(|m| m.confidence >= MatchConfidence::Medium)
}
```

---

## Driver Download and Extraction

### Download Manager

```rust
pub struct DriverDownloadManager {
    client: reqwest::Client,
    cache_dir: PathBuf,
}

impl DriverDownloadManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))  // 5 min timeout for large files
            .build()
            .expect("Failed to create HTTP client");

        Self { client, cache_dir }
    }

    pub async fn download_driverpack(
        &self,
        driverpack: &DriverPack,
        progress_callback: impl Fn(u64, u64),
    ) -> Result<PathBuf> {
        let file_path = self.cache_dir.join(&driverpack.filename);

        // Check if already downloaded and valid
        if file_path.exists() {
            if self.verify_hash(&file_path, &driverpack.hash_md5).await? {
                info!("DriverPack already cached and verified: {}", driverpack.filename);
                return Ok(file_path);
            } else {
                warn!("Cached file hash mismatch, re-downloading");
                fs::remove_file(&file_path).await?;
            }
        }

        info!("Downloading DriverPack: {}", driverpack.name);
        info!("URL: {}", driverpack.url);

        // Download with progress
        let response = self.client.get(&driverpack.url).send().await?;

        if !response.status().is_success() {
            return Err(DriverError::DownloadFailed(
                format!("HTTP {}", response.status())
            ));
        }

        let total_size = response.content_length()
            .or(driverpack.size_bytes)
            .unwrap_or(0);

        let mut file = fs::File::create(&file_path).await?;
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            progress_callback(downloaded, total_size);
        }

        file.flush().await?;

        // Verify hash
        if !self.verify_hash(&file_path, &driverpack.hash_md5).await? {
            fs::remove_file(&file_path).await?;
            return Err(DriverError::HashMismatch {
                file: driverpack.filename.clone(),
            });
        }

        info!("Download complete and verified: {}", driverpack.filename);

        Ok(file_path)
    }

    async fn verify_hash(&self, file_path: &Path, expected_md5: &str) -> Result<bool> {
        use md5::{Md5, Digest};
        use tokio::io::AsyncReadExt;

        let mut file = fs::File::open(file_path).await?;
        let mut hasher = Md5::new();
        let mut buffer = vec![0u8; 65536];  // 64KB buffer

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let result = hasher.finalize();
        let hash = format!("{:x}", result);

        Ok(hash.eq_ignore_ascii_case(expected_md5))
    }
}
```

### Extraction Strategies

```rust
use std::process::Command;
use tokio::process::Command as AsyncCommand;

pub trait Extractor: Send + Sync {
    fn extract(&self, source: &Path, destination: &Path) -> Result<()>;
}

/// CAB files - Windows cabinet format
pub struct CabExtractor;

impl Extractor for CabExtractor {
    fn extract(&self, source: &Path, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        // Use expand.exe (built into Windows)
        let output = Command::new("expand.exe")
            .args([
                source.to_str().unwrap(),
                "-F:*",
                destination.to_str().unwrap(),
            ])
            .output()?;

        if !output.status.success() {
            return Err(DriverError::ExtractionFailed(
                String::from_utf8_lossy(&output.stderr).to_string()
            ));
        }

        Ok(())
    }
}

/// ZIP files
pub struct ZipExtractor;

impl Extractor for ZipExtractor {
    fn extract(&self, source: &Path, destination: &Path) -> Result<()> {
        let file = std::fs::File::open(source)?;
        let mut archive = zip::ZipArchive::new(file)?;
        archive.extract(destination)?;
        Ok(())
    }
}

/// Dell self-extracting EXE files
pub struct DellExeExtractor;

impl Extractor for DellExeExtractor {
    fn extract(&self, source: &Path, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        // Dell executables support /s (silent) /e=path (extract to)
        let output = Command::new(source)
            .args([
                "/s",
                &format!("/e={}", destination.display()),
            ])
            .output()?;

        // Dell extractors may return non-zero even on success
        // Check if files were extracted
        if destination.read_dir()?.next().is_none() {
            return Err(DriverError::ExtractionFailed(
                "No files extracted from Dell package".to_string()
            ));
        }

        Ok(())
    }
}

/// HP SoftPaq files (7-Zip SFX)
pub struct HpSoftPaqExtractor;

impl Extractor for HpSoftPaqExtractor {
    fn extract(&self, source: &Path, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        // HP SoftPaqs are 7-Zip self-extracting archives
        // Try 7z.exe first, fall back to built-in extraction
        let output = Command::new("7z.exe")
            .args([
                "x",
                "-y",
                source.to_str().unwrap(),
                &format!("-o{}", destination.display()),
            ])
            .output();

        match output {
            Ok(o) if o.status.success() => Ok(()),
            _ => {
                // Fallback: run the SoftPaq with /s /e /f path
                let output = Command::new(source)
                    .args([
                        "/s",
                        "/e",
                        "/f",
                        destination.to_str().unwrap(),
                    ])
                    .output()?;

                if destination.read_dir()?.next().is_none() {
                    return Err(DriverError::ExtractionFailed(
                        "No files extracted from HP SoftPaq".to_string()
                    ));
                }

                Ok(())
            }
        }
    }
}

/// Lenovo SCCM packages (typically EXE that extracts)
pub struct LenovoExtractor;

impl Extractor for LenovoExtractor {
    fn extract(&self, source: &Path, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        // Lenovo SCCM packages: /VERYSILENT /DIR=path /EXTRACT
        let output = Command::new(source)
            .args([
                "/VERYSILENT",
                &format!("/DIR={}", destination.display()),
                "/EXTRACT",
            ])
            .output()?;

        if destination.read_dir()?.next().is_none() {
            return Err(DriverError::ExtractionFailed(
                "No files extracted from Lenovo package".to_string()
            ));
        }

        Ok(())
    }
}

/// Factory function to get appropriate extractor
pub fn get_extractor(filename: &str, manufacturer: &str) -> Box<dyn Extractor> {
    let lower = filename.to_lowercase();

    // By file extension
    if lower.ends_with(".cab") {
        return Box::new(CabExtractor);
    }
    if lower.ends_with(".zip") {
        return Box::new(ZipExtractor);
    }

    // By manufacturer
    match manufacturer {
        "Dell" => Box::new(DellExeExtractor),
        "HP" => Box::new(HpSoftPaqExtractor),
        "Lenovo" => Box::new(LenovoExtractor),
        _ => Box::new(ZipExtractor),  // Default fallback
    }
}
```

---

## Driver Installation

### Offline Driver Injection (WinPE → Offline Windows)

Inject drivers into the offline Windows image before first boot:

```rust
pub struct OfflineDriverInstaller;

impl OfflineDriverInstaller {
    /// Inject drivers into offline Windows image using DISM
    pub fn install_drivers(
        &self,
        windows_path: &Path,      // e.g., W:\ (mounted Windows partition)
        driver_path: &Path,       // Path to extracted drivers
    ) -> Result<InstallationResult> {
        info!("Injecting drivers from {} into {}",
            driver_path.display(),
            windows_path.display()
        );

        // Note: DISM requires /Flag:Value as a single argument
        let output = Command::new("dism.exe")
            .args([
                format!("/Image:{}", windows_path.display()),
                "/Add-Driver".to_string(),
                format!("/Driver:{}", driver_path.display()),
                "/Recurse".to_string(),
                "/ForceUnsigned".to_string(),
            ])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse DISM output for driver count
        let installed = Self::parse_driver_count(&stdout);

        if !output.status.success() {
            warn!("DISM returned error: {}", stderr);
            // DISM may return error but still install some drivers
        }

        info!("Driver injection complete: {} drivers installed", installed);

        Ok(InstallationResult {
            installed,
            failed: 0,  // DISM doesn't give per-driver failure count
            log: stdout.to_string(),
        })
    }

    fn parse_driver_count(output: &str) -> u32 {
        // Look for "Successfully installed driver" messages
        output.lines()
            .filter(|line| line.contains("Successfully") || line.contains("installed"))
            .count() as u32
    }
}

#[derive(Debug, Clone)]
pub struct InstallationResult {
    pub installed: u32,
    pub failed: u32,
    pub log: String,
}
```

---

## Configuration

### Driver Preferences

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPreferences {
    /// Use manufacturer DriverPacks (recommended)
    pub use_driverpacks: bool,

    /// Use CloudDriver for post-deployment (v1.1 feature)
    #[serde(default)]
    pub use_cloud_drivers_post_deploy: bool,

    /// Allow unsigned drivers (needed for some hardware)
    pub allow_unsigned_drivers: bool,

    /// Path to offline driver cache (for air-gapped deployments)
    pub offline_driver_cache: Option<PathBuf>,

    /// Embed drivers in WinPE boot media
    pub embed_drivers_in_winpe: bool,
}

impl Default for DriverPreferences {
    fn default() -> Self {
        Self {
            use_driverpacks: true,
            use_cloud_drivers_post_deploy: false,  // Disabled until v1.1
            allow_unsigned_drivers: true,
            offline_driver_cache: None,
            embed_drivers_in_winpe: false,
        }
    }
}
```

---

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("Failed to download driver: {0}")]
    DownloadFailed(String),

    #[error("Hash verification failed for {file}")]
    HashMismatch { file: String },

    #[error("No matching DriverPack found for {manufacturer} {model}")]
    NoMatchingDriverPack { manufacturer: String, model: String },

    #[error("Extraction failed: {0}")]
    ExtractionFailed(String),

    #[error("Driver installation failed: {0}")]
    InstallationFailed(String),

    #[error("Catalog fetch failed: {0}")]
    CatalogFetchFailed(String),

    #[error("Catalog sync failed: {0}")]
    CatalogSyncFailed(String),

    #[error("Invalid driver file: {0}")]
    InvalidDriverFile(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
```

---

## Integration with Deployment Engine

```rust
impl DeploymentEngine {
    /// Prepare drivers for deployment (runs in WinPE)
    pub async fn prepare_drivers(
        &self,
        hardware: &HardwareInfo,
        config: &DeployConfig,
    ) -> Result<PathBuf> {
        let drivers_dir = self.temp_dir.join("drivers");
        fs::create_dir_all(&drivers_dir).await?;

        if !config.driver_prefs.use_driverpacks {
            info!("DriverPack support disabled, skipping driver download");
            return Ok(drivers_dir);
        }

        // Load catalog from embedded JSON (no network needed for lookup)
        let catalog = self.load_embedded_catalog()?;

        let driverpacks = catalog.get(&hardware.manufacturer)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        // Find matching DriverPack
        match find_matching_driverpack(hardware, driverpacks, &config.os_version) {
            Some(driver_match) => {
                info!(
                    "Found DriverPack: {} (confidence: {:?})",
                    driver_match.driverpack.name,
                    driver_match.confidence
                );
                info!("Match reason: {}", driver_match.match_reason);

                // Download DriverPack
                let download_mgr = DriverDownloadManager::new(drivers_dir.clone());
                let archive = download_mgr
                    .download_driverpack(&driver_match.driverpack, |downloaded, total| {
                        self.progress.update_driver_download(downloaded, total);
                    })
                    .await?;

                // Extract
                let extractor = get_extractor(
                    &driver_match.driverpack.filename,
                    &hardware.manufacturer,
                );
                let extract_dir = drivers_dir.join("driverpack");
                extractor.extract(&archive, &extract_dir)?;

                info!("DriverPack extracted to: {}", extract_dir.display());
            }
            None => {
                warn!(
                    "No matching DriverPack found for {} {}",
                    hardware.manufacturer,
                    hardware.model
                );
                warn!("Windows will attempt to find drivers during setup");
                // This is not fatal - Windows can find many drivers on its own
            }
        }

        Ok(drivers_dir)
    }
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driverpack_matching_dell() {
        let hardware = HardwareInfo {
            manufacturer: "Dell".to_string(),
            product: "0A5D".to_string(),
            model: "Latitude 5520".to_string(),
            architecture: Architecture::X64,
            ..Default::default()
        };

        let driverpacks = vec![
            DriverPack {
                manufacturer: "Dell".to_string(),
                product: "0A5D".to_string(),
                model: "Latitude 5520".to_string(),
                os_version: "24H2".to_string(),
                architecture: Architecture::X64,
                ..Default::default()
            },
        ];

        let matched = find_matching_driverpack(&hardware, &driverpacks, "24H2");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().confidence, MatchConfidence::Exact);
    }

    #[test]
    fn test_driverpack_matching_lenovo() {
        let hardware = HardwareInfo {
            manufacturer: "Lenovo".to_string(),
            product: "".to_string(),
            model: "20WES1234".to_string(),  // ThinkPad model format
            architecture: Architecture::X64,
            ..Default::default()
        };

        let driverpacks = vec![
            DriverPack {
                manufacturer: "Lenovo".to_string(),
                product: "20WE".to_string(),  // First 4 chars
                model: "ThinkPad T14 Gen 2".to_string(),
                os_version: "24H2".to_string(),
                architecture: Architecture::X64,
                ..Default::default()
            },
        ];

        let matched = find_matching_driverpack(&hardware, &driverpacks, "24H2");
        assert!(matched.is_some());
    }

    #[test]
    fn test_no_match_different_arch() {
        let hardware = HardwareInfo {
            manufacturer: "Dell".to_string(),
            product: "0A5D".to_string(),
            model: "Latitude 5520".to_string(),
            architecture: Architecture::Arm64,
            ..Default::default()
        };

        let driverpacks = vec![
            DriverPack {
                manufacturer: "Dell".to_string(),
                product: "0A5D".to_string(),
                architecture: Architecture::X64,  // Different arch
                ..Default::default()
            },
        ];

        let matched = find_matching_driverpack(&hardware, &driverpacks, "24H2");
        assert!(matched.is_none());
    }

    #[test]
    fn test_get_extractor() {
        assert!(matches!(
            get_extractor("driver.cab", "Dell").as_ref(),
            &CabExtractor
        ));

        assert!(matches!(
            get_extractor("sp12345.exe", "HP").as_ref(),
            &HpSoftPaqExtractor
        ));
    }
}
```

---

## Future: CloudDriver (Post-Deployment)

> This section documents the planned v1.1 CloudDriver feature.

After Windows first boot, a scheduled task can run CloudDriver to fetch additional drivers:

```rust
// Runs in full Windows after deployment, not in WinPE
pub struct PostDeployCloudDriver {
    // Uses Windows Update Agent COM API
}

impl PostDeployCloudDriver {
    pub async fn install_missing_drivers(&self) -> Result<()> {
        // Query Device Manager for devices with missing drivers
        // Search Windows Update for matching drivers
        // Download and install

        // This uses:
        // - IUpdateSearcher from Windows Update Agent
        // - Or PSWindowsUpdate PowerShell module

        todo!("Planned for v1.1")
    }
}
```

This approach:
- Runs in full Windows where APIs are available
- Not in critical boot path
- Can fail gracefully (Windows already booted)
- User can manually install drivers if needed
