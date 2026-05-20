use crate::core::errors::{BitOSDTError, BitOSDTResult};
use crate::deploy::{WimImage, WimInfo};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tracing::info;

#[derive(Debug, Clone)]
pub struct WinpeAssetBundle {
    pub root: PathBuf,
    pub media_dir: PathBuf,
    pub runtime_executable: Option<PathBuf>,
}

pub fn default_winpe_assets_path() -> BitOSDTResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        BitOSDTError::Config(crate::core::errors::ConfigError::MissingField(
            "Home directory not found for WinPE asset cache".to_string(),
        ))
    })?;
    Ok(home.join(".bitosdt").join("winpe-assets"))
}

pub fn sync_winpe_asset_bundle(
    source: Option<&Path>,
    target: Option<&Path>,
) -> BitOSDTResult<WinpeAssetBundle> {
    let target_root = target
        .map(Path::to_path_buf)
        .unwrap_or(default_winpe_assets_path()?);

    let Some(source_root) = source else {
        return Err(BitOSDTError::NotImplemented(format!(
            "Automatic WinPE asset download is not configured for this build. Supply a local asset bundle and rerun with --source. Expected default cache path: {}",
            target_root.display()
        )));
    };

    if !source_root.exists() {
        return Err(BitOSDTError::NotFound(format!(
            "WinPE asset bundle source was not found: {}",
            source_root.display()
        )));
    }

    if target_root.exists() {
        fs::remove_dir_all(&target_root)?;
    }
    fs::create_dir_all(&target_root)?;
    copy_directory_recursive(source_root, &target_root)?;

    validate_winpe_asset_bundle(&target_root)
}

pub fn validate_winpe_asset_bundle(root: &Path) -> BitOSDTResult<WinpeAssetBundle> {
    let media_dir = [
        root.join("media"),
        root.join("Media"),
        root.join("winpe").join("media"),
        root.join("winpe").join("Media"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_dir())
    .ok_or_else(|| {
        BitOSDTError::NotFound(format!(
            "WinPE asset bundle is missing a media directory under {}. Expected one of: media/, Media/, winpe/media/.",
            root.display()
        ))
    })?;

    let boot_wim = media_dir.join("sources").join("boot.wim");
    if !boot_wim.is_file() {
        return Err(BitOSDTError::NotFound(format!(
            "WinPE asset bundle is missing sources/boot.wim at {}",
            boot_wim.display()
        )));
    }

    let runtime_executable = [
        root.join("runtime").join("bitosdt.exe"),
        root.join("bitosdt.exe"),
        root.join("tools").join("bitosdt.exe"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file());

    Ok(WinpeAssetBundle {
        root: root.to_path_buf(),
        media_dir,
        runtime_executable,
    })
}

pub fn extract_iso_image(source_path: &Path, extract_dir: &Path) -> BitOSDTResult<()> {
    fs::create_dir_all(extract_dir)?;

    if let Some(tool) = find_tool(&["7zz", "7z"]) {
        let output = Command::new(&tool)
            .arg("x")
            .arg("-y")
            .arg(format!("-o{}", extract_dir.display()))
            .arg(source_path)
            .output()
            .map_err(|e| {
                BitOSDTError::WinPE(format!(
                    "Failed to run {} for ISO extraction: {}",
                    tool.display(),
                    e
                ))
            })?;

        if output.status.success() {
            return Ok(());
        }

        return Err(BitOSDTError::WinPE(format!(
            "{} failed while extracting ISO: {}",
            tool.display(),
            output_message(&output),
        )));
    }

    if let Some(tool) = find_tool(&["bsdtar"]) {
        let output = Command::new(&tool)
            .arg("-C")
            .arg(extract_dir)
            .arg("-xf")
            .arg(source_path)
            .output()
            .map_err(|e| {
                BitOSDTError::WinPE(format!(
                    "Failed to run {} for ISO extraction: {}",
                    tool.display(),
                    e
                ))
            })?;

        if output.status.success() {
            return Ok(());
        }

        return Err(BitOSDTError::WinPE(format!(
            "{} failed while extracting ISO: {}",
            tool.display(),
            output_message(&output),
        )));
    }

    Err(BitOSDTError::NotImplemented(
        "Linux ISO extraction requires 7z/7zz or bsdtar to be installed".to_string(),
    ))
}

pub fn ensure_linux_build_prerequisites(require_iso_tools: bool) -> BitOSDTResult<()> {
    ensure_tool_available(
        &["wimlib-imagex"],
        "WIM servicing on Linux requires wimlib-imagex",
    )?;

    ensure_tool_available(&["xorriso"], "ISO creation on Linux requires xorriso")?;

    if require_iso_tools {
        ensure_tool_available(
            &["7zz", "7z", "bsdtar"],
            "Local ISO sources on Linux require 7zz, 7z, or bsdtar",
        )?;
    }

    Ok(())
}

pub fn export_image_to_wim(
    source_path: &Path,
    source_index: Option<u32>,
    destination_path: &Path,
) -> BitOSDTResult<()> {
    let index = source_index
        .map(|value| value.to_string())
        .unwrap_or_else(|| "all".to_string());

    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent)?;
    }

    run_wimlib_command(&[
        "export",
        &source_path.to_string_lossy(),
        &index,
        &destination_path.to_string_lossy(),
        "--compress=LZX",
    ])?;

    Ok(())
}

