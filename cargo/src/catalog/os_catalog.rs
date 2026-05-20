use crate::core::models::OsType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsVersion {
    pub id: String,
    pub os_type: OsType,
    pub version: String,
    pub build_number: String,
    pub release_date: DateTime<Utc>,
    pub end_of_service: Option<DateTime<Utc>>,
    pub esd_url: String,
    pub size_bytes: u64,
    pub languages: Vec<String>,
}

impl OsVersion {
    pub fn display_name(&self) -> String {
        format!(
            "{} {} (Build {})",
            self.os_type.display_name(),
            self.version,
            self.build_number
        )
    }
}

pub trait OsTypeExt {
    fn display_name(&self) -> &'static str;
    fn supported_versions() -> Vec<&'static str>;
}

impl OsTypeExt for OsType {
    fn display_name(&self) -> &'static str {
        match self {
            OsType::Windows10 => "Windows 10",
            OsType::Windows11 => "Windows 11",
            OsType::WindowsServer2022 => "Windows Server 2022",
            OsType::WindowsServer2025 => "Windows Server 2025",
            OsType::Other => "Other",
        }
    }

    fn supported_versions() -> Vec<&'static str> {
        vec![
            "22H2", // Windows 10
            "21H2", // Windows 11
            "22H2", // Windows 11
            "23H2", // Windows 11
            "24H2", // Windows 11
            "25H2", // Windows 11 (upcoming)
            "21H2", // Server 2022
        ]
    }
}

// Built-in catalog for common Windows versions (fallback when DB is empty)
// These represent English x64 versions - use sync for full multi-language catalog
pub fn get_builtin_os_catalog() -> Vec<OsVersion> {
    vec![
        // Windows 11 25H2 (Upcoming)
        OsVersion {
            id: "win11-25h2-x64-en-us".to_string(),
            os_type: OsType::Windows11,
            version: "25H2".to_string(),
            build_number: "26200".to_string(),
            release_date: Utc::now(),
            end_of_service: None,
            esd_url: "https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/placeholder".to_string(),
            size_bytes: 5_500_000_000,
            languages: vec!["en-US".to_string()],
        },
        // Windows 11 24H2
        OsVersion {
            id: "win11-24h2-x64-en-us".to_string(),
            os_type: OsType::Windows11,
            version: "24H2".to_string(),
            build_number: "26100".to_string(),
            release_date: Utc::now(),
            end_of_service: None,
            esd_url: "https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/placeholder".to_string(),
            size_bytes: 5_446_049_792,
            languages: vec!["en-US".to_string()],
        },
        // Windows 11 23H2
        OsVersion {
            id: "win11-23h2-x64-en-us".to_string(),
            os_type: OsType::Windows11,
            version: "23H2".to_string(),
            build_number: "22631".to_string(),
            release_date: Utc::now(),
            end_of_service: None,
            esd_url: "https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/placeholder".to_string(),
            size_bytes: 5_368_709_120,
            languages: vec!["en-US".to_string()],
        },
        // Windows 11 22H2
        OsVersion {
            id: "win11-22h2-x64-en-us".to_string(),
            os_type: OsType::Windows11,
            version: "22H2".to_string(),
            build_number: "22621".to_string(),
            release_date: Utc::now(),
            end_of_service: None,
            esd_url: "https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/placeholder".to_string(),
            size_bytes: 5_200_000_000,
            languages: vec!["en-US".to_string()],
        },
        // Windows 11 21H2
        OsVersion {
            id: "win11-21h2-x64-en-us".to_string(),
            os_type: OsType::Windows11,
            version: "21H2".to_string(),
            build_number: "22000".to_string(),
            release_date: Utc::now(),
            end_of_service: None,
            esd_url: "https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/placeholder".to_string(),
            size_bytes: 4_800_000_000,
            languages: vec!["en-US".to_string()],
        },
        // Windows 10 22H2
        OsVersion {
            id: "win10-22h2-x64-en-us".to_string(),
            os_type: OsType::Windows10,
            version: "22H2".to_string(),
            build_number: "19045".to_string(),
            release_date: Utc::now(),
            end_of_service: None,
            esd_url: "https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/placeholder".to_string(),
            size_bytes: 4_700_000_000,
            languages: vec!["en-US".to_string()],
        },
        // Windows 10 21H2
        OsVersion {
            id: "win10-21h2-x64-en-us".to_string(),
            os_type: OsType::Windows10,
            version: "21H2".to_string(),
            build_number: "19044".to_string(),
            release_date: Utc::now(),
            end_of_service: None,
            esd_url: "https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/placeholder".to_string(),
            size_bytes: 4_600_000_000,
            languages: vec!["en-US".to_string()],
        },
    ]
}
