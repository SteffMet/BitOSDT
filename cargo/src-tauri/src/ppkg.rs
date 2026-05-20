use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

use crate::oobe_profiles::{self, OobeProfileRequest};

const PROFILE_MANIFEST_FILE: &str = ".bitosdt-oobe.json";
const AUTOUNATTEND_FILE: &str = "Autounattend.xml";
const PROVISIONING_BOOTSTRAP_SCRIPT: &str = "Apply-BitOSDTProvisioning.ps1";
const PPKG_README_FILE: &str = "PPKG-README.txt";
const PROVISIONING_TOOLS_MODULE: &str = "ProvisioningTools";
const MISSING_TOOLING_MESSAGE: &str = "Provisioning package tooling was not found. Install Windows Configuration Designer (ICD/WCD) or provide builderPath in the request.";
const ICD_CUSTOMIZATION_FILE: &str = "Customization.xml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpkgSigningMetadata {
    pub pfx_path: String,
    pub password: Option<String>,
    pub timestamp_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpkgRequest {
    pub profile_name: Option<String>,
    pub profile_path: Option<String>,
    pub output_ppkg_path: String,
    pub builder_path: Option<String>,
    pub owner: Option<String>,
    pub rank: Option<u32>,
    pub version: Option<String>,
    pub signing: Option<PpkgSigningMetadata>,
    pub local_admin_username: Option<String>,
    pub local_admin_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpkgResponse {
    pub output_ppkg_path: String,
    pub logs_path: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpkgCapabilityStatus {
    pub native_builder_available: bool,
    pub local_admin_credentials_required: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProvisioningPayload {
    profile_name: String,
    generated_at: String,
    owner: String,
    rank: u32,
    version: String,
    settings: ProvisioningSettings,
    assets: ProvisioningAssets,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProvisioningSettings {
    skip_machine_oobe: bool,
    skip_user_oobe: bool,
    hide_eula: bool,
    hide_privacy_settings: bool,
    hide_online_account_screens: bool,
    network_location: String,
    protect_your_pc: String,
    prompt_for_computer_name: bool,
    domain_join_enabled: bool,
    default_user_enabled: bool,
    wifi_enabled: bool,
    disable_bitlocker: bool,
    reboot_after_disable_bitlocker: bool,
    winget_package_count: usize,
    chocolatey_package_count: usize,
    embedded_installer_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProvisioningAssets {
    autounattend: String,
    scripts_dir: String,
    apps_dir: String,
    files_dir: String,
    source_profile_path: String,
}

#[derive(Debug)]
struct ResolvedRequest {
    profile_name: String,
    profile_path: PathBuf,
    output_ppkg_path: PathBuf,
}

pub fn generate_oobe_ppkg(request: PpkgRequest) -> Result<PpkgResponse, String> {
    let resolved = validate_request(&request)?;

    let mut warnings = Vec::new();
    let staging_dir = std::env::temp_dir().join(format!("bitosdt-ppkg-{}", Uuid::new_v4()));
    let content_dir = staging_dir.join("Content");
    let metadata_dir = staging_dir.join("Metadata");
    fs::create_dir_all(&content_dir)
        .map_err(|e| format!("Failed to create content staging directory: {}", e))?;
    fs::create_dir_all(&metadata_dir)
        .map_err(|e| format!("Failed to create metadata staging directory: {}", e))?;

    let request_data = read_profile_request(&resolved.profile_path)?;
    stage_profile_assets(&resolved.profile_path, &content_dir, &mut warnings)?;
    oobe_profiles::materialize_request_derived_provisioning_payload(&request_data, &content_dir)?;
    copy_if_exists(
        &content_dir.join(PROVISIONING_BOOTSTRAP_SCRIPT),
        &content_dir
            .join("Scripts")
            .join(PROVISIONING_BOOTSTRAP_SCRIPT),
        &mut warnings,
        PROVISIONING_BOOTSTRAP_SCRIPT,
    )?;
    rewrite_staged_bootstrap_script(
        &content_dir,
        &resolved.output_ppkg_path,
        request_data.oobe_config.hide_privacy_settings,
    )?;

    let payload = map_payload(
        &resolved.profile_name,
        &resolved.profile_path,
        &request_data,
        &request,
    );
    let payload_path = metadata_dir.join("provisioningPayload.json");
    let payload_json = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("Failed to serialize provisioning payload: {}", e))?;
    fs::write(&payload_path, payload_json)
        .map_err(|e| format!("Failed to write payload file: {}", e))?;
    warnings.extend(collect_support_warnings(&request_data));
    write_icd_customization_xml(
        &staging_dir,
        &resolved.profile_name,
        &request,
        &request_data,
    )?;

    let logs_path = resolved.output_ppkg_path.with_extension("ppkg.log");
    run_builder(
        &request,
        &staging_dir,
        &resolved.output_ppkg_path,
        &logs_path,
    )?;
    export_sidecar_assets(
        &content_dir,
        &resolved.output_ppkg_path,
        resolved
            .output_ppkg_path
            .parent()
            .map(|path| path == resolved.profile_path)
            .unwrap_or(false),
        &mut warnings,
    )?;

    Ok(PpkgResponse {
        output_ppkg_path: resolved.output_ppkg_path.to_string_lossy().to_string(),
        logs_path: logs_path.to_string_lossy().to_string(),
        warnings,
    })
}

fn sanitize_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_ascii_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_string()
}

fn trim_wrapping_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[trimmed.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn validate_request(request: &PpkgRequest) -> Result<ResolvedRequest, String> {
    let normalized_output = trim_wrapping_quotes(&request.output_ppkg_path).trim();
    if normalized_output.is_empty() {
        return Err("Output .ppkg path is required.".to_string());
    }
    if normalized_output.contains('"') {
        return Err(format!(
            "Output .ppkg path contains an invalid quote character: {}",
            normalized_output
        ));
    }

    let output_ppkg_path = PathBuf::from(normalized_output);
    let is_ppkg = output_ppkg_path
        .extension()
        .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("ppkg"))
        .unwrap_or(false);
    if !is_ppkg {
        return Err("Output path must use a .ppkg extension.".to_string());
    }

    let by_name = request
        .profile_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|name| {
            let clean = sanitize_name(name);
            let path = oobe_profiles::resolve_oobe_profile_path(&clean)
                .unwrap_or_else(|| Path::new(oobe_profiles::OOBE_ROOT).join(&clean));
            (clean, path)
        });

    let by_path = request
        .profile_path
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);

    let (profile_name, profile_path) = match (by_name, by_path) {
        (Some((name, path)), _) => (name, path),
        (None, Some(path)) => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .ok_or_else(|| "Profile path does not include a folder name.".to_string())?;
            (name, path)
        }
        (None, None) => {
            return Err(
                "Provide either profileName or profilePath to export a package.".to_string(),
            )
        }
    };

    if !profile_path.exists() {
        return Err(format!(
            "Profile path does not exist: {}",
            profile_path.display()
        ));
    }
    if !profile_path.is_dir() {
        return Err(format!(
            "Profile path is not a directory: {}",
            profile_path.display()
        ));
    }

    Ok(ResolvedRequest {
        profile_name,
        profile_path,
        output_ppkg_path,
    })
}

#[derive(Debug, Deserialize)]
struct ManifestEnvelope {
    request: OobeProfileRequest,
}

fn read_profile_request(profile_path: &Path) -> Result<OobeProfileRequest, String> {
    let manifest_path = profile_path.join(PROFILE_MANIFEST_FILE);
    if manifest_path.is_file() {
        let content = fs::read_to_string(&manifest_path).map_err(|e| {
            format!(
                "Failed to read profile manifest {}: {}",
                manifest_path.display(),
                e
            )
        })?;
        let parsed: ManifestEnvelope = serde_json::from_str(&content).map_err(|e| {
            format!(
                "Failed to parse profile manifest {}: {}",
                manifest_path.display(),
                e
            )
        })?;
        return Ok(parsed.request);
    }

    Ok(OobeProfileRequest {
        name: profile_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "UnnamedProfile".to_string()),
        ..Default::default()
    })
}

fn copy_if_exists(
    src: &Path,
    dst: &Path,
    warnings: &mut Vec<String>,
    label: &str,
) -> Result<(), String> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
        fs::copy(src, dst).map_err(|e| format!("Failed to copy {}: {}", src.display(), e))?;
    } else {
        warnings.push(format!(
            "{} was not found in profile and was skipped.",
            label
        ));
    }
    Ok(())
}

fn copy_tree_if_exists(
    src: &Path,
    dst: &Path,
    warnings: &mut Vec<String>,
    label: &str,
) -> Result<(), String> {
    if !src.exists() {
        warnings.push(format!(
            "{} directory was not found in profile and was skipped.",
            label
        ));
        return Ok(());
    }

    fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {}: {}", dst.display(), e))?;
    for entry in fs::read_dir(src)
        .map_err(|e| format!("Failed to read directory {}: {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| {
            format!(
                "Failed to inspect directory entry in {}: {}",
                src.display(),
                e
            )
        })?;
        let child_src = entry.path();
        let child_dst = dst.join(entry.file_name());
        if child_src.is_dir() {
            copy_tree_if_exists(&child_src, &child_dst, warnings, label)?;
        } else {
            fs::copy(&child_src, &child_dst)
                .map_err(|e| format!("Failed to copy {}: {}", child_src.display(), e))?;
        }
    }
    Ok(())
}

fn stage_profile_assets(
    profile_path: &Path,
    content_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    copy_if_exists(
        &profile_path.join(AUTOUNATTEND_FILE),
        &content_dir.join(AUTOUNATTEND_FILE),
        warnings,
        AUTOUNATTEND_FILE,
    )?;
    copy_tree_if_exists(
        &profile_path.join("Apps"),
        &content_dir.join("Apps"),
        warnings,
        "Apps",
    )?;
    copy_tree_if_exists(
        &profile_path.join("Files"),
        &content_dir.join("Files"),
        warnings,
        "Files",
    )?;
    Ok(())
}

