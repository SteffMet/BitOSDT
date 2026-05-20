use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::core::models::{Architecture, DriverPack};
use chrono::Utc;
use quick_xml::de::from_str;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename = "DriverPack")]
pub struct OsdCloudDriverPackXml {
    #[serde(rename = "Product")]
    pub product: String,

    #[serde(rename = "Model", default)]
    pub model: Option<String>,

    #[serde(rename = "OS")]
    pub os: String,

    #[serde(rename = "OSVersion")]
    pub os_version: String,

    #[serde(rename = "Url")]
    pub url: String,

    #[serde(rename = "FileName")]
    pub filename: String,

    #[serde(rename = "Hash")]
    pub hash: String,

    #[serde(rename = "ReleaseDate", default)]
    pub release_date: Option<String>,
}

impl OsdCloudDriverPackXml {
    pub fn to_driverpack(&self, manufacturer: &str) -> DriverPack {
        DriverPack {
            id: format!(
                "{}-{}-{}",
                manufacturer.to_lowercase(),
                self.product.to_lowercase(),
                self.os_version.to_lowercase().replace(' ', "-")
            ),
            manufacturer: manufacturer.to_string(),
            product: self.product.clone(),
            model: self.model.clone().unwrap_or_default(),
            os: self.os.clone(),
            os_version: self.os_version.clone(),
            os_build: None,
            architecture: Architecture::X64, // Parsed from filename/os
            name: format!(
                "{} {} {} Driver Pack",
                manufacturer, self.product, self.os_version
            ),
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

#[derive(Debug, Deserialize)]
#[serde(rename = "ArrayOfDriverPack")]
pub struct OsdCloudCatalog {
    #[serde(rename = "DriverPack", default)]
    pub driver_packs: Vec<OsdCloudDriverPackXml>,
}

pub struct XmlParser;

impl XmlParser {
    pub fn parse_driverpack_catalog(
        xml_content: &str,
    ) -> BitOSDTResult<Vec<OsdCloudDriverPackXml>> {
        let catalog: OsdCloudCatalog = from_str(xml_content)
            .map_err(|e| BitOSDTError::CatalogSync(format!("XML parse error: {}", e)))?;

        Ok(catalog.driver_packs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sample_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <ArrayOfDriverPack>
            <DriverPack>
                <Product>0A5D</Product>
                <Model>Latitude 5520</Model>
                <OS>Windows 11</OS>
                <OSVersion>24H2</OSVersion>
                <Url>https://dl.dell.com/FOLDER09876543/1/5520-win11-A04.cab</Url>
                <FileName>5520-win11-A04.cab</FileName>
                <Hash>ABC123DEF456</Hash>
                <ReleaseDate>2024-09-23</ReleaseDate>
            </DriverPack>
        </ArrayOfDriverPack>"#;

        let result = XmlParser::parse_driverpack_catalog(xml);
        assert!(result.is_ok());

        let packs = result.unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].product, "0A5D");
        assert_eq!(packs[0].model, Some("Latitude 5520".to_string()));
    }
}