pub fn apply_wim_updates_from_directory(
    wim_path: &Path,
    image_index: u32,
    staging_root: &Path,
) -> BitOSDTResult<()> {
    let mut commands = Vec::new();
    collect_update_commands(staging_root, staging_root, &mut commands)?;

    if commands.is_empty() {
        return Ok(());
    }

    let args = vec![
        "update".to_string(),
        wim_path.to_string_lossy().to_string(),
        image_index.to_string(),
    ];
    let command_script = format!("{}\n", commands.join("\n"));
    run_wimlib_command_with_stdin(&args, &command_script)?;
    Ok(())
}

pub fn apply_wim_image(wim_path: &Path, image_index: u32, target_path: &Path) -> BitOSDTResult<()> {
    fs::create_dir_all(target_path)?;
    run_wimlib_command(&[
        "apply",
        &wim_path.to_string_lossy(),
        &image_index.to_string(),
        &target_path.to_string_lossy(),
    ])?;
    Ok(())
}

pub fn read_wim_info(wim_path: &Path) -> BitOSDTResult<WimInfo> {
    let output = run_wimlib_command_capture(&["info", &wim_path.to_string_lossy()])?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut images = Vec::new();
    let mut current_index: Option<u32> = None;
    let mut current_name: Option<String> = None;
    let mut current_description: Option<String> = None;

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if let Some(value) = line.strip_prefix("Index:") {
            if let (Some(index), Some(name)) = (current_index.take(), current_name.take()) {
                images.push(WimImage {
                    index,
                    name,
                    description: current_description.take().unwrap_or_default(),
                    size_bytes: 0,
                });
            }
            current_index = value.trim().parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("Name:") {
            current_name = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Description:") {
            current_description = Some(value.trim().to_string());
        }
    }

    if let (Some(index), Some(name)) = (current_index.take(), current_name.take()) {
        images.push(WimImage {
            index,
            name,
            description: current_description.unwrap_or_default(),
            size_bytes: 0,
        });
    }

    Ok(WimInfo {
        path: wim_path.to_path_buf(),
        images,
    })
}

pub fn runtime_executable_from_assets(root: &Path) -> Option<PathBuf> {
    validate_winpe_asset_bundle(root)
        .ok()
        .and_then(|bundle| bundle.runtime_executable)
}

pub fn ensure_tool_available(candidates: &[&str], message: &str) -> BitOSDTResult<PathBuf> {
    find_tool(candidates).ok_or_else(|| BitOSDTError::NotImplemented(message.to_string()))
}

