use crate::catalog::xml_parser::XmlParser;
use crate::core::database::Database;
use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::models::{CatalogSyncStatus, DriverPack};
use chrono::Utc;
use reqwest::Client;
use std::time::Duration;
use tracing::{info, warn};

pub struct CatalogSyncService {
    client: Client,
    db: Database,
}

impl CatalogSyncService {
    const OSDCLOUD_CATALOG_BASE: &'static str =
        "https://raw.githubusercontent.com/OSDeploy/OSD/master/Catalogs/DriverPack";

    const MANUFACTURERS: &'static [&'static str] = &["Dell", "HP", "Lenovo", "Microsoft"];

    pub fn new(db: Database) -> BitOSDTResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| BitOSDTError::Network(e.to_string()))?;

        Ok(Self { client, db })
    }

    pub async fn sync_all(&self) -> BitOSDTResult<Vec<CatalogSyncStatus>> {
        let mut results = Vec::new();

        for manufacturer in Self::MANUFACTURERS {
            let status = self.sync_manufacturer(manufacturer).await?;
            results.push(status);
        }

        Ok(results)
    }

    pub async fn sync_manufacturer(&self, manufacturer: &str) -> BitOSDTResult<CatalogSyncStatus> {
        let url = format!("{}/{}.xml", Self::OSDCLOUD_CATALOG_BASE, manufacturer);

        info!("Syncing {} catalog from {}", manufacturer, url);

        match self.fetch_and_parse(&url, manufacturer).await {
            Ok(driverpacks) => {
                let count = driverpacks.len() as u32;

                // Merge into database
                for dp in &driverpacks {
                    if let Err(e) = self.db.create_driverpack(dp) {
                        warn!("Failed to save driverpack {}: {}", dp.id, e);
                    }
                }

                info!("Synced {} entries for {}", count, manufacturer);

                Ok(CatalogSyncStatus {
                    manufacturer: manufacturer.to_string(),
                    last_sync: Some(Utc::now()),
                    last_sync_success: true,
                    entry_count: count,
                    source_url: url,
                    error_message: None,
                })
            }
            Err(e) => {
                warn!("Failed to sync {} catalog: {}", manufacturer, e);

                // Get existing count from DB for fallback
                let existing_count = self
                    .db
                    .get_driverpacks_by_manufacturer(manufacturer)
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);

                Ok(CatalogSyncStatus {
                    manufacturer: manufacturer.to_string(),
                    last_sync: Some(Utc::now()),
                    last_sync_success: false,
                    entry_count: existing_count,
                    source_url: url,
                    error_message: Some(e.to_string()),
                })
            }
        }
    }

    async fn fetch_and_parse(
        &self,
        url: &str,
        manufacturer: &str,
    ) -> BitOSDTResult<Vec<DriverPack>> {
        let response = self.client.get(url).send().await.map_err(|e| {
            BitOSDTError::Network(format!("Failed to fetch {}: {}", manufacturer, e))
        })?;

        if !response.status().is_success() {
            return Err(BitOSDTError::CatalogSync(format!(
                "HTTP {} for {}",
                response.status(),
                manufacturer
            )));
        }

        let xml_content = response.text().await.map_err(|e| {
            BitOSDTError::CatalogSync(format!(
                "Failed to read response for {}: {}",
                manufacturer, e
            ))
        })?;

        // Parse XML catalog
        let xml_packs = XmlParser::parse_driverpack_catalog(&xml_content)?;

        // Convert to our format
        let driverpacks: Vec<DriverPack> = xml_packs
            .into_iter()
            .map(|x| x.to_driverpack(manufacturer))
            .collect();

        Ok(driverpacks)
    }
}
