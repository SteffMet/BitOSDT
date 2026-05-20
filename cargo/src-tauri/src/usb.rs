use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

use crate::{oobe_profiles, ppkg};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbTarget {
    pub disk_number: u32,
    pub friendly_name: String,
    pub size_bytes: u64,
    pub bus_type: String,
    pub drive_letters: Vec<String>,
    pub is_system: bool,
    pub is_boot: bool,
    pub is_read_only: bool,
    pub confirmation_phrase: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteIsoToUsbRequest {
    pub iso_path: String,
    pub target_disk_number: u32,
    pub confirmation_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProvisioningBundleRequest {
    pub profile_name: String,
    pub target_disk_number: u32,
    pub confirmation_token: String,
    pub local_admin_username: Option<String>,
    pub local_admin_password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellUsbTarget {
    disk_number: u32,
    friendly_name: Option<String>,
    size_bytes: Option<u64>,
    bus_type: Option<String>,
    drive_letters: Option<Vec<String>>,
    is_system: Option<bool>,
    is_boot: Option<bool>,
    is_read_only: Option<bool>,
}

#[cfg(not(target_os = "windows"))]
pub fn list_usb_targets() -> Result<Vec<UsbTarget>, String> {
    Err("USB write operations are currently supported only on Windows.".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn write_iso_to_usb(_request: WriteIsoToUsbRequest) -> Result<String, String> {
    Err("USB write operations are currently supported only on Windows.".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn write_provisioning_bundle_to_usb(
    _request: WriteProvisioningBundleRequest,
) -> Result<String, String> {
    Err("USB write operations are currently supported only on Windows.".to_string())
}

#[cfg(target_os = "windows")]
pub fn list_usb_targets() -> Result<Vec<UsbTarget>, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
$targets = @()
Get-Disk | ForEach-Object {
    $disk = $_
    $bus = [string]$disk.BusType
    $isExternal = $bus -in @('USB', 'SD', 'MMC')
    if (-not $isExternal) {
        return
    }

    $letters = @()
    try {
        $letters = Get-Partition -DiskNumber $disk.Number -ErrorAction SilentlyContinue |
            Where-Object { $_.DriveLetter } |
            ForEach-Object { "$($_.DriveLetter):" }
    } catch {
    }

    $targets += [pscustomobject]@{
        diskNumber = [uint32]$disk.Number
        friendlyName = [string]$disk.FriendlyName
        sizeBytes = [uint64]$disk.Size
        busType = $bus
        driveLetters = @($letters)
        isSystem = [bool]$disk.IsSystem
        isBoot = [bool]$disk.IsBoot
        isReadOnly = [bool]$disk.IsReadOnly
    }
}
$targets | ConvertTo-Json -Depth 5 -Compress
"#;

    let output = run_powershell(script)?;
    if output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let parsed: serde_json::Value = serde_json::from_str(&output)
        .map_err(|e| format!("Failed to parse USB device list JSON: {}", e))?;
    let rows: Vec<PowerShellUsbTarget> = if parsed.is_array() {
        serde_json::from_value(parsed).map_err(|e| format!("Invalid USB target shape: {}", e))?
    } else {
        vec![serde_json::from_value(parsed)
            .map_err(|e| format!("Invalid USB target shape: {}", e))?]
    };

    Ok(rows
        .into_iter()
        .map(|row| UsbTarget {
            disk_number: row.disk_number,
            friendly_name: row
                .friendly_name
                .unwrap_or_else(|| "Removable Drive".to_string()),
            size_bytes: row.size_bytes.unwrap_or(0),
            bus_type: row.bus_type.unwrap_or_else(|| "Unknown".to_string()),
            drive_letters: row.drive_letters.unwrap_or_default(),
            is_system: row.is_system.unwrap_or(false),
            is_boot: row.is_boot.unwrap_or(false),
            is_read_only: row.is_read_only.unwrap_or(false),
            confirmation_phrase: format!("WIPE DISK {}", row.disk_number),
        })
        .collect())
}

#[cfg(target_os = "windows")]
pub fn write_iso_to_usb(request: WriteIsoToUsbRequest) -> Result<String, String> {
    let iso_path = PathBuf::from(request.iso_path.trim());
    if !iso_path.is_file() {
        return Err(format!("ISO file was not found: {}", iso_path.display()));
    }

    let targets = list_usb_targets()?;
    validate_target_and_confirmation(
        &targets,
        request.target_disk_number,
        request.confirmation_token.trim(),
    )?;

    let escaped_iso = ps_single_quote(&iso_path.to_string_lossy());
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$diskNumber = {disk}
$isoPath = '{iso}'
$physicalPath = "\\.\PhysicalDrive$diskNumber"

Set-Disk -Number $diskNumber -IsReadOnly $false -ErrorAction Stop
Set-Disk -Number $diskNumber -IsOffline $false -ErrorAction Stop
Clear-Disk -Number $diskNumber -RemoveData -Confirm:$false -ErrorAction Stop

$bufferSize = 4MB
$buffer = New-Object byte[] $bufferSize
$isoStream = [System.IO.File]::OpenRead($isoPath)
try {{
    $diskStream = New-Object System.IO.FileStream($physicalPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
    try {{
        while (($read = $isoStream.Read($buffer, 0, $buffer.Length)) -gt 0) {{
            $diskStream.Write($buffer, 0, $read)
        }}
        $diskStream.Flush()
    }} finally {{
        $diskStream.Close()
    }}
}} finally {{
    $isoStream.Close()
}}

Write-Output "ISO write completed to disk $diskNumber"
"#,
        disk = request.target_disk_number,
        iso = escaped_iso
    );

    run_powershell(&script)?;
    Ok(format!(
        "ISO was written to removable disk {}.",
        request.target_disk_number
    ))
}

#[cfg(target_os = "windows")]
pub fn write_provisioning_bundle_to_usb(
    request: WriteProvisioningBundleRequest,
) -> Result<String, String> {
    let profile_name = request.profile_name.trim();
    if profile_name.is_empty() {
        return Err("Profile name is required.".to_string());
    }

    let targets = list_usb_targets()?;
    validate_target_and_confirmation(
        &targets,
        request.target_disk_number,
        request.confirmation_token.trim(),
    )?;

    let profile_path = oobe_profiles::resolve_oobe_profile_path(profile_name)
        .or_else(|| Some(Path::new(oobe_profiles::OOBE_ROOT).join(profile_name)))
        .ok_or_else(|| format!("Failed to resolve provisioning profile: {}", profile_name))?;
    if !profile_path.is_dir() {
        return Err(format!(
            "Provisioning profile was not found at {}",
            profile_path.display()
        ));
    }

    let staging_root = std::env::temp_dir().join(format!(
        "bitosdt-usb-provisioning-{}-{}",
        profile_name,
        Uuid::new_v4()
    ));
    fs::create_dir_all(&staging_root)
        .map_err(|e| format!("Failed to create staging directory: {}", e))?;
    let ppkg_path = staging_root.join(format!("{}.ppkg", profile_name));

    let ppkg_request = ppkg::PpkgRequest {
        profile_name: Some(profile_name.to_string()),
        profile_path: None,
        output_ppkg_path: ppkg_path.to_string_lossy().to_string(),
        builder_path: None,
        owner: None,
        rank: None,
        version: None,
        signing: None,
        local_admin_username: request.local_admin_username.clone(),
        local_admin_password: request.local_admin_password.clone(),
    };
    ppkg::generate_oobe_ppkg(ppkg_request)
        .map_err(|e| format!("Failed to generate provisioning package: {}", e))?;

    let volume_root = prepare_removable_volume(request.target_disk_number)?;
    if volume_root
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with("c:\\")
    {
        return Err("Refusing to target C:\\ for USB provisioning writes.".to_string());
    }

    let provisioning_dest = volume_root.join("Provisioning").join(profile_name);
    copy_directory_recursive(&profile_path, &provisioning_dest)?;

    let ppkg_dir = ppkg_path
        .parent()
        .ok_or_else(|| "Failed to resolve PPKG output directory.".to_string())?;
    fs::copy(
        &ppkg_path,
        volume_root.join(format!("{}.ppkg", profile_name)),
    )
    .map_err(|e| format!("Failed to copy generated PPKG to USB media: {}", e))?;

    for folder in ["Scripts", "Apps", "Files"] {
        let source = ppkg_dir.join(folder);
        let destination = volume_root.join(folder);
        if source.exists() {
            copy_directory_recursive(&source, &destination)?;
        }
    }
    let readme = ppkg_dir.join("PPKG-README.txt");
    if readme.is_file() {
        fs::copy(&readme, volume_root.join("PPKG-README.txt"))
            .map_err(|e| format!("Failed to copy PPKG-README.txt: {}", e))?;
    }

    let _ = fs::remove_dir_all(&staging_root);

    Ok(format!(
        "Provisioning payload for '{}' was written to removable disk {} at {}",
        profile_name,
        request.target_disk_number,
        volume_root.display()
    ))
}

#[cfg(any(test, target_os = "windows"))]
fn validate_target_and_confirmation(
    targets: &[UsbTarget],
    target_disk_number: u32,
    confirmation_token: &str,
) -> Result<(), String> {
    let target = targets
        .iter()
        .find(|item| item.disk_number == target_disk_number)
        .ok_or_else(|| format!("USB target disk {} was not found.", target_disk_number))?;

    if target.is_system || target.is_boot {
        return Err(format!(
            "Refusing to write removable disk {} because it is marked as system/boot.",
            target_disk_number
        ));
    }
    if target.is_read_only {
        return Err(format!(
            "Refusing to write removable disk {} because it is read-only.",
            target_disk_number
        ));
    }
    if target
        .drive_letters
        .iter()
        .any(|letter| letter.eq_ignore_ascii_case("C:"))
    {
        return Err("Refusing to write a target that maps to C:\\".to_string());
    }

    let expected = format!("WIPE DISK {}", target_disk_number);
    if !confirmation_token.eq_ignore_ascii_case(&expected) {
        return Err(format!(
            "Invalid destructive confirmation token. Expected '{}'.",
            expected
        ));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn prepare_removable_volume(target_disk_number: u32) -> Result<PathBuf, String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$diskNumber = {disk}

Set-Disk -Number $diskNumber -IsReadOnly $false -ErrorAction Stop
Set-Disk -Number $diskNumber -IsOffline $false -ErrorAction Stop
Clear-Disk -Number $diskNumber -RemoveData -Confirm:$false -ErrorAction Stop
Initialize-Disk -Number $diskNumber -PartitionStyle GPT -ErrorAction Stop | Out-Null
$partition = New-Partition -DiskNumber $diskNumber -UseMaximumSize -AssignDriveLetter -ErrorAction Stop
$volume = Format-Volume -Partition $partition -FileSystem NTFS -NewFileSystemLabel 'BITOSDTUSB' -Confirm:$false -ErrorAction Stop
$letter = $volume.DriveLetter
if ([string]::IsNullOrWhiteSpace($letter)) {{
    throw 'Failed to assign drive letter to removable media.'
}}
Write-Output ($letter + ':\')
"#,
        disk = target_disk_number
    );

    let stdout = run_powershell(&script)?;
    let drive_root = stdout
        .lines()
        .map(str::trim)
        .find(|line| line.ends_with(":\\"))
        .ok_or_else(|| "Failed to resolve formatted USB drive letter.".to_string())?;
    Ok(PathBuf::from(drive_root))
}

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Result<String, String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|e| format!("Failed to launch PowerShell: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("PowerShell command failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn copy_directory_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }

    fs::create_dir_all(destination).map_err(|e| {
        format!(
            "Failed to create destination directory {}: {}",
            destination.display(),
            e
        )
    })?;

    for entry in
        fs::read_dir(source).map_err(|e| format!("Failed to read {}: {}", source.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = destination.join(entry.file_name());
        if src_path.is_dir() {
            copy_directory_recursive(&src_path, &dst_path)?;
        } else if src_path.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create directory {}: {}", parent.display(), e)
                })?;
            }
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

fn ps_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_target(disk_number: u32) -> UsbTarget {
        UsbTarget {
            disk_number,
            friendly_name: "USB Drive".to_string(),
            size_bytes: 64 * 1024 * 1024 * 1024,
            bus_type: "USB".to_string(),
            drive_letters: vec!["E:".to_string()],
            is_system: false,
            is_boot: false,
            is_read_only: false,
            confirmation_phrase: format!("WIPE DISK {}", disk_number),
        }
    }

    #[test]
    fn guardrails_accept_valid_target_and_confirmation() {
        let targets = vec![make_target(2)];
        validate_target_and_confirmation(&targets, 2, "WIPE DISK 2").expect("valid target");
    }

    #[test]
    fn guardrails_reject_c_drive_mapped_target() {
        let mut target = make_target(3);
        target.drive_letters = vec!["C:".to_string()];
        let err = validate_target_and_confirmation(&[target], 3, "WIPE DISK 3")
            .expect_err("c drive mapping should fail");
        assert!(err.contains("C:\\"));
    }

    #[test]
    fn guardrails_reject_system_or_boot_or_readonly_target() {
        let mut system = make_target(4);
        system.is_system = true;
        assert!(validate_target_and_confirmation(&[system], 4, "WIPE DISK 4").is_err());

        let mut boot = make_target(5);
        boot.is_boot = true;
        assert!(validate_target_and_confirmation(&[boot], 5, "WIPE DISK 5").is_err());

        let mut readonly = make_target(6);
        readonly.is_read_only = true;
        assert!(validate_target_and_confirmation(&[readonly], 6, "WIPE DISK 6").is_err());
    }

    #[test]
    fn guardrails_reject_invalid_confirmation_token() {
        let target = make_target(7);
        let err = validate_target_and_confirmation(&[target], 7, "WIPE DISK 9")
            .expect_err("invalid token should fail");
        assert!(err.contains("Invalid destructive confirmation token"));
    }
}