fn quote_icd_arg_value(path: &Path) -> String {
    format!(r#""{}""#, path.display())
}

fn rewrite_staged_bootstrap_script(
    content_dir: &Path,
    output_ppkg_path: &Path,
    hide_privacy_settings: bool,
) -> Result<(), String> {
    let expected_name = output_ppkg_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| {
            format!(
                "Output .ppkg path does not include a file name: {}",
                output_ppkg_path.display()
            )
        })?;
    let bootstrap_path = content_dir
        .join("Scripts")
        .join(PROVISIONING_BOOTSTRAP_SCRIPT);
    if !bootstrap_path.is_file() {
        return Ok(());
    }

    fs::write(
        &bootstrap_path,
        oobe_profiles::build_provisioning_bootstrap_script(
            Some(&expected_name),
            hide_privacy_settings,
        ),
    )
    .map_err(|e| {
        format!(
            "Failed to rewrite staged provisioning bootstrap {}: {}",
            bootstrap_path.display(),
            e
        )
    })
}

fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| {
            format!(
                "Failed to remove existing directory {}: {}",
                path.display(),
                e
            )
        })?;
    }
    Ok(())
}

fn export_sidecar_assets(
    sidecar_root: &Path,
    output_ppkg_path: &Path,
    merge_into_output: bool,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let output_dir = output_ppkg_path.parent().ok_or_else(|| {
        format!(
            "Output .ppkg path does not include a parent directory: {}",
            output_ppkg_path.display()
        )
    })?;

    let scripts_target = output_dir.join("Scripts");
    let scripts_source = sidecar_root.join("Scripts");
    if scripts_source != scripts_target {
        if !merge_into_output {
            remove_dir_if_exists(&scripts_target)?;
        }
        copy_tree_if_exists(&scripts_source, &scripts_target, warnings, "Scripts")?;
    }

    let apps_target = output_dir.join("Apps");
    let apps_source = sidecar_root.join("Apps");
    if apps_source != apps_target {
        if !merge_into_output {
            remove_dir_if_exists(&apps_target)?;
        }
        copy_tree_if_exists(&apps_source, &apps_target, warnings, "Apps")?;
    }

    let files_target = output_dir.join("Files");
    let files_source = sidecar_root.join("Files");
    if files_source != files_target {
        if !merge_into_output {
            remove_dir_if_exists(&files_target)?;
        }
        copy_tree_if_exists(&files_source, &files_target, warnings, "Files")?;
    }

    let readme_source = sidecar_root.join(PPKG_README_FILE);
    let readme_target = output_dir.join(PPKG_README_FILE);
    if readme_source != readme_target {
        copy_if_exists(&readme_source, &readme_target, warnings, PPKG_README_FILE)?;
    }

    Ok(())
}

fn map_payload(
    profile_name: &str,
    profile_path: &Path,
    request: &OobeProfileRequest,
    ppkg_request: &PpkgRequest,
) -> ProvisioningPayload {
    ProvisioningPayload {
        profile_name: profile_name.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        owner: ppkg_request
            .owner
            .clone()
            .unwrap_or_else(|| "ITAdmin".to_string()),
        rank: ppkg_request.rank.unwrap_or(0),
        version: ppkg_request
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0.0".to_string()),
        settings: ProvisioningSettings {
            skip_machine_oobe: request.oobe_config.skip_machine_oobe,
            skip_user_oobe: request.oobe_config.skip_user_oobe,
            hide_eula: request.oobe_config.hide_eula,
            hide_privacy_settings: request.oobe_config.hide_privacy_settings,
            hide_online_account_screens: request.oobe_config.hide_online_account_screens,
            network_location: request.oobe_config.network_location.clone(),
            protect_your_pc: request.oobe_config.protect_your_pc.clone(),
            prompt_for_computer_name: request.prompt_for_computer_name,
            domain_join_enabled: request.domain_join.enabled,
            default_user_enabled: request.default_user.enabled,
            wifi_enabled: request.wifi.enabled,
            disable_bitlocker: request.apps.disable_bitlocker,
            reboot_after_disable_bitlocker: request.apps.reboot_after_disable_bitlocker,
            winget_package_count: request
                .apps
                .winget_packages
                .iter()
                .filter(|p| p.enabled)
                .count(),
            chocolatey_package_count: request
                .apps
                .chocolatey_packages
                .iter()
                .filter(|p| p.enabled)
                .count(),
            embedded_installer_count: request
                .apps
                .custom_installers
                .iter()
                .filter(|p| p.enabled)
                .count(),
        },
        assets: ProvisioningAssets {
            autounattend: "Content/Autounattend.xml".to_string(),
            scripts_dir: "Content/Scripts".to_string(),
            apps_dir: "Content/Apps".to_string(),
            files_dir: "Content/Files".to_string(),
            source_profile_path: profile_path.to_string_lossy().to_string(),
        },
    }
}

fn icd_customization_path(staging_dir: &Path) -> PathBuf {
    staging_dir.join(ICD_CUSTOMIZATION_FILE)
}

fn normalize_owner_type(owner: Option<&str>) -> &'static str {
    match owner
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "oem" => "OEM",
        "mobileoperator" => "MobileOperator",
        "siloading" => "Siloading",
        _ => "ITAdmin",
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn collect_files_recursive(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(root).map_err(|e| {
        format!(
            "Failed to read dependency directory {}: {}",
            root.display(),
            e
        )
    })? {
        let entry = entry.map_err(|e| {
            format!(
                "Failed to inspect dependency entry in {}: {}",
                root.display(),
                e
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, output)?;
        } else if path.is_file() {
            output.push(path);
        }
    }

    Ok(())
}

fn collect_command_dependencies(
    scripts_dir: &Path,
    apps_dir: &Path,
    files_dir: &Path,
    command_file: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_recursive(scripts_dir, &mut files)?;
    collect_files_recursive(apps_dir, &mut files)?;
    collect_files_recursive(files_dir, &mut files)?;

    let command_file =
        fs::canonicalize(command_file).unwrap_or_else(|_| command_file.to_path_buf());
    files.retain(|path| {
        fs::canonicalize(path)
            .map(|candidate| candidate != command_file)
            .unwrap_or(true)
    });
    files.sort();
    files.dedup();
    Ok(files)
}

fn write_icd_customization_xml(
    staging_dir: &Path,
    profile_name: &str,
    request: &PpkgRequest,
    profile_request: &OobeProfileRequest,
) -> Result<(), String> {
    let path = icd_customization_path(staging_dir);
    let owner_type = normalize_owner_type(request.owner.as_deref());
    let version = request
        .version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("1.0.0.0");
    let rank = request.rank.unwrap_or(0);
    let package_id = format!("{{{}}}", Uuid::new_v4());
    let content_dir = staging_dir.join("Content");
    let scripts_dir = content_dir.join("Scripts");
    let apps_dir = content_dir.join("Apps");
    let files_dir = content_dir.join("Files");
    let command_file = scripts_dir.join(PROVISIONING_BOOTSTRAP_SCRIPT);
    let accounts_block = build_accounts_block(profile_request);
    let oobe_block = build_oobe_block(profile_request);
    let policies_block = build_policies_block();
    let variant_block = build_wifi_variant_block(profile_request);

    let command_block = if command_file.is_file() {
        let dependencies =
            collect_command_dependencies(&scripts_dir, &apps_dir, &files_dir, &command_file)?;
        let mut dependency_xml = String::new();
        if !dependencies.is_empty() {
            dependency_xml.push_str("                <DependencyPackages>\n");
            for (index, path) in dependencies.iter().enumerate() {
                dependency_xml.push_str(&format!(
                    "                  <Dependency Name=\"Asset{:03}\">{}</Dependency>\n",
                    index + 1,
                    xml_escape(&path.to_string_lossy())
                ));
            }
            dependency_xml.push_str("                </DependencyPackages>\n");
        }

        format!(
            r#"        <ProvisioningCommands>
          <PrimaryContext>
            <Command>
              <CommandConfig Name="BitOSDTBootstrap">
                <CommandFile>{}</CommandFile>
                <CommandLine>{}</CommandLine>
                <ContinueInstall>True</ContinueInstall>
{}                <RestartRequired>False</RestartRequired>
                <ReturnCodeRestart>3010</ReturnCodeRestart>
                <ReturnCodeSuccess>0</ReturnCodeSuccess>
              </CommandConfig>
            </Command>
          </PrimaryContext>
        </ProvisioningCommands>
"#,
            xml_escape(&command_file.to_string_lossy()),
            xml_escape(
                "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"Apply-BitOSDTProvisioning.ps1\""
            ),
            dependency_xml
        )
    } else {
        // Keep the package valid even when a provisioning bootstrap script was not staged.
        "        <ProvisioningCommands>\n          <DeviceContext>\n            <CommandLine>cmd /c exit /b 0</CommandLine>\n          </DeviceContext>\n        </ProvisioningCommands>\n".to_string()
    };

    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<WindowsCustomizations>
  <PackageConfig xmlns="urn:schemas-Microsoft-com:Windows-ICD-Package-Config.v1.0">
    <ID>{}</ID>
    <Name>{}</Name>
    <Version>{}</Version>
    <OwnerType>{}</OwnerType>
    <Rank>{}</Rank>
  </PackageConfig>
  <Settings xmlns="urn:schemas-microsoft-com:windows-provisioning">
    <Customizations>
      <Common>
{}{}{}{}
      </Common>
{}
    </Customizations>
  </Settings>
</WindowsCustomizations>
"#,
        xml_escape(&package_id),
        xml_escape(profile_name),
        xml_escape(version),
        xml_escape(owner_type),
        rank,
        accounts_block,
        oobe_block,
        policies_block,
        command_block,
        variant_block
    );

    fs::write(&path, xml).map_err(|e| {
        format!(
            "Failed to write ICD customization XML {}: {}",
            path.display(),
            e
        )
    })
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn fixed_computer_name(request: &OobeProfileRequest) -> Option<&str> {
    request
        .oobe_config
        .computer_name
        .as_deref()
        .and_then(trimmed_non_empty)
}

fn has_wifi_dns_overrides(request: &OobeProfileRequest) -> bool {
    trimmed_non_empty(&request.wifi.dns_server_1).is_some()
        || trimmed_non_empty(&request.wifi.dns_server_2).is_some()
}

fn can_apply_native_wifi_profile(request: &OobeProfileRequest) -> bool {
    request.wifi.enabled
        && !request.wifi.hidden_network
        && !has_wifi_dns_overrides(request)
        && matches!(request.wifi.authentication.as_str(), "Open" | "Wpa2Psk")
}

fn can_apply_native_domain_join(request: &OobeProfileRequest) -> bool {
    request.domain_join.enabled
        && request.domain_join_mode == oobe_profiles::DomainJoinMode::SpecializeXml
        && fixed_computer_name(request).is_some()
        && request
            .domain_join
            .ou_path
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_none()
}

fn uses_hide_oobe_setting(request: &OobeProfileRequest) -> bool {
    request.oobe_config.skip_machine_oobe && request.oobe_config.skip_user_oobe
}

fn build_accounts_block(request: &OobeProfileRequest) -> String {
    let fixed_name = fixed_computer_name(request);
    let native_domain_join = can_apply_native_domain_join(request);
    let default_user = &request.default_user;

    if fixed_name.is_none() && !native_domain_join && !default_user.enabled {
        return String::new();
    }

    let mut xml = String::from("        <Accounts>\n");

    if fixed_name.is_some() || native_domain_join {
        xml.push_str("          <ComputerAccount>\n");
        if let Some(name) = fixed_name {
            xml.push_str(&format!(
                "            <ComputerName>{}</ComputerName>\n",
                xml_escape(name)
            ));
        }
        if native_domain_join {
            xml.push_str(&format!(
                "            <DomainName>{}</DomainName>\n",
                xml_escape(request.domain_join.domain.trim())
            ));
            xml.push_str(&format!(
                "            <Account>{}</Account>\n",
                xml_escape(request.domain_join.username.trim())
            ));
            xml.push_str(&format!(
                "            <Password>{}</Password>\n",
                xml_escape(&request.domain_join.password)
            ));
        }
        xml.push_str("          </ComputerAccount>\n");
    }

    if default_user.enabled {
        xml.push_str("          <Users>\n");
        xml.push_str(&format!(
            "            <User UserName=\"{}\">\n",
            xml_escape(default_user.username.trim())
        ));
        xml.push_str(&format!(
            "              <Password>{}</Password>\n",
            xml_escape(&default_user.password)
        ));
        xml.push_str(&format!(
            "              <UserGroup>{}</UserGroup>\n",
            xml_escape(default_user.group.trim())
        ));
        xml.push_str("            </User>\n");
        xml.push_str("          </Users>\n");
    }

    xml.push_str("        </Accounts>\n");
    xml
}

fn build_oobe_block(request: &OobeProfileRequest) -> String {
    if !uses_hide_oobe_setting(request) {
        return String::new();
    }

    "        <OOBE>\n          <Desktop>\n            <HideOobe>True</HideOobe>\n          </Desktop>\n        </OOBE>\n"
        .to_string()
}

fn build_policies_block() -> String {
    "        <Policies>\n          <ApplicationManagement>\n            <AllowAllTrustedApps>Yes</AllowAllTrustedApps>\n          </ApplicationManagement>\n        </Policies>\n"
        .to_string()
}

fn map_wifi_security_type(authentication: &str) -> Option<&'static str> {
    match authentication {
        "Open" => Some("Open"),
        "Wpa2Psk" => Some("WPA2-Personal"),
        _ => None,
    }
}

