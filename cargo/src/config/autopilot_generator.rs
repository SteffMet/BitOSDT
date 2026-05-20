use crate::core::errors::{BitOSDTError, BitOSDTResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;
use uuid::Uuid;

/// Autopilot configuration generator
pub struct AutopilotGenerator;

/// Autopilot deployment profile configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotProfile {
    /// Azure AD Tenant ID
    pub tenant_id: String,

    /// Tenant domain (e.g., contoso.onmicrosoft.com)
    pub tenant_domain: String,

    /// Device name template (supports %SERIAL%, %RAND:X%)
    pub device_name_template: Option<String>,

    /// Deployment mode
    pub deployment_mode: DeploymentMode,

    /// OOBE configuration
    pub oobe_config: AutopilotOobeConfig,

    /// Group tag for device assignment
    pub group_tag: Option<String>,

    /// Assigned user UPN (optional)
    pub assigned_user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeploymentMode {
    /// User-driven deployment
    UserDriven,
    /// Self-deploying mode (no user interaction)
    SelfDeploying,
    /// Pre-provisioned (white glove)
    PreProvisioned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotOobeConfig {
    /// Hide keyboard selection
    pub hide_keyboard: bool,
    /// Hide escape button during OOBE
    pub hide_escape: bool,
    /// Hide privacy settings
    pub hide_privacy: bool,
    /// Hide EULA
    pub hide_eula: bool,
    /// Enable white glove OOBE
    pub enable_white_glove: bool,
    /// Require user to accept terms
    pub user_accept_terms: bool,
}

impl Default for AutopilotOobeConfig {
    fn default() -> Self {
        Self {
            hide_keyboard: true,
            hide_escape: true,
            hide_privacy: true,
            hide_eula: true,
            enable_white_glove: false,
            user_accept_terms: false,
        }
    }
}

/// Generated Autopilot configuration file content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfigurationFile {
    #[serde(rename = "CloudAssignedTenantId")]
    pub cloud_assigned_tenant_id: String,

    #[serde(rename = "CloudAssignedTenantDomain")]
    pub cloud_assigned_tenant_domain: String,

    #[serde(rename = "CloudAssignedDeviceName")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_assigned_device_name: Option<String>,

    #[serde(rename = "CloudAssignedOobeConfig")]
    pub cloud_assigned_oobe_config: i32,

    #[serde(rename = "CloudAssignedDomainJoinMethod")]
    pub cloud_assigned_domain_join_method: i32,

    #[serde(rename = "CloudAssignedLanguage")]
    pub cloud_assigned_language: String,

    #[serde(rename = "CloudAssignedForcedEnrollment")]
    pub cloud_assigned_forced_enrollment: i32,

    #[serde(rename = "ZtdCorrelationId")]
    pub ztd_correlation_id: String,

    #[serde(rename = "ZtdRegistrationId")]
    pub ztd_registration_id: String,

    #[serde(rename = "CloudAssignedAadServerData")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_assigned_aad_server_data: Option<String>,
}

impl AutopilotGenerator {
    /// Generate AutopilotConfigurationFile.json content
    pub fn generate_configuration(profile: &AutopilotProfile) -> BitOSDTResult<String> {
        info!(
            "Generating Autopilot configuration for tenant: {}",
            profile.tenant_id
        );

        let oobe_config =
            Self::calculate_oobe_config(&profile.oobe_config, &profile.deployment_mode);
        let domain_join_method = Self::get_domain_join_method(&profile.deployment_mode);

        let config = AutopilotConfigurationFile {
            cloud_assigned_tenant_id: profile.tenant_id.clone(),
            cloud_assigned_tenant_domain: profile.tenant_domain.clone(),
            cloud_assigned_device_name: profile.device_name_template.clone(),
            cloud_assigned_oobe_config: oobe_config,
            cloud_assigned_domain_join_method: domain_join_method,
            cloud_assigned_language: "en-US".to_string(),
            cloud_assigned_forced_enrollment: 1,
            ztd_correlation_id: Uuid::new_v4().to_string(),
            ztd_registration_id: Uuid::new_v4().to_string(),
            cloud_assigned_aad_server_data: None, // Would need AAD token for real enrollment
        };

        let json = serde_json::to_string_pretty(&config)?;

        Ok(json)
    }

    /// Calculate the CloudAssignedOobeConfig bitmask
    fn calculate_oobe_config(oobe: &AutopilotOobeConfig, mode: &DeploymentMode) -> i32 {
        let mut config: i32 = 0;

        // Bit flags for OOBE configuration
        // Reference: https://docs.microsoft.com/en-us/mem/autopilot/existing-devices

        // Bit 0: Skip keyboard selection
        if oobe.hide_keyboard {
            config |= 1;
        }

        // Bit 1: Skip EULA
        if oobe.hide_eula {
            config |= 2;
        }

        // Bit 2: Skip privacy settings
        if oobe.hide_privacy {
            config |= 4;
        }

        // Bit 3: Skip escape
        if oobe.hide_escape {
            config |= 8;
        }

        // Bit 4: Hide change account option
        config |= 16;

        // Bit 5: Hide sign-in options
        config |= 32;

        // Bit 6: Skip device name
        if matches!(mode, DeploymentMode::SelfDeploying) {
            config |= 64;
        }

        // Bit 7: Enable white glove
        if oobe.enable_white_glove {
            config |= 128;
        }

        // Bit 8: Skip OEM registration
        config |= 256;

        // Bit 9: Skip express settings
        config |= 512;

        // Bit 10: Skip OEM EULA
        config |= 1024;

        config
    }

    /// Get domain join method based on deployment mode
    fn get_domain_join_method(mode: &DeploymentMode) -> i32 {
        match mode {
            DeploymentMode::UserDriven => 0,     // Azure AD Join
            DeploymentMode::SelfDeploying => 0,  // Azure AD Join
            DeploymentMode::PreProvisioned => 1, // Hybrid Azure AD Join
        }
    }

    /// Save Autopilot configuration to file
    pub fn save_configuration(
        profile: &AutopilotProfile,
        output_path: &Path,
    ) -> BitOSDTResult<PathBuf> {
        let json = Self::generate_configuration(profile)?;

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Default filename
        let file_path = if output_path.is_dir() {
            output_path.join("AutopilotConfigurationFile.json")
        } else {
            output_path.to_path_buf()
        };

        fs::write(&file_path, &json)?;

        info!("Autopilot configuration saved to {:?}", file_path);
        Ok(file_path)
    }

    /// Generate a hash collection script for Autopilot registration
    pub fn generate_hash_collection_script() -> String {
        r#"# BitOSDT Autopilot Hash Collection Script
# This script collects the hardware hash for Autopilot device registration

[CmdletBinding()]
param (
    [Parameter(Mandatory=$false)]
    [string]$OutputPath = "$env:TEMP\AutopilotHWID.csv"
)

$ErrorActionPreference = 'Stop'

# Check if running as administrator
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")
if (-not $isAdmin) {
    Write-Warning "This script should be run as Administrator for best results"
}

# Install or update the Get-WindowsAutoPilotInfo script
Write-Host "Checking for Get-WindowsAutoPilotInfo script..." -ForegroundColor Cyan

$scriptInstalled = Get-Command Get-WindowsAutoPilotInfo -ErrorAction SilentlyContinue
if (-not $scriptInstalled) {
    Write-Host "Installing Get-WindowsAutoPilotInfo..." -ForegroundColor Yellow
    Install-Script -Name Get-WindowsAutoPilotInfo -Force -Scope CurrentUser
}

# Collect the hardware hash
Write-Host "Collecting hardware hash..." -ForegroundColor Cyan
Get-WindowsAutoPilotInfo -OutputFile $OutputPath

# Display results
Write-Host "`nHardware hash saved to: $OutputPath" -ForegroundColor Green
Write-Host "`nCSV Contents:" -ForegroundColor Cyan
Get-Content $OutputPath | Write-Host

# Instructions for next steps
Write-Host "`n" + "="*60 -ForegroundColor Gray
Write-Host "Next Steps:" -ForegroundColor Yellow
Write-Host "1. Upload this CSV to Microsoft Intune or Microsoft 365 Admin Center"
Write-Host "2. Assign an Autopilot deployment profile to the device"
Write-Host "3. Reset/reinstall Windows to begin Autopilot enrollment"
Write-Host "="*60 -ForegroundColor Gray
"#.to_string()
    }

    /// Generate Autopilot enrollment script for offline provisioning
    pub fn generate_offline_enrollment_script(profile: &AutopilotProfile) -> BitOSDTResult<String> {
        let config_json = Self::generate_configuration(profile)?;

        let script = format!(
            r#"# BitOSDT Autopilot Offline Enrollment Script
# This script applies Autopilot configuration for offline provisioning

$ErrorActionPreference = 'Stop'

# Autopilot configuration
$autopilotConfig = @'
{}
'@

# Target path for the configuration file
$targetPath = "$env:WINDIR\Provisioning\Autopilot\AutopilotConfigurationFile.json"

# Create directory if it doesn't exist
$targetDir = Split-Path $targetPath -Parent
if (-not (Test-Path $targetDir)) {{
    New-Item -Path $targetDir -ItemType Directory -Force | Out-Null
}}

# Write configuration file
$autopilotConfig | Out-File -FilePath $targetPath -Encoding utf8 -Force

Write-Host "Autopilot configuration applied successfully" -ForegroundColor Green
Write-Host "Configuration file: $targetPath" -ForegroundColor Cyan
Write-Host "`nThe device will enroll in Autopilot on next OOBE." -ForegroundColor Yellow
"#,
            config_json
        );

        Ok(script)
    }

    /// Validate an Autopilot profile
    pub fn validate_profile(profile: &AutopilotProfile) -> BitOSDTResult<Vec<String>> {
        let mut warnings = Vec::new();

        // Validate tenant ID format (GUID)
        if Uuid::parse_str(&profile.tenant_id).is_err() {
            return Err(BitOSDTError::Validation(
                "Invalid tenant ID format - must be a valid GUID".to_string(),
            ));
        }

        // Validate domain format
        if !profile.tenant_domain.contains('.') {
            return Err(BitOSDTError::Validation(
                "Invalid tenant domain format".to_string(),
            ));
        }

        // Check for common configuration issues
        if matches!(profile.deployment_mode, DeploymentMode::SelfDeploying)
            && profile.device_name_template.is_none()
        {
            warnings.push(
                "Self-deploying mode without device name template - random names will be used"
                    .to_string(),
            );
        }

        if profile.oobe_config.enable_white_glove
            && !matches!(profile.deployment_mode, DeploymentMode::PreProvisioned)
        {
            warnings
                .push("White glove enabled but deployment mode is not PreProvisioned".to_string());
        }

        Ok(warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile() -> AutopilotProfile {
        AutopilotProfile {
            tenant_id: "12345678-1234-1234-1234-123456789012".to_string(),
            tenant_domain: "contoso.onmicrosoft.com".to_string(),
            device_name_template: Some("PC-%SERIAL%".to_string()),
            deployment_mode: DeploymentMode::UserDriven,
            oobe_config: AutopilotOobeConfig::default(),
            group_tag: Some("Production".to_string()),
            assigned_user: None,
        }
    }

    #[test]
    fn test_generate_configuration() {
        let profile = create_test_profile();
        let json = AutopilotGenerator::generate_configuration(&profile).unwrap();

        assert!(json.contains("12345678-1234-1234-1234-123456789012"));
        assert!(json.contains("contoso.onmicrosoft.com"));
        assert!(json.contains("CloudAssignedTenantId"));
    }

    #[test]
    fn test_oobe_config_calculation() {
        let oobe = AutopilotOobeConfig::default();
        let config = AutopilotGenerator::calculate_oobe_config(&oobe, &DeploymentMode::UserDriven);

        // Should have multiple flags set
        assert!(config > 0);
        // Should include keyboard skip (bit 0)
        assert!(config & 1 == 1);
    }

    #[test]
    fn test_validate_profile_valid() {
        let profile = create_test_profile();
        let warnings = AutopilotGenerator::validate_profile(&profile).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_profile_invalid_tenant_id() {
        let mut profile = create_test_profile();
        profile.tenant_id = "invalid-guid".to_string();

        let result = AutopilotGenerator::validate_profile(&profile);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_hash_collection_script() {
        let script = AutopilotGenerator::generate_hash_collection_script();
        assert!(script.contains("Get-WindowsAutoPilotInfo"));
        assert!(script.contains("AutopilotHWID.csv"));
    }

    #[test]
    fn test_generate_offline_enrollment_script() {
        let profile = create_test_profile();
        let script = AutopilotGenerator::generate_offline_enrollment_script(&profile).unwrap();

        assert!(script.contains("AutopilotConfigurationFile.json"));
        assert!(script.contains("12345678-1234-1234-1234-123456789012"));
    }
}