pub fn find_tool(candidates: &[&str]) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    let paths: Vec<PathBuf> = env::split_paths(&path_var).collect();
    for candidate in candidates {
        let candidate_path = Path::new(candidate);
        if candidate_path.components().count() > 1 && candidate_path.is_file() {
            return Some(candidate_path.to_path_buf());
        }

        for path in &paths {
            let full = path.join(candidate);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

fn collect_update_commands(
    root: &Path,
    current: &Path,
    commands: &mut Vec<String>,
) -> BitOSDTResult<()> {
    if !current.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_update_commands(root, &path, commands)?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == ".bitosdt-wim-source")
        {
            continue;
        }

        let relative = path.strip_prefix(root).map_err(|e| {
            BitOSDTError::InvalidInput(format!(
                "Failed to map {} into a WIM update path: {}",
                path.display(),
                e
            ))
        })?;

        let destination = format!("/{}", relative.to_string_lossy().replace('\\', "/"));
        commands.push(format!(
            "add '{}' '{}'",
            escape_wimlib_value(&path.to_string_lossy()),
            escape_wimlib_value(&destination),
        ));
    }

    Ok(())
}

fn run_wimlib_command(args: &[&str]) -> BitOSDTResult<()> {
    let output = run_wimlib_command_capture(args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(BitOSDTError::WinPE(format!(
            "wimlib-imagex {} failed: {}",
            args.join(" "),
            output_message(&output)
        )))
    }
}

fn run_wimlib_command_with_stdin(args: &[String], stdin_payload: &str) -> BitOSDTResult<()> {
    let tool = ensure_tool_available(
        &["wimlib-imagex"],
        "WIM servicing on Linux requires wimlib-imagex",
    )?;
    info!("Running {} {} <stdin>", tool.display(), args.join(" "));

    let mut child = Command::new(&tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| BitOSDTError::WinPE(format!("Failed to run {}: {}", tool.display(), e)))?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        BitOSDTError::WinPE(format!(
            "Failed to open stdin for {} while updating WIM",
            tool.display()
        ))
    })?;
    stdin.write_all(stdin_payload.as_bytes()).map_err(|e| {
        BitOSDTError::WinPE(format!(
            "Failed to write update commands to wimlib-imagex stdin: {}",
            e
        ))
    })?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|e| BitOSDTError::WinPE(format!("Failed to wait on {}: {}", tool.display(), e)))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(BitOSDTError::WinPE(format!(
            "{} failed: {}",
            tool.display(),
            output_message(&output)
        )))
    }
}

fn run_wimlib_command_capture(args: &[&str]) -> BitOSDTResult<Output> {
    let tool = ensure_tool_available(
        &["wimlib-imagex"],
        "WIM servicing on Linux requires wimlib-imagex",
    )?;
    info!("Running {} {}", tool.display(), args.join(" "));
    Command::new(&tool)
        .args(args)
        .output()
        .map_err(|e| BitOSDTError::WinPE(format!("Failed to run {}: {}", tool.display(), e)))
}

fn copy_directory_recursive(source: &Path, destination: &Path) -> BitOSDTResult<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory_recursive(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn escape_wimlib_value(value: &str) -> String {
    value.replace('\'', "'\\''")
}

fn output_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("stdout={stdout}; stderr={stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => "<no output>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validate_winpe_asset_bundle_accepts_media_layout() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media").join("sources");
        fs::create_dir_all(&media).unwrap();
        fs::write(media.join("boot.wim"), b"boot").unwrap();
        fs::create_dir_all(dir.path().join("runtime")).unwrap();
        fs::write(dir.path().join("runtime").join("bitosdt.exe"), b"exe").unwrap();

        let bundle = validate_winpe_asset_bundle(dir.path()).expect("bundle should validate");
        assert_eq!(bundle.media_dir, dir.path().join("media"));
        assert_eq!(
            bundle.runtime_executable,
            Some(dir.path().join("runtime").join("bitosdt.exe"))
        );
    }

    #[test]
    fn collect_update_commands_builds_wimlib_add_commands() {
        let dir = tempdir().unwrap();
        let file = dir
            .path()
            .join("Windows")
            .join("Panther")
            .join("unattend.xml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"<xml/>").unwrap();

        let mut commands = Vec::new();
        collect_update_commands(dir.path(), dir.path(), &mut commands).unwrap();

        assert_eq!(commands.len(), 1);
        assert!(commands[0].contains("add '"));
        assert!(commands[0].contains("/Windows/Panther/unattend.xml"));
    }
}