fn build_wifi_variant_block(request: &OobeProfileRequest) -> String {
    if !can_apply_native_wifi_profile(request) {
        return String::new();
    }

    let security_type = match map_wifi_security_type(&request.wifi.authentication) {
        Some(value) => value,
        None => return String::new(),
    };

    let mut security_key = String::new();
    if security_type != "Open" {
        security_key.push_str(&format!(
            "                    <SecurityKey>{}</SecurityKey>\n",
            xml_escape(&request.wifi.password)
        ));
    }

    format!(
        concat!(
            "      <Targets>\n",
            "        <Target Id=\"laptop\">\n",
            "          <TargetState>\n",
            "            <Condition Name=\"PowerPlatformRole\" Value=\"2\" />\n",
            "          </TargetState>\n",
            "        </Target>\n",
            "      </Targets>\n",
            "      <Variant>\n",
            "        <TargetRefs>\n",
            "          <TargetRef Id=\"laptop\" />\n",
            "        </TargetRefs>\n",
            "        <Settings>\n",
            "          <ConnectivityProfiles>\n",
            "            <WLAN>\n",
            "              <WLANSetting>\n",
            "                <WLANConfig SSID=\"{}\">\n",
            "                  <WLANXmlSettings>\n",
            "                    <SecurityType>{}</SecurityType>\n",
            "{}",
            "                    <AutoConnect>{}</AutoConnect>\n",
            "                  </WLANXmlSettings>\n",
            "                </WLANConfig>\n",
            "              </WLANSetting>\n",
            "            </WLAN>\n",
            "          </ConnectivityProfiles>\n",
            "        </Settings>\n",
            "      </Variant>\n"
        ),
        xml_escape(request.wifi.ssid.trim()),
        xml_escape(security_type),
        security_key,
        if request.wifi.auto_connect {
            "True"
        } else {
            "False"
        }
    )
}

fn collect_support_warnings(request: &OobeProfileRequest) -> Vec<String> {
    let mut warnings = Vec::new();

    if request.prompt_for_computer_name {
        warnings.push(
            "Provisioning package mode keeps prompted computer naming as a post-sign-in step; only a fixed Computer Name is applied during package installation."
                .to_string(),
        );
    }

    if request.domain_join.enabled && !can_apply_native_domain_join(request) {
        warnings.push(
            "Provisioning package mode will keep domain join in post-sign-in orchestration unless you use Specialize XML mode with a fixed Computer Name."
                .to_string(),
        );
    }

    if request.wifi.enabled && !can_apply_native_wifi_profile(request) {
        warnings.push(
            "Provisioning package mode will keep Wi-Fi orchestration post-sign-in when the profile uses hidden SSIDs, DNS overrides, or unsupported native authentication types."
                .to_string(),
        );
    }

    if request.oobe_config.hide_eula
        || request.oobe_config.hide_wireless_setup
        || request.oobe_config.hide_online_account_screens
        || request.oobe_config.hide_privacy_settings
        || request.oobe_config.hide_local_account_screen
    {
        warnings.push(
            "Only broad HideOobe behavior is applied natively in provisioning-package mode; fine-grained OOBE/privacy toggles remain policy-driven or post-sign-in."
                .to_string(),
        );
    }

    if !request.apps.copied_items.is_empty()
        || request
            .apps
            .winget_packages
            .iter()
            .any(|package| package.enabled)
        || request
            .apps
            .chocolatey_packages
            .iter()
            .any(|package| package.enabled)
        || request
            .apps
            .custom_installers
            .iter()
            .any(|installer| installer.enabled)
        || request.apps.disable_bitlocker
        || (request.apps.enable_custom_scripts
            && request
                .apps
                .custom_scripts
                .iter()
                .any(|script| script.enabled))
        || request.enable_debloat
    {
        warnings.push(
            "BitLocker disable, applications, copied payloads, debloat, and custom scripts still run after the package applies, during the first admin sign-in orchestration flow."
                .to_string(),
        );
    }

    warnings
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuilderFlavor {
    Icd,
    Generic,
}

fn builder_flavor(path: &Path) -> BuilderFlavor {
    let name = path
        .file_stem()
        .map(|v| v.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.contains("icd") || name.contains("wcd") {
        BuilderFlavor::Icd
    } else {
        BuilderFlavor::Generic
    }
}

fn resolve_builder_path(request: &PpkgRequest) -> Option<PathBuf> {
    if let Some(path) = request
        .builder_path
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let builder = PathBuf::from(path);
        if builder.exists() {
            return Some(builder);
        }
    }

    let candidates = [
        r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Imaging and Configuration Designer\x86\ICD.exe",
        r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Imaging and Configuration Designer\x64\ICD.exe",
        "icd.exe",
        "wcd.exe",
    ];

    candidates
        .iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.exists() || candidate.components().count() == 1)
}

pub fn get_ppkg_capability_status(builder_path: Option<String>) -> PpkgCapabilityStatus {
    let explicit_builder = builder_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    let native_builder_available = explicit_builder
        .map(|builder| {
            builder.exists()
                && builder_flavor(&builder) == BuilderFlavor::Icd
                && resolve_icd_store_file(&builder).is_some()
        })
        .or_else(|| {
            let probe_request = PpkgRequest {
                profile_name: None,
                profile_path: None,
                output_ppkg_path: String::new(),
                builder_path: None,
                owner: None,
                rank: None,
                version: None,
                signing: None,
                local_admin_username: None,
                local_admin_password: None,
            };

            resolve_builder_path(&probe_request).map(|builder| {
                builder_flavor(&builder) == BuilderFlavor::Icd
                    && resolve_icd_store_file(&builder).is_some()
            })
        })
        .unwrap_or(false);

    PpkgCapabilityStatus {
        native_builder_available,
        local_admin_credentials_required: !native_builder_available,
    }
}

