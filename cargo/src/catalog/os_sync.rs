use crate::core::database::Database;
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

/// Represents an OS version entry from the OSDCloud catalog or local database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsVersionEntry {
    pub id: String,
    pub display_name: String,
    pub operating_system: String,
    pub release_id: String,
    pub build: String,
    pub architecture: String,
    pub language_code: String,
    pub license: String,
    pub size_bytes: Option<u64>,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub download_url: String,
    pub last_synced: DateTime<Utc>,
}

impl OsVersionEntry {
    /// Generate a unique ID from the entry's properties
    pub fn generate_id(
        release_id: &str,
        architecture: &str,
        language_code: &str,
        license: &str,
    ) -> String {
        format!(
            "{}-{}-{}-{}",
            release_id.to_lowercase(),
            architecture.to_lowercase(),
            language_code.to_lowercase(),
            license.to_lowercase()
        )
    }

    /// Get a short alias like "win11-25h2" for easy CLI usage
    pub fn short_alias(&self) -> String {
        let os_prefix = if self.operating_system.contains("11") {
            "win11"
        } else if self.operating_system.contains("10") {
            "win10"
        } else {
            "windows"
        };
        format!("{}-{}", os_prefix, self.release_id.to_lowercase())
    }
}

/// JSON structure from OSDCloud's build-operatingsystems.json
#[derive(Debug, Deserialize)]
pub struct OsdCloudOsVersion {
    #[serde(rename = "DisplayName")]
    pub display_name: String,

    #[serde(rename = "OperatingSystem")]
    pub operating_system: String,

    #[serde(rename = "ReleaseId")]
    pub release_id: String,

    #[serde(rename = "Build")]
    pub build: String,

    #[serde(rename = "Architecture")]
    pub architecture: String,

    #[serde(rename = "LanguageCode")]
    pub language_code: String,

    #[serde(rename = "License")]
    pub license: String,

    #[serde(rename = "Size")]
    pub size: String,

    #[serde(rename = "Sha1")]
    pub sha1: Option<String>,

    #[serde(rename = "Sha256")]
    pub sha256: Option<String>,

    #[serde(rename = "Url")]
    pub url: String,
}

impl OsdCloudOsVersion {
    /// Convert OSDCloud JSON entry to our internal format
    pub fn to_os_version_entry(&self) -> OsVersionEntry {
        let id = OsVersionEntry::generate_id(
            &self.release_id,
            &self.architecture,
            &self.language_code,
            &self.license,
        );

        let size_bytes = self.size.parse::<u64>().ok();

        OsVersionEntry {
            id,
            display_name: self.display_name.clone(),
            operating_system: self.operating_system.clone(),
            release_id: self.release_id.clone(),
            build: self.build.clone(),
            architecture: self.architecture.clone(),
            language_code: self.language_code.clone(),
            license: self.license.clone(),
            size_bytes,
            sha1: self.sha1.clone(),
            sha256: self.sha256.clone(),
            download_url: self.url.clone(),
            last_synced: Utc::now(),
        }
    }
}

/// Status returned after syncing the OS catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsCatalogSyncStatus {
    pub last_sync: Option<DateTime<Utc>>,
    pub last_sync_success: bool,
    pub entry_count: u32,
    pub source_url: String,
    pub error_message: Option<String>,
}

/// Service for syncing OS catalog from OSDCloud's GitHub repository
pub struct OsCatalogSyncService {
    db: Database,
}

/// OSDCloud OS catalog JSON URL
pub const OSDCLOUD_OS_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/OSDeploy/OSD/master/cache/os-catalogs/build-operatingsystems.json";

