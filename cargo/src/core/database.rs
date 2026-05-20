use crate::core::errors::{BitOSDTResult, DatabaseError};
use crate::core::models::*;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: &Path) -> BitOSDTResult<Self> {
        let conn =
            Connection::open(path).map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        let db = Self { conn };
        db.init_schema()?;
        db.run_migrations()?;

        Ok(db)
    }

    fn init_schema(&self) -> BitOSDTResult<()> {
        self.conn.execute_batch(
            r#"
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
"#,
        )?;
        self.conn.execute_batch(SCHEMA_SQL)?;
        Ok(())
    }

    fn run_migrations(&self) -> BitOSDTResult<()> {
        let mut version: i32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if version == 0 {
            self.migrate_v0_to_v1()?;
            version = 1;
        }

        if version < 2 {
            self.migrate_v1_to_v2()?;
        }

        Ok(())
    }

    fn migrate_v0_to_v1(&self) -> BitOSDTResult<()> {
        self.conn.execute("PRAGMA user_version = 1", [])?;
        Ok(())
    }

    fn migrate_v1_to_v2(&self) -> BitOSDTResult<()> {
        if !self.column_exists("images", "wizard_state_json")? {
            self.conn
                .execute("ALTER TABLE images ADD COLUMN wizard_state_json TEXT", [])?;
        }
        self.conn.execute("PRAGMA user_version = 2", [])?;
        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> BitOSDTResult<bool> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({})", table))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for row in rows {
            if row?.eq_ignore_ascii_case(column) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn create_image(&self, image: &Image) -> BitOSDTResult<()> {
        let activation_type_json = image
            .license
            .activation_type
            .map(|a| serde_json::to_string(&a))
            .transpose()?;

        self.conn.execute(
            "INSERT INTO images (id, name, description, os_type, os_version, os_architecture, 
             os_language, license_type, activation_type, status, config_json, 
             created_at, updated_at, workspace_path, wim_path, iso_path, size_bytes, hash_sha256, wizard_state_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                image.id.to_string(),
                image.name,
                image.description,
                serde_json::to_string(&image.os_info.os_type)?,
                image.os_info.version,
                serde_json::to_string(&image.os_info.architecture)?,
                image.os_info.language,
                serde_json::to_string(&image.license.license_type)?,
                activation_type_json,
                serde_json::to_string(&image.status)?,
                serde_json::to_string(&image.config)?,
                image.created_at.to_rfc3339(),
                image.updated_at.to_rfc3339(),
                image.workspace_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                image.wim_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                image.iso_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                image.size_bytes.map(|s| s as i64),
                image.hash_sha256,
                image
                    .wizard_state_json
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
            ],
        )?;
        Ok(())
    }

    pub fn get_image(&self, id: Uuid) -> BitOSDTResult<Option<Image>> {
        let mut stmt = self.conn.prepare("SELECT * FROM images WHERE id = ?1")?;

        let image = stmt
            .query_row([id.to_string()], Self::row_to_image)
            .optional()?;

        Ok(image)
    }

    pub fn list_images(&self) -> BitOSDTResult<Vec<Image>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM images ORDER BY created_at DESC")?;

        let images = stmt
            .query_map([], Self::row_to_image)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(images)
    }

    pub fn update_image_status(&self, id: Uuid, status: ImageStatus) -> BitOSDTResult<()> {
        self.conn.execute(
            "UPDATE images SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&status)?,
                Utc::now().to_rfc3339(),
                id.to_string(),
            ],
        )?;
        Ok(())
    }

    pub fn update_image(&self, image: &Image) -> BitOSDTResult<bool> {
        let activation_type_json = image
            .license
            .activation_type
            .map(|a| serde_json::to_string(&a))
            .transpose()?;

        let affected = self.conn.execute(
            "UPDATE images SET
                name = ?1,
                description = ?2,
                os_type = ?3,
                os_version = ?4,
                os_architecture = ?5,
                os_language = ?6,
                license_type = ?7,
                activation_type = ?8,
                status = ?9,
                config_json = ?10,
                updated_at = ?11,
                workspace_path = ?12,
                wim_path = ?13,
                iso_path = ?14,
                size_bytes = ?15,
                hash_sha256 = ?16,
                wizard_state_json = ?17
             WHERE id = ?18",
            params![
                image.name,
                image.description,
                serde_json::to_string(&image.os_info.os_type)?,
                image.os_info.version,
                serde_json::to_string(&image.os_info.architecture)?,
                image.os_info.language,
                serde_json::to_string(&image.license.license_type)?,
                activation_type_json,
                serde_json::to_string(&image.status)?,
                serde_json::to_string(&image.config)?,
                image.updated_at.to_rfc3339(),
                image
                    .workspace_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                image
                    .wim_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                image
                    .iso_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                image.size_bytes.map(|s| s as i64),
                image.hash_sha256,
                image
                    .wizard_state_json
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                image.id.to_string(),
            ],
        )?;

        Ok(affected > 0)
    }

    pub fn delete_image(&self, id: Uuid) -> BitOSDTResult<bool> {
        let affected = self
            .conn
            .execute("DELETE FROM images WHERE id = ?1", [id.to_string()])?;
        Ok(affected > 0)
    }

    pub fn create_driverpack(&self, driverpack: &DriverPack) -> BitOSDTResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO driverpacks 
             (id, manufacturer, product, model, os, os_version, os_build, architecture,
              name, filename, url, hash_md5, hash_sha256, size_bytes, release_date,
              catalog_version, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                driverpack.id,
                driverpack.manufacturer,
                driverpack.product,
                driverpack.model,
                driverpack.os,
                driverpack.os_version,
                driverpack.os_build,
                serde_json::to_string(&driverpack.architecture)?,
                driverpack.name,
                driverpack.filename,
                driverpack.url,
                driverpack.hash_md5,
                driverpack.hash_sha256,
                driverpack.size_bytes.map(|s| s as i64),
                driverpack.release_date,
                driverpack.catalog_version,
                driverpack.last_synced.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_driverpacks_by_manufacturer(
        &self,
        manufacturer: &str,
    ) -> BitOSDTResult<Vec<DriverPack>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM driverpacks WHERE manufacturer = ?1 ORDER BY model")?;

        let packs = stmt
            .query_map([manufacturer], Self::row_to_driverpack)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(packs)
    }

    pub fn get_all_driverpacks(&self) -> BitOSDTResult<Vec<DriverPack>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM driverpacks ORDER BY manufacturer, model")?;

        let packs = stmt
            .query_map([], Self::row_to_driverpack)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(packs)
    }

    pub fn count_driverpacks(&self) -> BitOSDTResult<u32> {
        let count: i32 = self
            .conn
            .query_row("SELECT COUNT(*) FROM driverpacks", [], |row| row.get(0))?;
        Ok(count as u32)
    }

    pub fn clear_driverpacks(&self) -> BitOSDTResult<()> {
        self.conn.execute("DELETE FROM driverpacks", [])?;
        Ok(())
    }

    pub fn count_driver_cache_records(&self) -> BitOSDTResult<u32> {
        let count: i32 = self
            .conn
            .query_row("SELECT COUNT(*) FROM driver_cache", [], |row| row.get(0))?;
        Ok(count as u32)
    }

    pub fn clear_driver_cache_records(&self) -> BitOSDTResult<()> {
        self.conn.execute("DELETE FROM driver_cache", [])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> BitOSDTResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;

        let value = stmt
            .query_row([key], |row| row.get::<_, String>(0))
            .optional()?;

        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str, value_type: &str) -> BitOSDTResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, value_type, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![key, value, value_type, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    // OS Version catalog methods
    pub fn create_os_version(
        &self,
        os_version: &crate::catalog::OsVersionEntry,
    ) -> BitOSDTResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO os_versions 
             (id, display_name, operating_system, release_id, build, architecture,
              language_code, license, size_bytes, sha1, sha256, download_url, last_synced)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                os_version.id,
                os_version.display_name,
                os_version.operating_system,
                os_version.release_id,
                os_version.build,
                os_version.architecture,
                os_version.language_code,
                os_version.license,
                os_version.size_bytes.map(|s| s as i64),
                os_version.sha1,
                os_version.sha256,
                os_version.download_url,
                os_version.last_synced.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_os_versions(&self) -> BitOSDTResult<Vec<crate::catalog::OsVersionEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, operating_system, release_id, build, architecture,
                    language_code, license, size_bytes, sha1, sha256, download_url, last_synced
             FROM os_versions",
        )?;

        let mut versions = stmt
            .query_map([], Self::row_to_os_version)?
            .collect::<Result<Vec<_>, _>>()?;

        sort_os_versions(&mut versions);
        Ok(versions)
    }

    pub fn get_last_catalog_sync_time(&self) -> BitOSDTResult<Option<String>> {
        let result: Option<String> =
            self.conn
                .query_row("SELECT MAX(last_synced) FROM os_versions", [], |row| {
                    row.get(0)
                })?;
        Ok(result)
    }

    pub fn get_os_versions_filtered(
        &self,
        operating_system: Option<&str>,
        release_id: Option<&str>,
        architecture: Option<&str>,
        language_code: Option<&str>,
    ) -> BitOSDTResult<Vec<crate::catalog::OsVersionEntry>> {
        let mut sql = String::from(
            "SELECT id, display_name, operating_system, release_id, build, architecture,
                    language_code, license, size_bytes, sha1, sha256, download_url, last_synced
             FROM os_versions WHERE 1=1",
        );

        let mut params_vec: Vec<String> = Vec::new();

        if let Some(os) = operating_system {
            sql.push_str(&format!(
                " AND operating_system LIKE ?{}",
                params_vec.len() + 1
            ));
            params_vec.push(format!("%{}%", os));
        }
        if let Some(rel) = release_id {
            sql.push_str(&format!(" AND release_id = ?{}", params_vec.len() + 1));
            params_vec.push(rel.to_string());
        }
        if let Some(lang) = language_code {
            sql.push_str(&format!(" AND language_code = ?{}", params_vec.len() + 1));
            params_vec.push(lang.to_string());
        }
        if let Some(arch) = architecture {
            sql.push_str(&format!(" AND architecture = ?{}", params_vec.len() + 1));
            params_vec.push(arch.to_string());
        }

        let mut stmt = self.conn.prepare(&sql)?;

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();

        let mut versions = stmt
            .query_map(params_refs.as_slice(), Self::row_to_os_version)?
            .collect::<Result<Vec<_>, _>>()?;

        sort_os_versions(&mut versions);
        Ok(versions)
    }

    pub fn count_os_versions(&self) -> BitOSDTResult<u32> {
        let count: i32 = self
            .conn
            .query_row("SELECT COUNT(*) FROM os_versions", [], |row| row.get(0))?;
        Ok(count as u32)
    }

    pub fn clear_os_versions(&self) -> BitOSDTResult<()> {
        self.conn.execute("DELETE FROM os_versions", [])?;
        Ok(())
    }

    // --- Row-mapping helpers ---

    /// Map a database row to an Image struct.
    fn row_to_image(row: &rusqlite::Row<'_>) -> rusqlite::Result<Image> {
        Ok(Image {
            id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_else(|_| Uuid::new_v4()),
            name: row.get(1)?,
            description: row.get(2)?,
            os_info: OsInfo {
                os_type: serde_json::from_str(&row.get::<_, String>(3)?)
                    .unwrap_or(OsType::Windows11),
                version: row.get(4)?,
                architecture: serde_json::from_str(&row.get::<_, String>(5)?)
                    .unwrap_or(Architecture::X64),
                language: row.get(6)?,
            },
            license: LicenseInfo {
                license_type: serde_json::from_str(&row.get::<_, String>(7)?)
                    .unwrap_or(LicenseType::Pro),
                activation_type: row
                    .get::<_, Option<String>>(8)?
                    .and_then(|s| serde_json::from_str(&s).ok()),
            },
            status: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or(ImageStatus::Draft),
            config: serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or_default(),
            created_at: row
                .get::<_, String>(10)?
                .parse()
                .unwrap_or_else(|_| Utc::now()),
            updated_at: row
                .get::<_, String>(11)?
                .parse()
                .unwrap_or_else(|_| Utc::now()),
            workspace_path: row.get::<_, Option<String>>(12)?.map(|s| s.into()),
            wim_path: row.get::<_, Option<String>>(13)?.map(|s| s.into()),
            iso_path: row.get::<_, Option<String>>(14)?.map(|s| s.into()),
            size_bytes: row.get::<_, Option<i64>>(16)?.map(|s| s as u64),
            hash_sha256: row.get(17)?,
            wizard_state_json: row
                .get::<_, Option<String>>(18)?
                .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok()),
            built_at: None,
        })
    }

    /// Map a database row to a DriverPack struct.
    fn row_to_driverpack(row: &rusqlite::Row<'_>) -> rusqlite::Result<DriverPack> {
        Ok(DriverPack {
            id: row.get(0)?,
            manufacturer: row.get(1)?,
            product: row.get(2)?,
            model: row.get(3)?,
            os: row.get(4)?,
            os_version: row.get(5)?,
            os_build: row.get(6)?,
            architecture: serde_json::from_str(&row.get::<_, String>(7)?)
                .unwrap_or(Architecture::X64),
            name: row.get(8)?,
            filename: row.get(9)?,
            url: row.get(10)?,
            hash_md5: row.get(11)?,
            hash_sha256: row.get(12)?,
            size_bytes: row.get::<_, Option<i64>>(13)?.map(|s| s as u64),
            release_date: row.get(14)?,
            catalog_version: row.get(15)?,
            last_synced: row
                .get::<_, String>(16)?
                .parse()
                .unwrap_or_else(|_| Utc::now()),
        })
    }

    /// Map a database row to an OsVersionEntry struct.
    fn row_to_os_version(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<crate::catalog::OsVersionEntry> {
        Ok(crate::catalog::OsVersionEntry {
            id: row.get(0)?,
            display_name: row.get(1)?,
            operating_system: row.get(2)?,
            release_id: row.get(3)?,
            build: row.get(4)?,
            architecture: row.get(5)?,
            language_code: row.get(6)?,
            license: row.get(7)?,
            size_bytes: row.get::<_, Option<i64>>(8)?.map(|s| s as u64),
            sha1: row.get(9)?,
            sha256: row.get(10)?,
            download_url: row.get(11)?,
            last_synced: row
                .get::<_, String>(12)?
                .parse()
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

fn release_sort_key(release_id: &str) -> (i32, i32, String) {
    let normalized = release_id.trim().to_ascii_uppercase();
    if let Some((year, half)) = normalized.split_once('H') {
        if let (Ok(year), Ok(half)) = (year.parse::<i32>(), half.parse::<i32>()) {
            return (year, half, normalized);
        }
    }

    (i32::MIN, i32::MIN, normalized)
}

fn build_sort_key(build: &str) -> u64 {
    build
        .chars()
        .filter(|value| value.is_ascii_digit())
        .collect::<String>()
        .parse::<u64>()
        .unwrap_or_default()
}

fn sort_os_versions(versions: &mut [crate::catalog::OsVersionEntry]) {
    versions.sort_by(|left, right| {
        release_sort_key(&right.release_id)
            .cmp(&release_sort_key(&left.release_id))
            .then_with(|| build_sort_key(&right.build).cmp(&build_sort_key(&left.build)))
            .then_with(|| left.operating_system.cmp(&right.operating_system))
            .then_with(|| left.language_code.cmp(&right.language_code))
            .then_with(|| left.architecture.cmp(&right.architecture))
            .then_with(|| left.license.cmp(&right.license))
    });
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS images (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    os_type TEXT NOT NULL,
    os_version TEXT NOT NULL,
    os_architecture TEXT NOT NULL,
    os_language TEXT DEFAULT 'en-US',
    license_type TEXT NOT NULL,
    activation_type TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    workspace_path TEXT,
    wim_path TEXT,
    iso_path TEXT,
    config_json TEXT NOT NULL,
    size_bytes INTEGER,
    hash_sha256 TEXT,
    wizard_state_json TEXT
);

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT DEFAULT '#0078D4',
    icon TEXT,
    sort_order INTEGER DEFAULT 0,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS image_groups (
    image_id TEXT REFERENCES images(id) ON DELETE CASCADE,
    group_id TEXT REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (image_id, group_id)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    image_id TEXT REFERENCES images(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    task_type TEXT NOT NULL,
    config_json TEXT NOT NULL,
    sort_order INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1,
    requires_reboot INTEGER DEFAULT 0,
    run_once INTEGER DEFAULT 1,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    mac_address TEXT UNIQUE,
    serial_number TEXT,
    asset_tag TEXT,
    device_name TEXT,
    manufacturer TEXT,
    model TEXT,
    product TEXT,
    architecture TEXT,
    hardware_json TEXT,
    first_seen TEXT DEFAULT CURRENT_TIMESTAMP,
    last_seen TEXT,
    deployment_count INTEGER DEFAULT 0,
    device_group_id TEXT
);

CREATE TABLE IF NOT EXISTS device_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT DEFAULT '#0078D4',
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS deployments (
    id TEXT PRIMARY KEY,
    device_id TEXT REFERENCES devices(id),
    image_id TEXT REFERENCES images(id),
    status TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    duration_seconds INTEGER,
    error_message TEXT,
    log_path TEXT,
    metadata_json TEXT
);

CREATE TABLE IF NOT EXISTS driver_cache (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    manufacturer TEXT,
    version TEXT,
    pnp_class TEXT,
    hardware_ids TEXT,
    compatible_ids TEXT,
    filename TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size INTEGER,
    hash_sha256 TEXT,
    source_type TEXT,
    source_url TEXT,
    downloaded_at TEXT DEFAULT CURRENT_TIMESTAMP,
    last_used_at TEXT,
    use_count INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS driverpacks (
    id TEXT PRIMARY KEY,
    manufacturer TEXT NOT NULL,
    product TEXT NOT NULL,
    model TEXT,
    os TEXT NOT NULL,
    os_version TEXT NOT NULL,
    os_build TEXT,
    architecture TEXT NOT NULL,
    name TEXT NOT NULL,
    filename TEXT NOT NULL,
    url TEXT NOT NULL,
    hash_md5 TEXT,
    hash_sha256 TEXT,
    size_bytes INTEGER,
    release_date TEXT,
    catalog_version TEXT,
    last_updated TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_driverpacks_manufacturer ON driverpacks(manufacturer);

CREATE TABLE IF NOT EXISTS os_versions (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    operating_system TEXT NOT NULL,
    release_id TEXT NOT NULL,
    build TEXT NOT NULL,
    architecture TEXT NOT NULL,
    language_code TEXT NOT NULL,
    license TEXT NOT NULL,
    size_bytes INTEGER,
    sha1 TEXT,
    sha256 TEXT,
    download_url TEXT NOT NULL,
    last_synced TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_os_versions_release ON os_versions(release_id, language_code, architecture);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    value_type TEXT NOT NULL DEFAULT 'string',
    category TEXT DEFAULT 'general',
    description TEXT,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT DEFAULT CURRENT_TIMESTAMP,
    action TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    user_id TEXT,
    changes_json TEXT,
    ip_address TEXT
);

PRAGMA foreign_keys = ON;
"#;

impl Default for DeployConfig {
    fn default() -> Self {
        Self {
            target_disk: None,
            uefi: true,
            interactive: true,
            cleanup: true,
            wim_path: None,
            os_version: "24H2".to_string(),
            driver_prefs: DriverPreferences::default(),
            runtime_driver_context: None,
            unattend: None,
            tasks: None,
            autopilot: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;
    use tempfile::tempdir;

    fn sample_os_version_entry(release_id: &str, build: &str) -> crate::catalog::OsVersionEntry {
        crate::catalog::OsVersionEntry {
            id: format!(
                "windows-11-{}-amd64-en-us-retail",
                release_id.to_lowercase()
            ),
            display_name: format!("Win11-{}-amd64", release_id),
            operating_system: "Windows 11".to_string(),
            release_id: release_id.to_string(),
            build: build.to_string(),
            architecture: "amd64".to_string(),
            language_code: "en-us".to_string(),
            license: "Retail".to_string(),
            size_bytes: None,
            sha1: None,
            sha256: None,
            download_url: format!("https://example.invalid/{}.esd", release_id.to_lowercase()),
            last_synced: Utc::now(),
        }
    }

    fn sample_image() -> Image {
        let now = Utc::now();
        Image {
            id: Uuid::new_v4(),
            name: "Test Image".to_string(),
            description: Some("Image for DB tests".to_string()),
            os_info: OsInfo {
                os_type: OsType::Windows11,
                version: "24H2".to_string(),
                architecture: Architecture::X64,
                language: "en-US".to_string(),
            },
            license: LicenseInfo {
                license_type: LicenseType::Pro,
                activation_type: None,
            },
            status: ImageStatus::Ready,
            created_at: now,
            updated_at: now,
            built_at: None,
            workspace_path: None,
            wim_path: None,
            iso_path: None,
            config: DeployConfig::default(),
            size_bytes: Some(123),
            hash_sha256: Some("abc123".to_string()),
            wizard_state_json: Some(json!({
                "windowsVersion": { "name": "Windows 11", "build": "24H2", "edition": "Pro" },
                "output": { "outputType": "FullISO", "outputPath": "C:\\\\tmp\\\\test.iso" }
            })),
        }
    }

    #[test]
    fn migration_v2_adds_wizard_state_and_preserves_legacy_rows() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("legacy.db");

        {
            let conn = Connection::open(&db_path).expect("open sqlite db");
            conn.execute_batch(
                r#"
CREATE TABLE images (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    os_type TEXT NOT NULL,
    os_version TEXT NOT NULL,
    os_architecture TEXT NOT NULL,
    os_language TEXT DEFAULT 'en-US',
    license_type TEXT NOT NULL,
    activation_type TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    workspace_path TEXT,
    wim_path TEXT,
    iso_path TEXT,
    config_json TEXT NOT NULL,
    size_bytes INTEGER,
    hash_sha256 TEXT
);
PRAGMA user_version = 1;
"#,
            )
            .expect("create legacy schema");

            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO images (id, name, description, os_type, os_version, os_architecture, os_language, license_type, activation_type, status, created_at, updated_at, workspace_path, wim_path, iso_path, config_json, size_bytes, hash_sha256)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    Uuid::new_v4().to_string(),
                    "Legacy Image",
                    Option::<String>::None,
                    "\"Windows11\"",
                    "23H2",
                    "\"x64\"",
                    "en-US",
                    "\"Pro\"",
                    Option::<String>::None,
                    "\"Draft\"",
                    now,
                    Utc::now().to_rfc3339(),
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    "{}",
                    Option::<i64>::None,
                    Option::<String>::None,
                ],
            )
            .expect("insert legacy image");
        }

        let db = Database::new(&db_path).expect("open database with migrations");
        let conn = Connection::open(&db_path).expect("re-open sqlite db");

        let user_version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(user_version, 2);

        let has_wizard_state = db
            .column_exists("images", "wizard_state_json")
            .expect("wizard_state_json column exists");
        assert!(has_wizard_state);

        let images = db.list_images().expect("list images");
        assert_eq!(images.len(), 1);
        assert!(images[0].wizard_state_json.is_none());
    }

    #[test]
    fn image_wizard_state_round_trips() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("roundtrip.db");
        let db = Database::new(&db_path).expect("open database");

        let image = sample_image();
        db.create_image(&image).expect("create image");

        let fetched = db
            .get_image(image.id)
            .expect("load image")
            .expect("image exists");
        assert_eq!(fetched.wizard_state_json, image.wizard_state_json);
    }

    #[test]
    fn update_image_returns_true_for_existing_and_false_for_missing() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("update.db");
        let db = Database::new(&db_path).expect("open database");

        let mut image = sample_image();
        db.create_image(&image).expect("create image");

        image.name = "Updated Name".to_string();
        image.updated_at = Utc::now();
        assert!(db.update_image(&image).expect("update existing image"));

        let mut missing = image.clone();
        missing.id = Uuid::new_v4();
        missing.updated_at = Utc::now();
        assert!(!db
            .update_image(&missing)
            .expect("missing image should return false"));
    }

    #[test]
    fn sorts_os_versions_by_semantic_release_then_build_desc() {
        let mut versions = vec![
            sample_os_version_entry("24H2", "26100.1150"),
            sample_os_version_entry("25H2", "26200.1000"),
            sample_os_version_entry("26H1", "26300.10"),
            sample_os_version_entry("25H2", "26200.9000"),
        ];

        sort_os_versions(&mut versions);

        let ordered: Vec<(&str, &str)> = versions
            .iter()
            .map(|entry| (entry.release_id.as_str(), entry.build.as_str()))
            .collect();

        assert_eq!(
            ordered,
            vec![
                ("26H1", "26300.10"),
                ("25H2", "26200.9000"),
                ("25H2", "26200.1000"),
                ("24H2", "26100.1150"),
            ]
        );
    }

    #[test]
    fn get_last_catalog_sync_time_returns_max_last_synced_value() {
        let temp = tempdir().expect("temp dir");
        let db_path = temp.path().join("catalog-sync.db");
        let db = Database::new(&db_path).expect("open database");

        let mut older = sample_os_version_entry("24H2", "26100.1150");
        older.last_synced = "2026-04-01T10:00:00+00:00"
            .parse()
            .expect("older sync time");
        db.create_os_version(&older).expect("insert older entry");

        let mut newer = sample_os_version_entry("25H2", "26200.9000");
        newer.last_synced = "2026-04-08T12:00:00+00:00"
            .parse()
            .expect("newer sync time");
        db.create_os_version(&newer).expect("insert newer entry");

        let last_synced = db
            .get_last_catalog_sync_time()
            .expect("last sync query")
            .expect("sync time should exist");
        assert_eq!(last_synced, "2026-04-08T12:00:00+00:00");
    }
}