fn resolve_icd_store_file(builder: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = builder.parent() {
        candidates.push(parent.join("Microsoft-Desktop-Provisioning.dat"));
        candidates.push(parent.join("Microsoft-Common-Provisioning.dat"));
    }

    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Imaging and Configuration Designer\x86\Microsoft-Desktop-Provisioning.dat",
    ));
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Imaging and Configuration Designer\x64\Microsoft-Desktop-Provisioning.dat",
    ));
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Imaging and Configuration Designer\x86\Microsoft-Common-Provisioning.dat",
    ));
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit\Imaging and Configuration Designer\x64\Microsoft-Common-Provisioning.dat",
    ));

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn build_command_arguments(
    flavor: BuilderFlavor,
    staging_dir: &Path,
    output_ppkg_path: &Path,
    icd_store_file: Option<&Path>,
    signing: Option<&PpkgSigningMetadata>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    match flavor {
        BuilderFlavor::Icd => {
            let customization_xml = icd_customization_path(staging_dir);
            args.push("/Build-ProvisioningPackage".into());
            args.push(OsString::from(format!(
                "/CustomizationXML:{}",
                quote_icd_arg_value(&customization_xml)
            )));
            args.push(OsString::from(format!(
                "/PackagePath:{}",
                quote_icd_arg_value(output_ppkg_path)
            )));
            if let Some(store_file) = icd_store_file {
                args.push(OsString::from(format!(
                    "/StoreFile:{}",
                    quote_icd_arg_value(store_file)
                )));
            }
            args.push("+Overwrite".into());
        }
        BuilderFlavor::Generic => {
            args.push("build".into());
            args.push("--project".into());
            args.push(staging_dir.as_os_str().to_os_string());
            args.push("--output".into());
            args.push(output_ppkg_path.as_os_str().to_os_string());
        }
    }

    if let Some(signing) = signing {
        if flavor == BuilderFlavor::Generic {
            args.push("--sign-pfx".into());
            args.push(OsString::from(&signing.pfx_path));
            if let Some(password) = signing.password.as_ref().filter(|v| !v.is_empty()) {
                args.push("--sign-password".into());
                args.push(OsString::from(password));
            }
            if let Some(url) = signing.timestamp_url.as_ref().filter(|v| !v.is_empty()) {
                args.push("--timestamp-url".into());
                args.push(OsString::from(url));
            }
        }
    }

    args
}

fn run_builder(
    request: &PpkgRequest,
    staging_dir: &Path,
    output_ppkg_path: &Path,
    logs_path: &Path,
) -> Result<(), String> {
    ensure_output_directories(output_ppkg_path, logs_path)?;

    if let Some(builder) = resolve_builder_path(request) {
        match run_native_builder(request, staging_dir, output_ppkg_path, logs_path, &builder) {
            Ok(()) => {}
            Err(native_err) => {
                append_log(
                    logs_path,
                    &format!(
                        "native-builder error: {}\nAttempting ProvisioningTools fallback...\n",
                        native_err
                    ),
                )?;

                match run_provisioningtools_fallback(
                    request,
                    staging_dir,
                    output_ppkg_path,
                    logs_path,
                ) {
                    Ok(()) => {
                        append_log(
                            logs_path,
                            "fallback result: success (native-builder failure was bypassed)\n",
                        )?;
                    }
                    Err(fallback_err) => {
                        return Err(format!(
                            "Native builder failed: {} Fallback failed: {}",
                            native_err, fallback_err
                        ));
                    }
                }
            }
        }
    } else {
        // Fallback path installs ProvisioningTools and attempts PPKG build via PowerShell.
        run_provisioningtools_fallback(request, staging_dir, output_ppkg_path, logs_path)?;
    }

    if !output_ppkg_path.exists() {
        return Err(format!(
            "Provisioning tool finished but did not create {}. Review logs: {}",
            output_ppkg_path.display(),
            logs_path.display()
        ));
    }

    Ok(())
}

fn ensure_output_directories(output_ppkg_path: &Path, logs_path: &Path) -> Result<(), String> {
    if let Some(parent) = output_ppkg_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create output directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }
    if let Some(parent) = logs_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create logs directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }
    Ok(())
}

fn append_log(logs_path: &Path, section: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_path)
        .map_err(|e| {
            format!(
                "Failed to open provisioning logs at {}: {}",
                logs_path.display(),
                e
            )
        })?;
    file.write_all(section.as_bytes()).map_err(|e| {
        format!(
            "Failed to write provisioning logs to {}: {}",
            logs_path.display(),
            e
        )
    })
}

fn read_text_if_exists(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn run_icd_builder_via_powershell(
    builder: &Path,
    args: &[OsString],
    logs_path: &Path,
) -> Result<(), String> {
    let stdout_path = logs_path.with_extension("native.stdout.log");
    let stderr_path = logs_path.with_extension("native.stderr.log");
    let shell_script = format!(
        r#"$ErrorActionPreference = 'Stop'
$stdoutPath = {stdout_path}
$stderrPath = {stderr_path}
Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
$args = @(
{args_block}
)
$proc = Start-Process -FilePath {builder_path} -ArgumentList $args -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
exit $proc.ExitCode
"#,
        stdout_path = ps_single_quote(&stdout_path.to_string_lossy()),
        stderr_path = ps_single_quote(&stderr_path.to_string_lossy()),
        builder_path = ps_single_quote(&builder.to_string_lossy()),
        args_block = args
            .iter()
            .map(|arg| format!("    {}", ps_single_quote(&arg.to_string_lossy())))
            .collect::<Vec<_>>()
            .join(",\n"),
    );

    let shells = ["pwsh.exe", "powershell.exe"];
    let mut launch_errors = Vec::new();

    for shell in shells {
        match Command::new(shell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &shell_script,
            ])
            .output()
        {
            Ok(output) => {
                let mut log = String::new();
                log.push_str("strategy: native-builder\n");
                log.push_str(&format!("tool: {}\n", builder.display()));
                log.push_str(&format!("launcher: {}\n", shell));
                log.push_str("args:\n");
                for arg in args {
                    log.push_str(&format!("  {}\n", arg.to_string_lossy()));
                }
                log.push_str("stdout:\n");
                let launcher_stdout = String::from_utf8_lossy(&output.stdout);
                if !launcher_stdout.trim().is_empty() {
                    log.push_str(&launcher_stdout);
                    log.push('\n');
                }
                log.push_str(&read_text_if_exists(&stdout_path));
                log.push_str("\nstderr:\n");
                let launcher_stderr = String::from_utf8_lossy(&output.stderr);
                if !launcher_stderr.trim().is_empty() {
                    log.push_str(&launcher_stderr);
                    log.push('\n');
                }
                log.push_str(&read_text_if_exists(&stderr_path));
                log.push('\n');

                fs::write(logs_path, log).map_err(|e| {
                    format!(
                        "Failed to write provisioning logs to {}: {}",
                        logs_path.display(),
                        e
                    )
                })?;

                let _ = fs::remove_file(&stdout_path);
                let _ = fs::remove_file(&stderr_path);

                if output.status.success() {
                    return Ok(());
                }

                return Err(format!(
                    "Provisioning tool failed with exit code {:?}. Review logs: {}",
                    output.status.code(),
                    logs_path.display()
                ));
            }
            Err(e) => launch_errors.push(format!("{}: {}", shell, e)),
        }
    }

    Err(format!(
        "Failed to launch PowerShell host for ICD builder. {}",
        launch_errors.join(" | ")
    ))
}

fn run_native_builder(
    request: &PpkgRequest,
    staging_dir: &Path,
    output_ppkg_path: &Path,
    logs_path: &Path,
    builder: &Path,
) -> Result<(), String> {
    let flavor = builder_flavor(builder);
    let icd_store_file = if flavor == BuilderFlavor::Icd {
        Some(resolve_icd_store_file(builder).ok_or_else(|| {
            format!(
                "ICD store file was not found beside {}. Expected Microsoft-Desktop-Provisioning.dat (or Microsoft-Common-Provisioning.dat).",
                builder.display()
            )
        })?)
    } else {
        None
    };
    let args = build_command_arguments(
        flavor,
        staging_dir,
        output_ppkg_path,
        icd_store_file.as_deref(),
        request.signing.as_ref(),
    );

    if flavor == BuilderFlavor::Icd {
        return run_icd_builder_via_powershell(builder, &args, logs_path);
    }

    let output = Command::new(builder).args(&args).output().map_err(|e| {
        format!(
            "Failed to launch provisioning tool {}: {}",
            builder.display(),
            e
        )
    })?;

    let mut log = String::new();
    log.push_str("strategy: native-builder\n");
    log.push_str(&format!("tool: {}\n", builder.display()));
    if flavor == BuilderFlavor::Icd && request.signing.is_some() {
        log.push_str(
            "note: signing metadata ignored for ICD builder; use generic builder or fallback flow.\n",
        );
    }
    log.push_str("args:\n");
    for arg in &args {
        log.push_str(&format!("  {}\n", arg.to_string_lossy()));
    }
    log.push_str("stdout:\n");
    log.push_str(&String::from_utf8_lossy(&output.stdout));
    log.push_str("\nstderr:\n");
    log.push_str(&String::from_utf8_lossy(&output.stderr));
    log.push('\n');

    fs::write(logs_path, log).map_err(|e| {
        format!(
            "Failed to write provisioning logs to {}: {}",
            logs_path.display(),
            e
        )
    })?;

    if !output.status.success() {
        return Err(format!(
            "Provisioning tool failed with exit code {:?}. Review logs: {}",
            output.status.code(),
            logs_path.display()
        ));
    }

    Ok(())
}

