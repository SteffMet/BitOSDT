# BitOSDT 2.0 - Database Schema and Data Models

## Overview

SQLite database for storing image configurations, deployment tasks, device history, and application settings.

## Database Schema

### Tables

```sql
-- Images: Windows deployment image definitions
CREATE TABLE images (
    id TEXT PRIMARY KEY,                    -- UUID
    name TEXT NOT NULL,
    description TEXT,
    
    -- OS Information
    os_type TEXT NOT NULL,                  -- 'Windows10', 'Windows11', 'Server2022'
    os_version TEXT NOT NULL,               -- '24H2', '23H2', etc.
    os_architecture TEXT NOT NULL,          -- 'x64', 'arm64'
    os_language TEXT DEFAULT 'en-US',       -- Language code
    
    -- License Information
    license_type TEXT NOT NULL,             -- 'Home', 'Pro', 'Enterprise'
    activation_type TEXT,                   -- 'Retail', 'Volume', 'OEM'
    
    -- Status
    status TEXT NOT NULL DEFAULT 'draft',   -- 'draft', 'ready', 'building', 'failed'
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    built_at DATETIME,
    
    -- File Paths
    workspace_path TEXT,                    -- Local workspace directory
    wim_path TEXT,                          -- Path to WIM file
    iso_path TEXT,                          -- Path to generated ISO
    
    -- Configuration (JSON)
    config_json TEXT NOT NULL,              -- Serialized DeployConfig
    
    -- Metadata
    size_bytes INTEGER,
    hash_sha256 TEXT
);

-- Image Groups: Logical grouping of images
CREATE TABLE groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT DEFAULT '#0078D4',          -- Hex color code
    icon TEXT,                              -- Icon identifier
    sort_order INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Image-Group relationship
CREATE TABLE image_groups (
    image_id TEXT REFERENCES images(id) ON DELETE CASCADE,
    group_id TEXT REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (image_id, group_id)
);

-- Tasks: Post-deployment tasks
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    image_id TEXT REFERENCES images(id) ON DELETE CASCADE,
    
    name TEXT NOT NULL,
    description TEXT,
    task_type TEXT NOT NULL,               -- 'install_app', 'run_script', 'copy_files', etc.
    
    -- Task Configuration (JSON)
    config_json TEXT NOT NULL,
    
    -- Execution
    sort_order INTEGER DEFAULT 0,
    enabled BOOLEAN DEFAULT 1,
    requires_reboot BOOLEAN DEFAULT 0,
    run_once BOOLEAN DEFAULT 1,
    
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Devices: Deployment target tracking
CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    
    -- Identification
    mac_address TEXT UNIQUE,
    serial_number TEXT,
    asset_tag TEXT,
    
    -- Hardware Info
    device_name TEXT,
    manufacturer TEXT,
    model TEXT,
    product TEXT,
    architecture TEXT,
    
    -- Specifications (JSON)
    hardware_json TEXT,
    
    -- Tracking
    first_seen DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_seen DATETIME,
    deployment_count INTEGER DEFAULT 0,
    
    -- Group
    device_group_id TEXT REFERENCES device_groups(id)
);

-- Device Groups
CREATE TABLE device_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT DEFAULT '#0078D4',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Deployments: Deployment history
CREATE TABLE deployments (
    id TEXT PRIMARY KEY,
    
    -- Relationships
    device_id TEXT REFERENCES devices(id),
    image_id TEXT REFERENCES images(id),
    
    -- Status
    status TEXT NOT NULL,                  -- 'pending', 'in_progress', 'completed', 'failed'
    
    -- Timing
    started_at DATETIME,
    completed_at DATETIME,
    duration_seconds INTEGER,
    
    -- Results
    error_message TEXT,
    log_path TEXT,
    
    -- Metadata (JSON)
    metadata_json TEXT                     -- Hardware info, driver versions, etc.
);

-- Driver Cache: Cached driver information
CREATE TABLE driver_cache (
    id TEXT PRIMARY KEY,
    
    -- Driver Info
    name TEXT NOT NULL,
    manufacturer TEXT,
    version TEXT,
    pnp_class TEXT,                        -- 'DiskDrive', 'Net', etc.
    
    -- Hardware IDs
    hardware_ids TEXT,                     -- Comma-separated HW IDs
    compatible_ids TEXT,                   -- Comma-separated compatible IDs
    
    -- File Info
    filename TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size INTEGER,
    hash_sha256 TEXT,
    
    -- Source
    source_type TEXT,                      -- 'cloud', 'driverpack', 'manual'
    source_url TEXT,
    
    -- Cache
    downloaded_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_used_at DATETIME,
    use_count INTEGER DEFAULT 0
);

-- Driver Packs Catalog: Cached driverpack metadata
CREATE TABLE driverpacks (
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
    last_updated DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Settings: Application configuration
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    value_type TEXT NOT NULL DEFAULT 'string',  -- 'string', 'integer', 'boolean', 'json'
    category TEXT DEFAULT 'general',
    description TEXT,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Audit Log: Change tracking
CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    action TEXT NOT NULL,                  -- 'create', 'update', 'delete'
    entity_type TEXT NOT NULL,             -- 'image', 'task', 'device', etc.
    entity_id TEXT,
    user_id TEXT,                          -- Future: multi-user support
    changes_json TEXT,                     -- JSON diff of changes
    ip_address TEXT
);
```

