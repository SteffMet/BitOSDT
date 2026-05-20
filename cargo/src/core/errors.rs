use thiserror::Error;

#[derive(Error, Debug)]
pub enum BitOSDTError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Hardware detection failed: {0}")]
    HardwareDetection(String),

    #[error("Driver operation failed: {0}")]
    Driver(String),

    #[error("Deployment failed: {0}")]
    Deployment(String),

    #[error("WinPE build failed: {0}")]
    WinPE(String),

    #[error("Catalog sync failed: {0}")]
    CatalogSync(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Operation cancelled by user")]
    Cancelled,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Download error: {0}")]
    Download(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<rusqlite::Error> for BitOSDTError {
    fn from(err: rusqlite::Error) -> Self {
        BitOSDTError::Database(DatabaseError::QueryFailed(err.to_string()))
    }
}

#[cfg(target_os = "windows")]
impl From<windows::core::Error> for BitOSDTError {
    fn from(err: windows::core::Error) -> Self {
        BitOSDTError::HardwareDetection(err.to_string())
    }
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    #[error("Record not found: {0}")]
    RecordNotFound(String),

    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to load config: {0}")]
    LoadFailed(String),

    #[error("Failed to save config: {0}")]
    SaveFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

pub type BitOSDTResult<T> = Result<T, BitOSDTError>;