fn fallback_computer_name(request: &PpkgRequest, output_ppkg_path: &Path) -> String {
    let _ = (request, output_ppkg_path);
    "BitOSDTDevice".to_string()
}

fn ps_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn provisioningtools_application_files(staging_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let content_dir = staging_dir.join("Content");
    for root in [content_dir.join("Scripts"), content_dir.join("Apps")] {
        let _ = collect_files_recursive(&root, &mut files);
    }
    files.sort();
    files.dedup();
    files
}

fn normalize_optional_field(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn build_provisioningtools_script(
    staging_dir: &Path,
    output_ppkg_path: &Path,
    computer_name: &str,
    local_admin_username: Option<&str>,
    local_admin_password: Option<&str>,
) -> String {
    let output_path = ps_single_quote(&output_ppkg_path.to_string_lossy());
    let output_dir = output_ppkg_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    let output_dir = ps_single_quote(&output_dir);
    let computer_name = ps_single_quote(computer_name);
    let local_admin_username = local_admin_username
        .map(ps_single_quote)
        .unwrap_or_else(|| "$null".to_string());
    let local_admin_password = local_admin_password
        .map(ps_single_quote)
        .unwrap_or_else(|| "$null".to_string());
    let bootstrap_script = staging_dir
        .join("Content")
        .join("Scripts")
        .join("Apply-BitOSDTProvisioning.ps1");
    let bootstrap_script = ps_single_quote(&bootstrap_script.to_string_lossy());
    let application_files = provisioningtools_application_files(staging_dir);
    let application_files = if application_files.is_empty() {
        "@()".to_string()
    } else {
        format!(
            "@({})",
            application_files
                .iter()
                .map(|path| ps_single_quote(&path.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let mut script = String::new();
    script.push_str("$ErrorActionPreference = 'Stop'\n");
    script.push_str("$ProgressPreference = 'SilentlyContinue'\n");
    script.push_str("$WarningPreference = 'Continue'\n");
    script.push_str("$VerbosePreference = 'Continue'\n");
    script.push_str("$repo = $null\n");
    script.push_str("if (Get-Command -Name Get-PSRepository -ErrorAction SilentlyContinue) {\n");
    script.push_str("  try { $repo = Get-PSRepository -Name PSGallery -ErrorAction Stop } catch { Write-Warning \"Get-PSRepository unavailable: $($_.Exception.Message)\" }\n");
    script.push_str("}\n");
    script.push_str("if ($repo -and $repo.InstallationPolicy -ne 'Trusted' -and (Get-Command -Name Set-PSRepository -ErrorAction SilentlyContinue)) {\n");
    script.push_str("  try { Set-PSRepository -Name PSGallery -InstallationPolicy Trusted -ErrorAction Stop } catch { Write-Warning \"Failed to trust PSGallery: $($_.Exception.Message)\" }\n");
    script.push_str("}\n");
    script.push_str("if (Get-Command -Name Import-Module -ErrorAction SilentlyContinue) {\n");
    script.push_str("  try { Import-Module PackageManagement -ErrorAction Stop | Out-Null } catch { Write-Warning \"PackageManagement import failed: $($_.Exception.Message)\" }\n");
    script.push_str("}\n");
    script.push_str("$nugetProviderAvailable = $false\n");
    script.push_str("if (Get-Command -Name Get-PackageProvider -ErrorAction SilentlyContinue) {\n");
    script.push_str("  try { $nugetProviderAvailable = $null -ne (Get-PackageProvider -Name NuGet -ListAvailable -ErrorAction Stop) } catch { Write-Warning \"NuGet provider probe failed: $($_.Exception.Message)\" }\n");
    script.push_str("}\n");
    script.push_str("if (-not $nugetProviderAvailable -and (Get-Command -Name Install-PackageProvider -ErrorAction SilentlyContinue)) {\n");
    script.push_str("  try {\n");
    script.push_str(
        "    Install-PackageProvider -Name NuGet -MinimumVersion 2.8.5.201 -Scope CurrentUser -Force -ErrorAction Stop | Out-Null\n",
    );
    script.push_str("  } catch { Write-Warning \"NuGet provider bootstrap failed: $($_.Exception.Message)\" }\n");
    script.push_str("}\n");
    script.push_str(&format!(
        "if (-not (Get-Module -ListAvailable -Name {})) {{\n",
        PROVISIONING_TOOLS_MODULE
    ));
    script.push_str("  $installed = $false\n");
    script.push_str("  if (Get-Command -Name Install-Module -ErrorAction SilentlyContinue) {\n");
    script.push_str("    try {\n");
    script.push_str(&format!(
        "      Install-Module -Name {} -Scope CurrentUser -Force -AllowClobber -ErrorAction Stop\n",
        PROVISIONING_TOOLS_MODULE
    ));
    script.push_str("      $installed = $true\n");
    script.push_str(
        "    } catch { Write-Warning \"Install-Module failed: $($_.Exception.Message)\" }\n",
    );
    script.push_str("  }\n");
    script.push_str("  if (-not $installed -and (Get-Command -Name Install-PSResource -ErrorAction SilentlyContinue)) {\n");
    script.push_str("    try {\n");
    script.push_str(&format!(
        "      Install-PSResource -Name {} -Scope CurrentUser -TrustRepository -Reinstall -Quiet -ErrorAction Stop\n",
        PROVISIONING_TOOLS_MODULE
    ));
    script.push_str("      $installed = $true\n");
    script.push_str(
        "    } catch { Write-Warning \"Install-PSResource failed: $($_.Exception.Message)\" }\n",
    );
    script.push_str("  }\n");
    script.push_str(&format!(
        "  if (-not $installed) {{ throw 'Unable to install {} via Install-Module or Install-PSResource.' }}\n",
        PROVISIONING_TOOLS_MODULE
    ));
    script.push_str("}\n");
    script.push_str(&format!(
        "Import-Module {} -Force -ErrorAction Stop\n",
        PROVISIONING_TOOLS_MODULE
    ));
    script.push_str("$cmd = Get-Command -Name New-ProvisioningPackage -ErrorAction Stop\n");
    script.push_str(&format!("$outputPath = {}\n", output_path));
    script.push_str(&format!("$outputDir = {}\n", output_dir));
    script.push_str(
        "if ([string]::IsNullOrWhiteSpace($outputDir)) { $outputDir = (Get-Location).Path }\n",
    );
    script.push_str("New-Item -Path $outputDir -ItemType Directory -Force | Out-Null\n");
    script.push_str("if (Test-Path -LiteralPath $outputPath) { Remove-Item -LiteralPath $outputPath -Force -ErrorAction Stop }\n");
    script.push_str(&format!("$computerName = {}\n", computer_name));
    script.push_str(
        "if ([string]::IsNullOrWhiteSpace($computerName)) { $computerName = 'BitOSDTDevice' }\n",
    );
    script.push_str(&format!("$localAdminUsername = {}\n", local_admin_username));
    script.push_str(&format!("$localAdminPassword = {}\n", local_admin_password));
    script.push_str("$before = @()\n");
    script.push_str("if (Test-Path -LiteralPath $outputDir) {\n");
    script.push_str("  $before = Get-ChildItem -LiteralPath $outputDir -Filter *.ppkg -File -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName\n");
    script.push_str("}\n");
    script.push_str("$params = @{ ComputerName = $computerName }\n");
    script.push_str("if ($cmd.Parameters.ContainsKey('Force')) { $params['Force'] = $true }\n");
    script.push_str("$requiresLocalAdminCredential = $false\n");
    script.push_str("if ($cmd.Parameters.ContainsKey('LocalAdminCredential')) {\n");
    script.push_str(
        "  foreach ($paramAttr in $cmd.Parameters['LocalAdminCredential'].Attributes) {\n",
    );
    script.push_str("    if ($paramAttr -is [System.Management.Automation.ParameterAttribute] -and $paramAttr.Mandatory) {\n");
    script.push_str("      $requiresLocalAdminCredential = $true\n");
    script.push_str("      break\n");
    script.push_str("    }\n");
    script.push_str("  }\n");
    script.push_str("}\n");
    script.push_str("if ($cmd.Parameters.ContainsKey('Path')) {\n");
    script.push_str("  $params['Path'] = $outputDir\n");
    script.push_str("} elseif ($cmd.Parameters.ContainsKey('OutputPath')) {\n");
    script.push_str("  $params['OutputPath'] = $outputPath\n");
    script.push_str("} elseif ($cmd.Parameters.ContainsKey('PackagePath')) {\n");
    script.push_str("  $params['PackagePath'] = $outputPath\n");
    script.push_str("} else {\n");
    script.push_str(
        "  throw 'New-ProvisioningPackage does not expose Path/OutputPath/PackagePath parameter.'\n",
    );
    script.push_str("}\n");
    script.push_str(&format!("$bootstrapScript = {}\n", bootstrap_script));
    script.push_str(&format!("$applicationFiles = {}\n", application_files));
    script.push_str("if (Test-Path -LiteralPath $bootstrapScript) {\n");
    script.push_str("  if ($cmd.Parameters.ContainsKey('Application')) {\n");
    script.push_str(
        "    if (-not $applicationFiles -or $applicationFiles.Count -eq 0) { $applicationFiles = @($bootstrapScript) }\n",
    );
    script.push_str("    $params['Application'] = @{ Path = $applicationFiles; Command = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"Apply-BitOSDTProvisioning.ps1\"' }\n");
    script.push_str("  } elseif ($cmd.Parameters.ContainsKey('Script')) {\n");
    script.push_str("    $params['Script'] = $bootstrapScript\n");
    script.push_str("  }\n");
    script.push_str("}\n");
    script.push_str("$hasLocalAdminCredential = -not [string]::IsNullOrWhiteSpace($localAdminUsername) -and -not [string]::IsNullOrWhiteSpace($localAdminPassword)\n");
    script.push_str("if ($cmd.Parameters.ContainsKey('LocalAdminCredential') -and $hasLocalAdminCredential) {\n");
    script.push_str("  $secureLocalAdminPassword = ConvertTo-SecureString $localAdminPassword -AsPlainText -Force\n");
    script.push_str("  $params['LocalAdminCredential'] = [pscredential]::new($localAdminUsername, $secureLocalAdminPassword)\n");
    script.push_str("} elseif ($requiresLocalAdminCredential) {\n");
    script.push_str("  throw 'New-ProvisioningPackage requires LocalAdminCredential. Supply localAdminUsername and localAdminPassword in the generate_oobe_ppkg request, or configure ICD/WCD via builderPath.'\n");
    script.push_str("}\n");
    script.push_str("& $cmd @params | Out-Null\n");
    script.push_str("if (-not (Test-Path -LiteralPath $outputPath)) {\n");
    script.push_str(
        "  $candidate = Join-Path -Path $outputDir -ChildPath ($computerName + '.ppkg')\n",
    );
    script.push_str("  if (Test-Path -LiteralPath $candidate) {\n");
    script.push_str("    Move-Item -LiteralPath $candidate -Destination $outputPath -Force\n");
    script.push_str("  } else {\n");
    script.push_str("    $after = Get-ChildItem -LiteralPath $outputDir -Filter *.ppkg -File -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName\n");
    script.push_str("    $newFiles = @($after | Where-Object { $before -notcontains $_ })\n");
    script.push_str("    if ($newFiles.Count -eq 1) {\n");
    script.push_str("      Move-Item -LiteralPath $newFiles[0] -Destination $outputPath -Force\n");
    script.push_str("    }\n");
    script.push_str("  }\n");
    script.push_str("}\n");
    script.push_str("if (-not (Test-Path -LiteralPath $outputPath)) {\n");
    script.push_str("  throw \"ProvisioningTools completed but did not produce expected output path: $outputPath\"\n");
    script.push_str("}\n");
    script
}

fn run_provisioningtools_fallback(
    request: &PpkgRequest,
    staging_dir: &Path,
    output_ppkg_path: &Path,
    logs_path: &Path,
) -> Result<(), String> {
    if request.signing.is_some() {
        let _ = append_log(
            logs_path,
            "strategy: provisioningtools-fallback\nnote: signing metadata was supplied, but fallback mode does not support signing options.\n",
        );
        return Err(format!(
            "{} Fallback via {} does not support signing metadata; install ICD/WCD or provide builderPath. Review logs: {}",
            MISSING_TOOLING_MESSAGE,
            PROVISIONING_TOOLS_MODULE,
            logs_path.display()
        ));
    }

    let computer_name = fallback_computer_name(request, output_ppkg_path);
    let local_admin_username = normalize_optional_field(request.local_admin_username.as_deref());
    let local_admin_password = normalize_optional_field(request.local_admin_password.as_deref());
    let script = build_provisioningtools_script(
        staging_dir,
        output_ppkg_path,
        &computer_name,
        local_admin_username.as_deref(),
        local_admin_password.as_deref(),
    );
    let shells = ["pwsh.exe", "powershell.exe"];

    let mut attempted = false;
    let mut launch_errors = Vec::new();

    append_log(
        logs_path,
        "strategy: provisioningtools-fallback\nnote: fallback path installs a community PowerShell module from PSGallery.\n",
    )?;

    for shell in shells {
        let args = [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ];
        match Command::new(shell).args(args).output() {
            Ok(output) => {
                attempted = true;
                let mut section = String::new();
                section.push_str(&format!("tool: {}\n", shell));
                section.push_str("args:\n");
                section.push_str("  -NoProfile\n  -NonInteractive\n  -ExecutionPolicy\n  Bypass\n  -Command\n  <ProvisioningTools script>\n");
                section.push_str("stdout:\n");
                section.push_str(&String::from_utf8_lossy(&output.stdout));
                section.push_str("\nstderr:\n");
                section.push_str(&String::from_utf8_lossy(&output.stderr));
                section.push('\n');
                append_log(logs_path, &section)?;

                if output.status.success() {
                    return Ok(());
                }
            }
            Err(e) => {
                launch_errors.push(format!("{}: {}", shell, e));
            }
        }
    }

    if !attempted {
        append_log(
            logs_path,
            &format!(
                "Failed to launch PowerShell fallback shells.\n{}\n",
                launch_errors.join("\n")
            ),
        )?;
        return Err(format!(
            "{} Fallback via {} failed to launch PowerShell shell(s). Review logs: {}",
            MISSING_TOOLING_MESSAGE,
            PROVISIONING_TOOLS_MODULE,
            logs_path.display()
        ));
    }

    Err(format!(
        "{} Attempted fallback via {} but package build failed (for example, module bootstrap errors or missing required LocalAdminCredential). Review logs: {}",
        MISSING_TOOLING_MESSAGE,
        PROVISIONING_TOOLS_MODULE,
        logs_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("bitosdt-ppkg-{}-{}", prefix, Uuid::new_v4()))
    }

    fn base_profile_request() -> OobeProfileRequest {
        OobeProfileRequest::default()
    }

    #[test]
    fn validate_request_requires_profile_reference() {
        let request = PpkgRequest {
            profile_name: None,
            profile_path: None,
            output_ppkg_path: "C:/out/profile.ppkg".to_string(),
            builder_path: None,
            owner: None,
            rank: None,
            version: None,
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        let err = validate_request(&request).expect_err("missing profile reference should fail");
        assert!(err.contains("profileName or profilePath"));
    }

    #[test]
    fn validate_request_requires_ppkg_extension() {
        let request = PpkgRequest {
            profile_name: Some("MyProfile".to_string()),
            profile_path: None,
            output_ppkg_path: "C:/out/profile.zip".to_string(),
            builder_path: None,
            owner: None,
            rank: None,
            version: None,
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        let err = validate_request(&request).expect_err("bad extension should fail");
        assert!(err.contains(".ppkg"));
    }

    #[test]
    fn validate_request_allows_wrapped_output_path_quotes() {
        let request = PpkgRequest {
            profile_name: Some("Sample".to_string()),
            profile_path: Some("C:/profiles/Sample".to_string()),
            output_ppkg_path: "\"C:/out/profile.ppkg\"".to_string(),
            builder_path: None,
            owner: None,
            rank: None,
            version: None,
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        let err = validate_request(&request).expect_err("profile path should fail in test context");
        assert!(err.contains("Profile path does not exist"));
    }

    #[test]
    fn validate_request_rejects_embedded_quote_in_output_path() {
        let request = PpkgRequest {
            profile_name: Some("MyProfile".to_string()),
            profile_path: None,
            output_ppkg_path: "D:\\\"bad.ppkg".to_string(),
            builder_path: None,
            owner: None,
            rank: None,
            version: None,
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        let err = validate_request(&request).expect_err("embedded quote should fail");
        assert!(err.contains("invalid quote character"));
    }

    #[test]
    fn rewrite_staged_bootstrap_script_uses_expected_output_file_name() {
        let root = make_temp_dir("bootstrap-rewrite");
        let scripts_dir = root.join("Content").join("Scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::write(
            scripts_dir.join(PROVISIONING_BOOTSTRAP_SCRIPT),
            "placeholder",
        )
        .unwrap();

        rewrite_staged_bootstrap_script(
            &root.join("Content"),
            Path::new("D:/USB/BranchOffice.ppkg"),
            true,
        )
        .unwrap();

        let bootstrap =
            fs::read_to_string(scripts_dir.join(PROVISIONING_BOOTSTRAP_SCRIPT)).unwrap();
        assert!(bootstrap.contains("BranchOffice.ppkg"));
        assert!(bootstrap.contains("Resolve-BitOSDTProvisioningMediaRoot"));
        assert!(bootstrap.contains("DisablePrivacyExperience"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stage_profile_assets_copies_apps_and_files_for_request_derived_provisioning() {
        let profile_root = make_temp_dir("stage-assets");
        let content_root = make_temp_dir("stage-assets-content");
        fs::create_dir_all(profile_root.join("Scripts")).unwrap();
        fs::create_dir_all(profile_root.join("Apps")).unwrap();
        fs::create_dir_all(profile_root.join("Files").join("Payload")).unwrap();
        fs::write(
            profile_root.join("Scripts").join("script.ps1"),
            "Write-Host hi",
        )
        .unwrap();
        fs::write(profile_root.join("Apps").join("setup.exe"), "stub").unwrap();
        fs::write(
            profile_root
                .join("Files")
                .join("Payload")
                .join("answer.txt"),
            "42",
        )
        .unwrap();

        let mut warnings = Vec::new();
        stage_profile_assets(&profile_root, &content_root, &mut warnings).unwrap();

        assert!(content_root.join("Apps").join("setup.exe").is_file());
        assert!(content_root
            .join("Files")
            .join("Payload")
            .join("answer.txt")
            .is_file());
        assert!(!content_root.join("Scripts").join("script.ps1").exists());
        assert!(warnings
            .iter()
            .any(|warning| warning.contains(AUTOUNATTEND_FILE)));

        let _ = fs::remove_dir_all(profile_root);
        let _ = fs::remove_dir_all(content_root);
    }

    #[test]
    fn request_derived_provisioning_assets_are_generated_for_first_logon_profiles() {
        let profile_root = make_temp_dir("firstlogon-profile");
        let content_root = make_temp_dir("firstlogon-content");
        fs::create_dir_all(profile_root.join("Scripts")).unwrap();
        fs::create_dir_all(profile_root.join("Apps")).unwrap();
        fs::create_dir_all(profile_root.join("Files").join("Payload")).unwrap();
        fs::write(
            profile_root
                .join("Scripts")
                .join("Start-BitOSDTUsbOrchestrator.ps1"),
            "usb orchestrator only",
        )
        .unwrap();
        fs::write(profile_root.join("Apps").join("setup.exe"), "stub").unwrap();
        fs::write(
            profile_root
                .join("Files")
                .join("Payload")
                .join("answer.txt"),
            "42",
        )
        .unwrap();

        let mut warnings = Vec::new();
        stage_profile_assets(&profile_root, &content_root, &mut warnings).unwrap();

        let mut request = base_profile_request();
        request.name = "DualArtifact".to_string();
        request.enable_debloat = true;
        request.apps.enable_custom_scripts = true;
        request.apps.custom_scripts = vec![oobe_profiles::OobeCustomScript {
            name: "Harden".to_string(),
            content: "Write-Host 'custom'".to_string(),
            enabled: true,
            continue_on_error: true,
        }];
        request.apps.custom_installers = vec![oobe_profiles::OobeCustomInstaller {
            name: "Tool".to_string(),
            path: r"C:\payloads\tool.msi".to_string(),
            source_type: Some("EmbeddedFile".to_string()),
            source_file_name: None,
            dependencies: vec![],
            dependency_destination: None,
            silent_args: "/qn".to_string(),
            installer_type: "Msi".to_string(),
            enabled: true,
        }];
        request.domain_join.enabled = true;
        request.domain_join.domain = "contoso.local".to_string();
        request.domain_join.username = "CONTOSO\\join".to_string();
        request.domain_join.password = "Secret123!".to_string();
        request.domain_join.ou_path = Some("OU=Devices,DC=contoso,DC=local".to_string());
        request.wifi.enabled = true;
        request.wifi.ssid = "CorpWiFi".to_string();
        request.wifi.password = "WirelessP@ss123".to_string();
        request.wifi.authentication = "Wpa2Psk".to_string();
        request.wifi.encryption = "Aes".to_string();
        request.wifi.hidden_network = true;
        request.wifi.dns_server_1 = "10.0.0.10".to_string();
        request.wifi.dns_server_2 = "10.0.0.11".to_string();

        oobe_profiles::materialize_request_derived_provisioning_payload(&request, &content_root)
            .unwrap();

        assert!(content_root.join(PROVISIONING_BOOTSTRAP_SCRIPT).is_file());
        assert!(content_root
            .join("Scripts")
            .join("Start-BitOSDTOrchestrator.ps1")
            .is_file());
        assert!(content_root
            .join("Scripts")
            .join("Start-BitOSDTProvisioningUi.hta")
            .is_file());
        assert!(content_root
            .join("Scripts")
            .join("ProvisioningUiProfile.json")
            .is_file());
        assert!(content_root
            .join("Scripts")
            .join("installapps.ps1")
            .is_file());
        assert!(content_root
            .join("Scripts")
            .join("domainjoin.ps1")
            .is_file());
        assert!(content_root
            .join("Scripts")
            .join("wifi-connect.ps1")
            .is_file());
        assert!(content_root
            .join("Scripts")
            .join("custom-01-Harden.ps1")
            .is_file());
        assert!(content_root.join("Scripts").join("debloat.ps1").is_file());
        assert!(content_root.join(PPKG_README_FILE).is_file());
        assert!(!content_root
            .join("Scripts")
            .join("Start-BitOSDTUsbOrchestrator.ps1")
            .exists());

        let install_script =
            fs::read_to_string(content_root.join("Scripts").join("installapps.ps1")).unwrap();
        assert!(install_script.contains(r#"$ProvisioningProgressPath = "C:\ProgramData\BitOSDT\ProvisioningUi\app-progress.json""#));

        let _ = fs::remove_dir_all(profile_root);
        let _ = fs::remove_dir_all(content_root);
    }

    #[test]
    fn export_sidecar_assets_writes_scripts_apps_files_and_readme_beside_ppkg() {
        let profile_root = make_temp_dir("profile");
        let output_root = make_temp_dir("output");
        fs::create_dir_all(profile_root.join("Scripts")).unwrap();
        fs::create_dir_all(profile_root.join("Apps").join("Vendor")).unwrap();
        fs::create_dir_all(profile_root.join("Files").join("Payload")).unwrap();
        fs::write(
            profile_root.join("Scripts").join("script.ps1"),
            "Write-Host hi",
        )
        .unwrap();
        fs::write(
            profile_root.join("Apps").join("Vendor").join("setup.exe"),
            "stub",
        )
        .unwrap();
        fs::write(
            profile_root
                .join("Files")
                .join("Payload")
                .join("answer.txt"),
            "42",
        )
        .unwrap();
        fs::write(profile_root.join(PPKG_README_FILE), "readme").unwrap();
        fs::create_dir_all(output_root.join("Scripts")).unwrap();
        fs::write(output_root.join("Scripts").join("stale.ps1"), "stale").unwrap();
        fs::create_dir_all(output_root.join("Files")).unwrap();
        fs::write(output_root.join("Files").join("stale.txt"), "stale").unwrap();

        let mut warnings = Vec::new();
        export_sidecar_assets(
            &profile_root,
            &output_root.join("Profile.ppkg"),
            false,
            &mut warnings,
        )
        .unwrap();

        assert!(output_root.join("Scripts").join("script.ps1").is_file());
        assert!(output_root
            .join("Apps")
            .join("Vendor")
            .join("setup.exe")
            .is_file());
        assert!(output_root
            .join("Files")
            .join("Payload")
            .join("answer.txt")
            .is_file());
        assert!(output_root.join(PPKG_README_FILE).is_file());
        assert!(!output_root.join("Scripts").join("stale.ps1").exists());
        assert!(!output_root.join("Files").join("stale.txt").exists());
        assert!(warnings.is_empty());

        let _ = fs::remove_dir_all(profile_root);
        let _ = fs::remove_dir_all(output_root);
    }

    #[test]
    fn export_sidecar_assets_merges_when_output_is_profile_dir() {
        let sidecar_root = make_temp_dir("profile-inline-sidecar");
        let profile_root = make_temp_dir("profile-inline-output");
        fs::create_dir_all(sidecar_root.join("Scripts")).unwrap();
        fs::create_dir_all(sidecar_root.join("Apps")).unwrap();
        fs::create_dir_all(sidecar_root.join("Files")).unwrap();
        fs::write(
            sidecar_root.join("Scripts").join("script.ps1"),
            "Write-Host hi",
        )
        .unwrap();
        fs::write(sidecar_root.join("Apps").join("setup.exe"), "stub").unwrap();
        fs::write(sidecar_root.join("Files").join("answer.txt"), "42").unwrap();
        fs::write(sidecar_root.join(PPKG_README_FILE), "readme").unwrap();

        fs::create_dir_all(profile_root.join("Scripts")).unwrap();
        fs::write(
            profile_root
                .join("Scripts")
                .join("Start-BitOSDTUsbOrchestrator.ps1"),
            "usb orchestrator",
        )
        .unwrap();
        fs::write(profile_root.join(AUTOUNATTEND_FILE), "<xml />").unwrap();

        let mut warnings = Vec::new();
        export_sidecar_assets(
            &sidecar_root,
            &profile_root.join("Profile.ppkg"),
            true,
            &mut warnings,
        )
        .unwrap();

        assert!(profile_root.join("Scripts").join("script.ps1").is_file());
        assert!(profile_root
            .join("Scripts")
            .join("Start-BitOSDTUsbOrchestrator.ps1")
            .is_file());
        assert!(profile_root.join("Apps").join("setup.exe").is_file());
        assert!(profile_root.join("Files").join("answer.txt").is_file());
        assert!(profile_root.join(PPKG_README_FILE).is_file());
        assert!(profile_root.join(AUTOUNATTEND_FILE).is_file());
        assert!(warnings.is_empty());

        let _ = fs::remove_dir_all(sidecar_root);
        let _ = fs::remove_dir_all(profile_root);
    }

    #[test]
    fn builds_icd_arguments_with_signing_metadata() {
        let args = build_command_arguments(
            BuilderFlavor::Icd,
            Path::new("C:/temp/staging root"),
            Path::new("C:/temp/output root/out.ppkg"),
            Some(Path::new(
                "C:/Program Files (x86)/Windows Kits/Microsoft-Desktop-Provisioning.dat",
            )),
            Some(&PpkgSigningMetadata {
                pfx_path: "C:/keys/code-sign.pfx".to_string(),
                password: Some("secret".to_string()),
                timestamp_url: Some("http://timestamp.local".to_string()),
            }),
        );

        let rendered: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "/Build-ProvisioningPackage",
                "/CustomizationXML:\"C:/temp/staging root\\Customization.xml\"",
                "/PackagePath:\"C:/temp/output root/out.ppkg\"",
                "/StoreFile:\"C:/Program Files (x86)/Windows Kits/Microsoft-Desktop-Provisioning.dat\"",
                "+Overwrite",
            ]
        );
    }

    #[test]
    fn builds_generic_arguments_without_signing_metadata() {
        let args = build_command_arguments(
            BuilderFlavor::Generic,
            Path::new("C:/temp/staging"),
            Path::new("C:/temp/out.ppkg"),
            None,
            None,
        );

        let rendered: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "build",
                "--project",
                "C:/temp/staging",
                "--output",
                "C:/temp/out.ppkg",
            ]
        );
    }

    #[test]
    fn fallback_computer_name_uses_neutral_default() {
        let request = PpkgRequest {
            profile_name: Some("Branch-01".to_string()),
            profile_path: None,
            output_ppkg_path: "C:/out/ignored.ppkg".to_string(),
            builder_path: None,
            owner: None,
            rank: None,
            version: None,
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        let value = fallback_computer_name(&request, Path::new("C:/out/ignored.ppkg"));
        assert_eq!(value, "BitOSDTDevice");
    }

    #[test]
    fn provisioningtools_script_installs_module_and_builds_package() {
        let root = make_temp_dir("provisioningtools-script");
        let scripts_dir = root.join("Content").join("Scripts");
        let apps_dir = root.join("Content").join("Apps");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::create_dir_all(&apps_dir).unwrap();
        fs::write(scripts_dir.join(PROVISIONING_BOOTSTRAP_SCRIPT), "bootstrap").unwrap();
        fs::write(
            scripts_dir.join("Start-BitOSDTOrchestrator.ps1"),
            "orchestrator",
        )
        .unwrap();
        fs::write(apps_dir.join("setup.exe"), "stub").unwrap();

        let script = build_provisioningtools_script(
            &root,
            Path::new("C:/temp/BranchOffice.ppkg"),
            "BranchOffice",
            Some("localadmin"),
            Some("Password123!"),
        );
        assert!(script.contains("Import-Module PackageManagement"));
        assert!(script.contains("Install-Module -Name ProvisioningTools"));
        assert!(script.contains("Get-Command -Name New-ProvisioningPackage"));
        assert!(script.contains("ComputerName = $computerName"));
        assert!(script.contains("LocalAdminCredential"));
        assert!(script.contains("requires LocalAdminCredential"));
        assert!(script.contains(&format!(
            "$bootstrapScript = {}",
            ps_single_quote(
                &root
                    .join("Content")
                    .join("Scripts")
                    .join(PROVISIONING_BOOTSTRAP_SCRIPT)
                    .to_string_lossy()
            )
        )));
        assert!(script.contains("$applicationFiles = @("));
        assert!(script.contains("Start-BitOSDTOrchestrator.ps1"));
        assert!(script.contains("setup.exe"));
        assert!(script.contains(
            "Command = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"Apply-BitOSDTProvisioning.ps1\"'"
        ));
        assert!(script.contains("Move-Item -LiteralPath"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn writes_icd_customization_xml_with_expected_metadata() {
        let root = std::env::temp_dir().join(format!("bitosdt-icd-xml-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let request = PpkgRequest {
            profile_name: Some("ProfileOne".to_string()),
            profile_path: None,
            output_ppkg_path: "C:/out/profile.ppkg".to_string(),
            builder_path: None,
            owner: Some("itadmin".to_string()),
            rank: Some(7),
            version: Some("2.1.0.0".to_string()),
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        write_icd_customization_xml(&root, "ProfileOne", &request, &base_profile_request())
            .unwrap();
        let xml = fs::read_to_string(icd_customization_path(&root)).unwrap();
        assert!(xml.contains("<Name>ProfileOne</Name>"));
        assert!(xml.contains("<OwnerType>ITAdmin</OwnerType>"));
        assert!(xml.contains("<Version>2.1.0.0</Version>"));
        assert!(xml.contains("<Rank>7</Rank>"));
        assert!(xml.contains("<CommandLine>cmd /c exit /b 0</CommandLine>"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn writes_icd_customization_xml_with_bootstrap_command_and_dependencies() {
        let root = std::env::temp_dir().join(format!("bitosdt-icd-bootstrap-{}", Uuid::new_v4()));
        let scripts_dir = root.join("Content").join("Scripts");
        let apps_dir = root.join("Content").join("Apps");
        let files_dir = root.join("Content").join("Files");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::create_dir_all(&apps_dir).unwrap();
        fs::create_dir_all(&files_dir).unwrap();
        fs::write(
            scripts_dir.join(PROVISIONING_BOOTSTRAP_SCRIPT),
            "Write-Host 'bootstrap'",
        )
        .unwrap();
        fs::write(scripts_dir.join("custom-01.ps1"), "Write-Host 'custom'").unwrap();
        fs::write(apps_dir.join("setup.msi"), "stub").unwrap();
        fs::write(files_dir.join("answer.txt"), "42").unwrap();

        let request = PpkgRequest {
            profile_name: Some("ProfileTwo".to_string()),
            profile_path: None,
            output_ppkg_path: "C:/out/profile.ppkg".to_string(),
            builder_path: None,
            owner: Some("itadmin".to_string()),
            rank: Some(1),
            version: Some("1.0.0.0".to_string()),
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        write_icd_customization_xml(&root, "ProfileTwo", &request, &base_profile_request())
            .unwrap();
        let xml = fs::read_to_string(icd_customization_path(&root)).unwrap();
        assert!(xml.contains("<CommandConfig Name=\"BitOSDTBootstrap\">"));
        assert!(xml.contains("<CommandFile>"));
        assert!(xml.contains(PROVISIONING_BOOTSTRAP_SCRIPT));
        assert!(xml.contains("<DependencyPackages>"));
        assert!(xml.contains("custom-01.ps1"));
        assert!(xml.contains("setup.msi"));
        assert!(xml.contains("answer.txt"));
        assert!(xml.contains("powershell.exe -NoProfile -ExecutionPolicy Bypass -File"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn writes_icd_customization_xml_with_native_account_wifi_and_domain_settings() {
        let root = std::env::temp_dir().join(format!("bitosdt-icd-native-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("Content").join("Scripts")).unwrap();

        let request = PpkgRequest {
            profile_name: Some("NativeProfile".to_string()),
            profile_path: None,
            output_ppkg_path: "C:/out/native.ppkg".to_string(),
            builder_path: None,
            owner: Some("itadmin".to_string()),
            rank: Some(0),
            version: Some("1.0.0.0".to_string()),
            signing: None,
            local_admin_username: None,
            local_admin_password: None,
        };

        let mut profile_request = base_profile_request();
        profile_request.oobe_config.computer_name = Some("BRANCH-01".to_string());
        profile_request.default_user.enabled = true;
        profile_request.default_user.username = "source".to_string();
        profile_request.default_user.password = "Password123!".to_string();
        profile_request.default_user.group = "Administrators".to_string();
        profile_request.domain_join.enabled = true;
        profile_request.domain_join.domain = "contoso.local".to_string();
        profile_request.domain_join.username = "CONTOSO\\joiner".to_string();
        profile_request.domain_join.password = "Passw0rd!".to_string();
        profile_request.wifi.enabled = true;
        profile_request.wifi.ssid = "CorpWifi".to_string();
        profile_request.wifi.authentication = "Wpa2Psk".to_string();
        profile_request.wifi.password = "WifiPass123!".to_string();
        profile_request.oobe_config.skip_machine_oobe = true;
        profile_request.oobe_config.skip_user_oobe = true;

        write_icd_customization_xml(&root, "NativeProfile", &request, &profile_request).unwrap();
        let xml = fs::read_to_string(icd_customization_path(&root)).unwrap();
        assert!(xml.contains("<Accounts>"));
        assert!(xml.contains("<ComputerName>BRANCH-01</ComputerName>"));
        assert!(xml.contains("<DomainName>contoso.local</DomainName>"));
        assert!(xml.contains("<Account>CONTOSO\\joiner</Account>"));
        assert!(xml.contains("<Users>"));
        assert!(xml.contains("<User UserName=\"source\">"));
        assert!(xml.contains("<UserGroup>Administrators</UserGroup>"));
        assert!(xml.contains("<HideOobe>True</HideOobe>"));
        assert!(xml.contains("<AllowAllTrustedApps>Yes</AllowAllTrustedApps>"));
        assert!(xml.contains("<WLANConfig SSID=\"CorpWifi\">"));
        assert!(xml.contains("<SecurityType>WPA2-Personal</SecurityType>"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collect_support_warnings_flags_post_sign_in_fallbacks() {
        let mut request = base_profile_request();
        request.prompt_for_computer_name = true;
        request.domain_join.enabled = true;
        request.domain_join.domain = "contoso.local".to_string();
        request.domain_join.username = "CONTOSO\\joiner".to_string();
        request.domain_join.password = "Passw0rd!".to_string();
        request.domain_join.ou_path = Some("OU=Computers,DC=contoso,DC=local".to_string());
        request.wifi.enabled = true;
        request.wifi.ssid = "HiddenWifi".to_string();
        request.wifi.authentication = "Wpa3Sae".to_string();
        request.wifi.hidden_network = true;
        request
            .apps
            .copied_items
            .push(oobe_profiles::OobeLocalPayloadItem {
                source_path: "C:\\payload.txt".to_string(),
                source_kind: "File".to_string(),
                display_name: None,
            });
        request.apps.disable_bitlocker = true;

        let warnings = collect_support_warnings(&request);
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("computer naming")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("domain join")));
        assert!(warnings.iter().any(|warning| warning.contains("Wi-Fi")));
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("BitLocker disable, applications, copied payloads")));
    }

    #[test]
    fn map_payload_includes_bitlocker_settings() {
        let mut request = base_profile_request();
        request.apps.disable_bitlocker = true;
        request.apps.reboot_after_disable_bitlocker = true;

        let payload = map_payload(
            "ProfileOne",
            Path::new(r"C:\Profiles\ProfileOne"),
            &request,
            &PpkgRequest {
                profile_name: Some("ProfileOne".to_string()),
                profile_path: None,
                output_ppkg_path: r"C:\Output\ProfileOne.ppkg".to_string(),
                builder_path: None,
                owner: None,
                rank: None,
                version: None,
                signing: None,
                local_admin_username: None,
                local_admin_password: None,
            },
        );

        assert!(payload.settings.disable_bitlocker);
        assert!(payload.settings.reboot_after_disable_bitlocker);
    }

    #[test]
    fn ppkg_capability_status_requires_local_admin_without_native_icd() {
        let status =
            get_ppkg_capability_status(Some("C:/definitely-missing/builder.exe".to_string()));
        assert!(!status.native_builder_available);
        assert!(status.local_admin_credentials_required);
    }
}