## Rust Data Models

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Images
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    
    pub os_info: OsInfo,
    pub license: LicenseInfo,
    pub status: ImageStatus,
    
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub built_at: Option<DateTime<Utc>>,
    
    pub workspace_path: Option<PathBuf>,
    pub wim_path: Option<PathBuf>,
    pub iso_path: Option<PathBuf>,
    
    pub config: DeployConfig,
    
    pub size_bytes: Option<u64>,
    pub hash_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub os_type: OsType,
    pub version: String,                    // "24H2"
    pub architecture: Architecture,
    pub language: String,                   // "en-US"
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum OsType {
    Windows10,
    Windows11,
    WindowsServer2022,
    WindowsServer2025,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Architecture {
    #[serde(rename = "x64")]
    X64,
    #[serde(rename = "arm64")]
    Arm64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub license_type: LicenseType,
    pub activation_type: Option<ActivationType>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LicenseType {
    Home,
    Pro,
    Enterprise,
    Education,
    Ltsc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivationType {
    Retail,
    Volume,
    Oem,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageStatus {
    Draft,
    Ready,
    Building,
    Failed,
}

// ============================================================================
// Tasks
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub image_id: Uuid,
    
    pub name: String,
    pub description: Option<String>,
    pub task_type: TaskType,
    pub config: TaskConfig,
    
    pub sort_order: i32,
    pub enabled: bool,
    pub requires_reboot: bool,
    pub run_once: bool,
    
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    InstallApplication,
    RunScript,
    CopyFiles,
    CreateUser,
    DomainJoin,
    RenameComputer,
    RegistryModify,
    InstallUpdates,
    Debloat,
    Autopilot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskConfig {
    InstallApplication {
        installer_path: String,
        installer_type: InstallerType,      // Msi, Exe, Msix
        arguments: Vec<String>,
        wait_for_exit: bool,
        exit_codes: Vec<i32>,              // Success exit codes
    },
    RunScript {
        script_content: String,
        script_type: ScriptType,            // PowerShell, Batch, VbScript
        execution_policy: String,
        run_as_admin: bool,
    },
    CopyFiles {
        source: String,
        destination: String,
        recursive: bool,
        overwrite: bool,
    },
    CreateUser {
        username: String,
        password: String,                   // Encrypted
        full_name: Option<String>,
        is_admin: bool,
    },
    DomainJoin {
        domain: String,
        username: String,
        password: String,                   // Encrypted
        ou_path: Option<String>,
    },
    RenameComputer {
        name_template: String,              // "COMPANY-%SERIAL%", "DESKTOP-%RAND%"
    },
    RegistryModify {
        key: String,
        value_name: String,
        value_data: String,
        value_type: RegistryValueType,
        operation: RegistryOperation,       // Add, Update, Delete
    },
    InstallUpdates {
        categories: Vec<String>,            // "Critical", "Security", etc.
        auto_reboot: bool,
    },
    Debloat {
        remove_apps: Vec<String>,           -- App names to remove
        remove_features: Vec<String>,       -- Windows features
    },
    Autopilot {
        profile_json: String,
        tenant_id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InstallerType {
    Msi,
    Exe,
    Msix,
    Appx,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ScriptType {
    PowerShell,
    Batch,
    VbScript,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RegistryValueType {
    String,
    Dword,
    Qword,
    Binary,
    MultiString,
    ExpandString,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RegistryOperation {
    Add,
    Update,
    Delete,
}

// ============================================================================
// Devices
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    
    pub mac_address: Option<String>,
    pub serial_number: Option<String>,
    pub asset_tag: Option<String>,
    
    pub device_name: Option<String>,
    pub manufacturer: String,
    pub model: String,
    pub product: String,
    pub architecture: Architecture,
    
    pub hardware: HardwareInfo,
    
    pub first_seen: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub deployment_count: i32,
    
    pub device_group_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub network_adapters: Vec<NetworkAdapterInfo>,
    pub form_factor: FormFactor,
    pub is_vm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub name: String,
    pub cores: u32,
    pub logical_processors: u32,
    pub max_speed_mhz: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub total_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub index: u32,
    pub model: String,
    pub size_bytes: u64,
    pub size_gb: f64,
    pub media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAdapterInfo {
    pub name: String,
    pub mac_address: String,
    pub adapter_type: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormFactor {
    Laptop,
    Desktop,
    Server,
    Tablet,
    SmallFormFactor,
    Unknown,
}

// ============================================================================
// Deployments
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: Uuid,
    pub device_id: Option<Uuid>,
    pub image_id: Uuid,
    
    pub status: DeploymentStatus,
    
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,
    
    pub error_message: Option<String>,
    pub log_path: Option<PathBuf>,
    
    pub metadata: DeploymentMetadata,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentMetadata {
    pub hardware_detected: Option<HardwareInfo>,
    pub drivers_installed: Vec<String>,
    pub os_version_deployed: String,
    pub task_results: Vec<TaskResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: Uuid,
    pub task_name: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub output: Option<String>,
    pub error: Option<String>,
}

// ============================================================================
// Groups
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGroup {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub created_at: DateTime<Utc>,
}
```

## Database Operations

```rust
use rusqlite::{Connection, Result, params};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }
    
    fn init_schema(&self,
    ) -> Result<()> {
        self.conn.execute_batch(include_str!("schema.sql"))?;
        Ok(())
    }
    
    // Images
    pub fn create_image(&self,
        image: &Image,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO images (id, name, description, os_type, os_version, 
             os_architecture, os_language, license_type, activation_type,
             status, config_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                image.id.to_string(),
                image.name,
                image.description,
                serde_json::to_string(&image.os_info.os_type)?,
                image.os_info.version,
                serde_json::to_string(&image.os_info.architecture)?,
                image.os_info.language,
                serde_json::to_string(&image.license.license_type)?,
                image.license.activation_type.map(|a| serde_json::to_string(&a).unwrap()),
                serde_json::to_string(&image.status)?,
                serde_json::to_string(&image.config)?,
                image.created_at,
                image.updated_at,
            ],
        )?;
        Ok(())
    }
    
    pub fn get_image(&self,
        id: Uuid,
    ) -> Result<Option<Image>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM images WHERE id = ?1"
        )?;
        
        let image = stmt.query_row([id.to_string()], |row| {
            Ok(Image {
                id: Uuid::parse_str(&row.get::<String, _>("id")?).unwrap(),
                name: row.get("name")?,
                description: row.get("description")?,
                os_info: OsInfo {
                    os_type: serde_json::from_str(&row.get::<String, _>("os_type")?).unwrap(),
                    version: row.get("os_version")?,
                    architecture: serde_json::from_str(&row.get::<String, _>("os_architecture")?).unwrap(),
                    language: row.get("os_language")?,
                },
                license: LicenseInfo {
                    license_type: serde_json::from_str(&row.get::<String, _>("license_type")?).unwrap(),
                    activation_type: row.get::<Option<String>, _>("activation_type")?
                        .map(|s| serde_json::from_str(&s).unwrap()),
                },
                status: serde_json::from_str(&row.get::<String, _>("status")?).unwrap(),
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
                built_at: row.get("built_at")?,
                workspace_path: row.get::<Option<String>, _>("workspace_path")?.map(|s| PathBuf::from(s)),
                wim_path: row.get::<Option<String>, _>("wim_path")?.map(|s| PathBuf::from(s)),
                iso_path: row.get::<Option<String>, _>("iso_path")?.map(|s| PathBuf::from(s)),
                config: serde_json::from_str(&row.get::<String, _>("config_json")?).unwrap(),
                size_bytes: row.get("size_bytes")?,
                hash_sha256: row.get("hash_sha256")?,
            })
        }).optional()?;
        
        Ok(image)
    }
    
    pub fn list_images(&self,
    ) -> Result<Vec<Image>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM images ORDER BY created_at DESC"
        )?;
        
        let images = stmt.query_map([], |row| {
            // ... same mapping as get_image
        })?.collect::<Result<Vec<_>>>()?;
        
        Ok(images)
    }
    
    pub fn update_image_status(
        &self,
        id: Uuid,
        status: ImageStatus,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&status)?,
                Utc::now(),
                id.to_string(),
            ],
        )?;
        Ok(())
    }
    
    // Tasks
    pub fn create_task(&self,
        task: &Task,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tasks (id, image_id, name, description, task_type,
             config_json, sort_order, enabled, requires_reboot, run_once, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.id.to_string(),
                task.image_id.to_string(),
                task.name,
                task.description,
                serde_json::to_string(&task.task_type)?,
                serde_json::to_string(&task.config)?,
                task.sort_order,
                task.enabled,
                task.requires_reboot,
                task.run_once,
                task.created_at,
            ],
        )?;
        Ok(())
    }
    
    pub fn get_image_tasks(&self,
        image_id: Uuid,
    ) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM tasks WHERE image_id = ?1 ORDER BY sort_order"
        )?;
        
        let tasks = stmt.query_map([image_id.to_string()], |row| {
            Ok(Task {
                id: Uuid::parse_str(&row.get::<String, _>("id")?).unwrap(),
                image_id: Uuid::parse_str(&row.get::<String, _>("image_id")?).unwrap(),
                name: row.get("name")?,
                description: row.get("description")?,
                task_type: serde_json::from_str(&row.get::<String, _>("task_type")?).unwrap(),
                config: serde_json::from_str(&row.get::<String, _>("config_json")?).unwrap(),
                sort_order: row.get("sort_order")?,
                enabled: row.get("enabled")?,
                requires_reboot: row.get("requires_reboot")?,
                run_once: row.get("run_once")?,
                created_at: row.get("created_at")?,
            })
        })?.collect::<Result<Vec<_>>>()?;
        
        Ok(tasks)
    }
    
    // Settings
    pub fn get_setting(&self,
        key: &str,
    ) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT value FROM settings WHERE key = ?1"
        )?;
        
        let value = stmt.query_row([key], |row| {
            row.get::<String, _>(0)
        }).optional()?;
        
        Ok(value)
    }
    
    pub fn set_setting(
        &self,
        key: &str,
        value: &str,
        value_type: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, value_type, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![key, value, value_type, Utc::now()],
        )?;
        Ok(())
    }
}
```

## Migrations

```rust
pub struct Migration;

impl Migration {
    pub fn run(conn: &Connection) -> Result<()> {
        let version: i32 = conn.query_row(
            "PRAGMA user_version",
            [],
            |row| row.get(0),
        )?;
        
        match version {
            0 => Self::migrate_v0_to_v1(conn)?,
            1 => Self::migrate_v1_to_v2(conn)?,
            // Add more migrations as needed
            _ => {}
        }
        
        Ok(())
    }
    
    fn migrate_v0_to_v1(conn: &Connection) -> Result<()> {
        // Initial schema creation
        conn.execute_batch(include_str!("schema_v1.sql"))?;
        conn.execute("PRAGMA user_version = 1", [])?;
        Ok(())
    }
    
    fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
        // Future migration example
        conn.execute(
            "ALTER TABLE images ADD COLUMN new_column TEXT",
            [],
        )?;
        conn.execute("PRAGMA user_version = 2", [])?;
        Ok(())
    }
}
```