/// Fetch OS catalog entries from OSDCloud (async, no DB access)
/// This function is Send-safe as it doesn't hold any non-Send types across await points
pub async fn fetch_os_catalog() -> BitOSDTResult<Vec<OsVersionEntry>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| BitOSDTError::Network(e.to_string()))?;

    info!("Fetching OS catalog from {}", OSDCLOUD_OS_CATALOG_URL);

    let response = client
        .get(OSDCLOUD_OS_CATALOG_URL)
        .send()
        .await
        .map_err(|e| BitOSDTError::Network(format!("Failed to fetch OS catalog: {}", e)))?;

    if !response.status().is_success() {
        return Err(BitOSDTError::CatalogSync(format!(
            "HTTP {} fetching OS catalog",
            response.status()
        )));
    }

    let json_content = response.text().await.map_err(|e| {
        BitOSDTError::CatalogSync(format!("Failed to read OS catalog response: {}", e))
    })?;

    // Parse JSON array
    let osdcloud_entries: Vec<OsdCloudOsVersion> =
        serde_json::from_str(&json_content).map_err(|e| {
            BitOSDTError::CatalogSync(format!("Failed to parse OS catalog JSON: {}", e))
        })?;

    // Convert to our format
    let entries: Vec<OsVersionEntry> = osdcloud_entries
        .into_iter()
        .map(|e| e.to_os_version_entry())
        .collect();

    info!("Fetched {} OS catalog entries", entries.len());
    Ok(entries)
}

impl OsCatalogSyncService {
    pub fn new(db: Database) -> BitOSDTResult<Self> {
        Ok(Self { db })
    }

    /// Sync OS catalog from OSDCloud: fetch entries and save to database
    pub async fn sync(&self) -> BitOSDTResult<OsCatalogSyncStatus> {
        match fetch_os_catalog().await {
            Ok(entries) => self.save_entries(entries),
            Err(e) => {
                let status = self.failure_status(e.to_string());
                Ok(status)
            }
        }
    }

    /// Save fetched OS catalog entries to the database (synchronous)
    pub fn save_entries(&self, entries: Vec<OsVersionEntry>) -> BitOSDTResult<OsCatalogSyncStatus> {
        // Clear old entries and insert new ones
        if let Err(e) = self.db.clear_os_versions() {
            warn!("Failed to clear old OS versions: {}", e);
        }

        let mut success_count = 0;
        for entry in &entries {
            if let Err(e) = self.db.create_os_version(entry) {
                warn!("Failed to save OS version {}: {}", entry.id, e);
            } else {
                success_count += 1;
            }
        }

        info!("Saved {} OS version entries to database", success_count);

        Ok(OsCatalogSyncStatus {
            last_sync: Some(Utc::now()),
            last_sync_success: true,
            entry_count: success_count,
            source_url: OSDCLOUD_OS_CATALOG_URL.to_string(),
            error_message: None,
        })
    }

    /// Create a failure status (for when fetch fails)
    pub fn failure_status(&self, error: String) -> OsCatalogSyncStatus {
        let existing_count = self.db.count_os_versions().unwrap_or(0);

        OsCatalogSyncStatus {
            last_sync: Some(Utc::now()),
            last_sync_success: false,
            entry_count: existing_count,
            source_url: OSDCLOUD_OS_CATALOG_URL.to_string(),
            error_message: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id() {
        let id = OsVersionEntry::generate_id("25H2", "amd64", "en-us", "Retail");
        assert_eq!(id, "25h2-amd64-en-us-retail");
    }

    #[test]
    fn test_parse_osdcloud_entry() {
        let json = r#"{
            "DisplayName": "Win11-25H2-amd64",
            "OperatingSystem": "Windows 11",
            "ReleaseId": "25H2",
            "Build": "26200.7623",
            "Architecture": "amd64",
            "LanguageCode": "en-us",
            "License": "Retail",
            "Size": "5664921115",
            "Sha1": null,
            "Sha256": "739e6a6f00a5cd6b8795a9095313e363ed3b03f13b854d745060397ef2bf6948",
            "Url": "http://dl.delivery.mp.microsoft.com/filestreamingservice/files/example.esd"
        }"#;

        let entry: OsdCloudOsVersion = serde_json::from_str(json).unwrap();
        let converted = entry.to_os_version_entry();

        assert_eq!(converted.id, "25h2-amd64-en-us-retail");
        assert_eq!(converted.operating_system, "Windows 11");
        assert_eq!(converted.release_id, "25H2");
        assert_eq!(converted.size_bytes, Some(5664921115));
        assert_eq!(converted.short_alias(), "win11-25h2");
    }
}
