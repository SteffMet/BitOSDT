use crate::core::errors::{BitOSDTResult, ConfigError};
use crate::core::models::Settings;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub settings: Settings,
    pub database_path: PathBuf,
}

impl Config {
    pub fn load() -> BitOSDTResult<Self> {
        let _ = Self::ensure_app_dir_exists()?;
        let config_path = Self::config_path()?;

        if config_path.exists() {
            return Self::load_from_path(&config_path);
        }

        if let Some(legacy_config_path) = Self::legacy_config_path() {
            if legacy_config_path.exists() {
                return Self::load_from_path(&legacy_config_path);
            }
        }

        let config = Self::default();
        config.save()?;
        Ok(config)
    }

    pub fn save(&self) -> BitOSDTResult<()> {
        let config_path = Self::config_path()?;

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, content).map_err(|e| ConfigError::SaveFailed(e.to_string()))?;

        Ok(())
    }

    pub fn ensure_app_dir_exists() -> BitOSDTResult<PathBuf> {
        let app_dir = Self::app_dir()?;
        fs::create_dir_all(&app_dir)?;
        Ok(app_dir)
    }

    fn config_path() -> BitOSDTResult<PathBuf> {
        let app_dir = Self::ensure_app_dir_exists()?;
        Ok(app_dir.join("config.json"))
    }

    fn load_from_path(path: &Path) -> BitOSDTResult<Self> {
        let content =
            fs::read_to_string(path).map_err(|e| ConfigError::LoadFailed(e.to_string()))?;

        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    fn app_dir() -> BitOSDTResult<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            return Ok(Self::windows_app_dir());
        }

        #[cfg(not(target_os = "windows"))]
        {
            let home = dirs::home_dir()
                .ok_or_else(|| ConfigError::LoadFailed("Home directory not found".to_string()))?;
            Ok(home.join(".bitosdt"))
        }
    }

    pub fn database_path() -> BitOSDTResult<PathBuf> {
        let app_dir = Self::ensure_app_dir_exists()?;
        Ok(app_dir.join("bitosdt.db"))
    }

    pub fn default_download_path() -> BitOSDTResult<PathBuf> {
        let app_dir = Self::ensure_app_dir_exists()?;
        Ok(app_dir.join("Downloads"))
    }

    pub fn default_workspace_path() -> BitOSDTResult<PathBuf> {
        let app_dir = Self::ensure_app_dir_exists()?;
        Ok(app_dir.join("Workspace"))
    }

    pub fn configured_download_path() -> BitOSDTResult<PathBuf> {
        Self::load()
            .map(|config| config.settings.download_path)
            .or_else(|_| Self::default_download_path())
    }

    pub fn configured_workspace_path() -> BitOSDTResult<PathBuf> {
        Self::load()
            .map(|config| config.settings.workspace_path)
            .or_else(|_| Self::default_workspace_path())
    }

    #[cfg(target_os = "windows")]
    fn windows_app_dir() -> PathBuf {
        PathBuf::from(r"C:\BitOSDT")
    }

    fn legacy_config_path() -> Option<PathBuf> {
        Self::legacy_app_dir().map(|path| path.join("config.json"))
    }

    fn legacy_app_dir() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            return dirs::home_dir().map(|home| home.join(".bitosdt"));
        }

        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }

    fn fallback_app_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            return Self::windows_app_dir();
        }

        #[cfg(not(target_os = "windows"))]
        {
            dirs::home_dir()
                .map(|h| h.join(".bitosdt"))
                .unwrap_or_else(|| PathBuf::from(".bitosdt"))
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let app_dir = Self::fallback_app_dir();

        Self {
            database_path: app_dir.join("bitosdt.db"),
            settings: Settings {
                default_language: "en-US".to_string(),
                theme: "system".to_string(),
                auto_check_updates: true,
                download_path: app_dir.join("Downloads"),
                workspace_path: app_dir.join("Workspace"),
                adk_path: None,
                suppress_credential_warning: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use std::path::PathBuf;

    #[cfg(target_os = "windows")]
    #[test]
    fn default_config_uses_c_drive_root_on_windows() {
        let config = Config::default();
        assert_eq!(
            config.database_path,
            PathBuf::from(r"C:\BitOSDT\bitosdt.db")
        );
        assert_eq!(
            config.settings.download_path,
            PathBuf::from(r"C:\BitOSDT\Downloads")
        );
        assert_eq!(
            config.settings.workspace_path,
            PathBuf::from(r"C:\BitOSDT\Workspace")
        );
    }
}
