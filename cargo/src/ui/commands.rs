#[cfg(feature = "gui")]
use serde::{Deserialize, Serialize};

// ============================================================================
// System Info Commands
// ============================================================================

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub version: String,
    pub platform: String,
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    SystemInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

// ============================================================================
// Download Commands
// ============================================================================

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub url: String,
    pub output_path: String,
    pub expected_hash: Option<String>,
}

#[cfg(feature = "gui")]
#[tauri::command]
pub async fn start_esd_download(request: DownloadRequest) -> Result<String, String> {
    Ok(format!(
        "Download started: {} -> {}",
        request.url, request.output_path
    ))
}

// ============================================================================
// Config Generation Commands
// ============================================================================

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_unattend_xml(config_json: String, output_path: String) -> Result<String, String> {
    use crate::config::{UnattendConfig, UnattendGenerator};

    let config: UnattendConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let xml =
        UnattendGenerator::generate(&config).map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &xml).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(xml)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_autopilot_json(config_json: String, output_path: String) -> Result<String, String> {
    use crate::config::{AutopilotGenerator, AutopilotProfile};

    let profile: AutopilotProfile =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let json = AutopilotGenerator::generate_configuration(&profile)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &json).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(json)
}

// ============================================================================
// App Installation Script Commands
// ============================================================================

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_app_install_script(
    config_json: String,
    output_path: String,
) -> Result<String, String> {
    use crate::tasks::{AppInstallConfig, AppInstaller};

    let config: AppInstallConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = AppInstaller::generate_install_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_winget_script(
    packages_json: String,
    output_path: String,
) -> Result<String, String> {
    use crate::tasks::{AppInstaller, WingetPackage};

    let packages: Vec<WingetPackage> =
        serde_json::from_str(&packages_json).map_err(|e| format!("Invalid packages: {}", e))?;

    let script = AppInstaller::generate_winget_only_script(&packages);

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

// ============================================================================
// Task Generation Commands
// ============================================================================

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_windows_update_script(
    config_json: String,
    output_path: String,
) -> Result<String, String> {
    use crate::tasks::{WindowsUpdateConfig, WindowsUpdateGenerator};

    let config: WindowsUpdateConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = WindowsUpdateGenerator::generate_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_domain_join_script(
    config_json: String,
    output_path: String,
) -> Result<String, String> {
    use crate::tasks::{DomainJoinConfig, DomainJoinGenerator};

    let config: DomainJoinConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = DomainJoinGenerator::generate_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_user_creation_script(
    config_json: String,
    output_path: String,
) -> Result<String, String> {
    use crate::tasks::{UserCreatorGenerator, UsersConfig};

    let config: UsersConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config: {}", e))?;

    let script = UserCreatorGenerator::generate_script(&config)
        .map_err(|e| format!("Generation failed: {}", e))?;

    std::fs::write(&output_path, &script).map_err(|e| format!("Failed to write script: {}", e))?;

    Ok(script)
}

// ============================================================================
// Startnet Commands
// ============================================================================

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_osdcloud_startnet(
    output_path: String,
    use_start_osdcloud: bool,
) -> Result<String, String> {
    use crate::build::StartnetGenerator;

    let content = StartnetGenerator::generate_osdcloud(use_start_osdcloud);

    std::fs::write(&output_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(content)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_bitosdt_startnet(output_path: String, exe_path: String) -> Result<String, String> {
    use crate::build::StartnetGenerator;

    let content = StartnetGenerator::generate_bitosdt_gui(&exe_path);

    std::fs::write(&output_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(content)
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn generate_network_startnet(
    output_path: String,
    server_url: String,
) -> Result<String, String> {
    use crate::build::StartnetGenerator;

    let content = StartnetGenerator::generate_network_boot(&server_url);

    std::fs::write(&output_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(content)
}

// ============================================================================
// ISO Creation Commands
// ============================================================================

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoRequest {
    pub source_dir: String,
    pub output_path: String,
    pub volume_label: String,
}

#[cfg(feature = "gui")]
#[tauri::command]
pub fn create_iso(request: IsoRequest) -> Result<String, String> {
    use crate::build::IsoCreator;
    use std::path::PathBuf;

    IsoCreator::create_iso(
        &PathBuf::from(&request.source_dir),
        &PathBuf::from(&request.output_path),
        &request.volume_label,
    )
    .map_err(|e| format!("ISO creation failed: {}", e))?;

    Ok(request.output_path)
}
