#[cfg(not(target_os = "windows"))]
use crate::build::linux_support::{
    apply_wim_updates_from_directory, default_winpe_assets_path, validate_winpe_asset_bundle,
};
use crate::core::adk::AdkPaths;
#[cfg(target_os = "windows")]
use crate::core::adk::{
    candidate_copype_paths, is_supported_copype_arch, normalize_arch, resolve_adk_paths,
};
use crate::core::errors::BitOSDTError;
use crate::core::errors::BitOSDTResult;
#[cfg(target_os = "windows")]
use crate::core::windows_tools::{
    dism_path_arg, format_process_failure, resolve_dism_exe, run_dism_streaming_with_role,
    run_dism_with_role, run_tracked_command_streaming,
};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;
use tracing::{info, warn};

pub struct WinPEBuilder {
    adk_paths: Option<AdkPaths>,
    working_dir: PathBuf,
    architecture: String,
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    linux_asset_bundle: Option<PathBuf>,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn normalize_winpe_language_tag(language: &str) -> String {
    let normalized = language.trim().replace('_', "-").to_ascii_lowercase();

    match normalized.as_str() {
        "" | "en" | "enus" | "en-us" => "en-us".to_string(),
        "engb" | "en-gb" => "en-gb".to_string(),
        _ if normalized.len() == 4 && !normalized.contains('-') => {
            format!("{}-{}", &normalized[0..2], &normalized[2..4])
        }
        _ => normalized,
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn language_cab_path(oc_path: &Path, package_name: &str, language: &str) -> PathBuf {
    let language_tag = normalize_winpe_language_tag(language);
    oc_path
        .join(&language_tag)
        .join(format!("{}_{}.cab", package_name, language_tag))
}

fn find_file_case_insensitive(root: &Path, target_file_name: &str) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }

    find_file_case_insensitive_inner(root, target_file_name)
}

fn find_file_case_insensitive_inner(dir: &Path, target_file_name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.eq_ignore_ascii_case(target_file_name) {
                    return Some(path);
                }
            }
            continue;
        }

        if path.is_dir() {
            if let Some(found) = find_file_case_insensitive_inner(&path, target_file_name) {
                return Some(found);
            }
        }
    }

    None
}

fn collect_font_files_recursive(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_font_files_recursive(&path, out);
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "ttf" | "otf" | "ttc") {
            out.push(path);
        }
    }
}

fn clean_existing_copype_destination(path: &Path) -> BitOSDTResult<()> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

impl WinPEBuilder {
    pub fn new(working_dir: PathBuf, architecture: String) -> Self {
        Self {
            adk_paths: None,
            working_dir,
            architecture,
            linux_asset_bundle: None,
        }
    }

    /// Initialize the builder by locating Windows ADK
    pub fn initialize(&mut self) -> BitOSDTResult<()> {
        self.initialize_with_assets(None, None)
    }

    /// Initialize the builder by locating Windows ADK with an optional override
    pub fn initialize_with_override(&mut self, adk_override: Option<&Path>) -> BitOSDTResult<()> {
        self.initialize_with_assets(adk_override, None)
    }

    pub fn initialize_with_assets(
        &mut self,
        adk_override: Option<&Path>,
        _linux_asset_bundle: Option<&Path>,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            let adk_paths = if let Some(path) = adk_override {
                resolve_adk_paths(Some(path), &self.architecture)
            } else {
                resolve_adk_paths(None, &self.architecture)
            };
            self.adk_paths = adk_paths;
            info!(
                "Resolved Windows ADK: {:?}",
                self.adk_paths.as_ref().map(|p| &p.root)
            );
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = adk_override;
            self.linux_asset_bundle = _linux_asset_bundle.map(Path::to_path_buf);
        }

        // Create working directory
        std::fs::create_dir_all(&self.working_dir)?;

        Ok(())
    }

    /// Create a new WinPE image
    pub fn create_winpe(&self) -> BitOSDTResult<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            self.create_winpe_windows()
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.create_winpe_linux()
        }
    }

    #[cfg(target_os = "windows")]
    fn create_winpe_windows(&self) -> BitOSDTResult<PathBuf> {
        let adk_paths = self
            .adk_paths
            .as_ref()
            .ok_or_else(|| BitOSDTError::WinPE("Windows ADK not found".to_string()))?;
        let copype_arch = normalize_arch(&self.architecture);

        if !is_supported_copype_arch(&copype_arch) {
            return Err(BitOSDTError::WinPE(format!(
                "Unsupported WinPE architecture '{}' (normalized '{}'). Supported values: amd64, x86, arm, arm64.",
                self.architecture, copype_arch
            )));
        }

        if !adk_paths.winpe_root.exists() {
            return Err(BitOSDTError::WinPE(format!(
                "Windows ADK found at {:?}, but WinPE files were not detected at {:?}. Install the Windows PE add-on for ADK.",
                adk_paths.root, adk_paths.winpe_root
            )));
        }

        if !adk_paths.copype_cmd.exists() {
            let all_candidates = candidate_copype_paths(&adk_paths.root, &copype_arch);
            let searched: Vec<String> = all_candidates
                .iter()
                .map(|p| format!("  - {} {}", if p.exists() { "✓" } else { "✗" }, p.display()))
                .collect();
            return Err(BitOSDTError::WinPE(format!(
                "copype tool not found in ADK root {:?}. Searched paths:\n{}",
                adk_paths.root,
                searched.join("\n")
            )));
        }

        let winpe_dir = self.working_dir.join("winpe");
        let winpe_root_env = adk_paths.root.join("Windows Preinstallation Environment");
        let oscdimg_root_env = adk_paths
            .oscdimg_exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| {
                adk_paths
                    .root
                    .join("Deployment Tools")
                    .join(&copype_arch)
                    .join("Oscdimg")
            });
        let dism_root_env = adk_paths
            .dism_exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| {
                adk_paths
                    .root
                    .join("Deployment Tools")
                    .join(&copype_arch)
                    .join("DISM")
            });

        info!("Creating WinPE working directory...");

        clean_existing_copype_destination(&winpe_dir)?;

        // Run copype to create base WinPE
        let extension = adk_paths
            .copype_cmd
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let (executable, args, output) = match extension.as_str() {
            "ps1" => {
                let executable = "powershell".to_string();
                let args = vec![
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    adk_paths.copype_cmd.to_string_lossy().to_string(),
                    copype_arch.clone(),
                    winpe_dir.to_string_lossy().to_string(),
                ];

                let mut command = Command::new(&executable);
                command.args(&args);
                command
                    .env("WinPERoot", &winpe_root_env)
                    .env("OSCDImgRoot", &oscdimg_root_env)
                    .env("DISMRoot", &dism_root_env);
                let output = run_tracked_command_streaming(
                    command,
                    Path::new(&executable),
                    &args,
                    "winpe-create",
                    |_| {},
                )
                .map_err(|e| BitOSDTError::WinPE(format!("Failed to run copype: {}", e)))?;
                (executable, args, output)
            }
            _ => {
                let executable = "cmd".to_string();
                let args = vec![
                    "/c".to_string(),
                    adk_paths.copype_cmd.to_string_lossy().to_string(),
                    copype_arch.clone(),
                    winpe_dir.to_string_lossy().to_string(),
                ];

                let mut command = Command::new(&executable);
                command.args(&args);
                command
                    .env("WinPERoot", &winpe_root_env)
                    .env("OSCDImgRoot", &oscdimg_root_env)
                    .env("DISMRoot", &dism_root_env);
                let output = run_tracked_command_streaming(
                    command,
                    Path::new(&executable),
                    &args,
                    "winpe-create",
                    |_| {},
                )
                .map_err(|e| BitOSDTError::WinPE(format!("Failed to run copype: {}", e)))?;
                (executable, args, output)
            }
        };

        if !output.status.success() {
            return Err(BitOSDTError::WinPE(format!(
                "copype failed (input_arch='{}', normalized_arch='{}', WinPERoot='{}', OSCDImgRoot='{}', DISMRoot='{}'): {}",
                self.architecture,
                copype_arch,
                winpe_root_env.display(),
                oscdimg_root_env.display(),
                dism_root_env.display(),
                format_process_failure(Path::new(&executable), &args, &output)
            )));
        }

        info!("WinPE base created at: {:?}", winpe_dir);

        Ok(winpe_dir)
    }

    #[cfg(not(target_os = "windows"))]
    fn create_winpe_linux(&self) -> BitOSDTResult<PathBuf> {
        let asset_root = self
            .linux_asset_bundle
            .clone()
            .unwrap_or(default_winpe_assets_path()?);
        let asset_bundle = validate_winpe_asset_bundle(&asset_root)?;

        let winpe_dir = self.working_dir.join("winpe");
        clean_existing_copype_destination(&winpe_dir)?;
        std::fs::create_dir_all(&winpe_dir)?;
        Self::copy_dir_all(&asset_bundle.media_dir, &winpe_dir.join("media"))?;

        info!(
            "Linux WinPE workspace created from asset bundle {:?} at {:?} (arch: {})",
            asset_bundle.root, winpe_dir, self.architecture
        );

        Ok(winpe_dir)
    }

    /// Mount the boot.wim for modification
    pub fn mount_wim(&self, wim_path: &Path, mount_dir: &Path) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!("Mounting WIM: {:?} -> {:?}", wim_path, mount_dir);

            std::fs::create_dir_all(mount_dir)?;

            let args = vec![
                "/Mount-Wim".to_string(),
                dism_path_arg("/WimFile", wim_path),
                "/Index:1".to_string(),
                dism_path_arg("/MountDir", mount_dir),
            ];

            let output = run_dism_with_role(&args, self.adk_paths.as_ref(), "winpe-mount")?;

            if !output.status.success() {
                let dism_exe = resolve_dism_exe(self.adk_paths.as_ref());
                return Err(BitOSDTError::WinPE(format!(
                    "DISM mount failed: {}",
                    format_process_failure(&dism_exe, &args, &output)
                )));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if mount_dir.exists() {
                std::fs::remove_dir_all(mount_dir)?;
            }
            std::fs::create_dir_all(mount_dir)?;
            std::fs::write(
                mount_dir.join(".bitosdt-wim-source"),
                wim_path.to_string_lossy().to_string(),
            )?;
        }

        Ok(())
    }

    /// Unmount the WIM and save changes
    pub fn unmount_wim(&self, mount_dir: &Path, commit: bool) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            info!("Unmounting WIM: {:?} (commit: {})", mount_dir, commit);

            let commit_flag = if commit { "/Commit" } else { "/Discard" };

            let args = vec![
                "/Unmount-Wim".to_string(),
                dism_path_arg("/MountDir", mount_dir),
                commit_flag.to_string(),
            ];

            let output = run_dism_with_role(&args, self.adk_paths.as_ref(), "winpe-unmount")?;

            if !output.status.success() {
                let dism_exe = resolve_dism_exe(self.adk_paths.as_ref());
                return Err(BitOSDTError::WinPE(format!(
                    "DISM unmount failed: {}",
                    format_process_failure(&dism_exe, &args, &output)
                )));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if commit {
                let marker_path = mount_dir.join(".bitosdt-wim-source");
                let source = std::fs::read_to_string(&marker_path).map_err(|e| {
                    BitOSDTError::WinPE(format!(
                        "Linux WIM staging metadata was missing at {}: {}",
                        marker_path.display(),
                        e
                    ))
                })?;
                apply_wim_updates_from_directory(Path::new(source.trim()), 1, mount_dir)?;
            }
        }

        Ok(())
    }

    /// Add drivers to mounted WIM
    pub fn add_drivers(&self, mount_dir: &Path, driver_dir: &Path) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            if !driver_dir.exists() {
                warn!("Driver directory does not exist: {:?}", driver_dir);
                return Ok(());
            }

            info!("Adding drivers from: {:?}", driver_dir);

            let args = vec![
                dism_path_arg("/Image", mount_dir),
                "/Add-Driver".to_string(),
                dism_path_arg("/Driver", driver_dir),
                "/Recurse".to_string(),
            ];

            let output = run_dism_streaming_with_role(
                &args,
                self.adk_paths.as_ref(),
                "winpe-drivers",
                |_| {},
            )?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Some drivers failed to add: {}", stderr);
                // Continue anyway - some drivers may fail but others succeed
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (mount_dir, driver_dir);
            warn!("Driver injection requires Windows DISM - skipping in development mode");
        }

        Ok(())
    }

    /// Add files to mounted WinPE
    pub fn add_files(
        &self,
        mount_dir: &Path,
        source: &Path,
        destination: &str,
    ) -> BitOSDTResult<()> {
        let dest_path = mount_dir.join(normalize_destination_path(destination));

        if source.is_file() {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(source, dest_path)?;
        } else if source.is_dir() {
            Self::copy_dir_all(source, &dest_path)?;
        }

        info!("Added {:?} -> {:?}", source, destination);

        Ok(())
    }

    /// Copy directory recursively
    fn copy_dir_all(src: &Path, dst: &Path) -> BitOSDTResult<()> {
        std::fs::create_dir_all(dst)?;

        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let dest = dst.join(entry.file_name());

            if path.is_dir() {
                Self::copy_dir_all(&path, &dest)?;
            } else {
                std::fs::copy(&path, &dest)?;
            }
        }

        Ok(())
    }

    /// Enable PowerShell in WinPE
    pub fn enable_powershell(&self, mount_dir: &Path) -> BitOSDTResult<()> {
        self.enable_powershell_for_language(mount_dir, "en-us")
    }

    /// Enable PowerShell in WinPE with language-specific optional components when available.
    pub fn enable_powershell_for_language(
        &self,
        mount_dir: &Path,
        language: &str,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            let language_tag = normalize_winpe_language_tag(language);
            info!(
                "Enabling PowerShell in WinPE (language: {})...",
                language_tag
            );

            let adk_paths = match self.adk_paths.as_ref() {
                Some(paths) => paths,
                None => {
                    warn!("PowerShell enablement requires Windows ADK - skipping");
                    return Ok(());
                }
            };

            let winpe_oc_path = adk_paths.winpe_ocs.clone();

            if !winpe_oc_path.exists() {
                warn!("WinPE OC path not found: {:?}", winpe_oc_path);
                return Ok(());
            }

            // Add WinPE PowerShell dependency chain.
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-WMI",
                &language_tag,
            )?;
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-NetFX",
                &language_tag,
            )?;
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-Scripting",
                &language_tag,
            )?;
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-PowerShell",
                &language_tag,
            )?;
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-StorageWMI",
                &language_tag,
            )?;
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-DismCmdlets",
                &language_tag,
            )?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (mount_dir, language);
            warn!("PowerShell enablement requires Windows ADK - skipping in development mode");
        }

        Ok(())
    }

    /// Attempt to enable HTA support in WinPE.
    /// Returns true when WinPE-HTA was successfully added, false when unavailable.
    pub fn enable_hta(&self, mount_dir: &Path) -> BitOSDTResult<bool> {
        self.enable_hta_for_language(mount_dir, "en-us")
    }

    /// Attempt to enable HTA support in WinPE with language-specific optional components.
    /// Returns true when WinPE-HTA was successfully added, false when unavailable.
    pub fn enable_hta_for_language(&self, mount_dir: &Path, language: &str) -> BitOSDTResult<bool> {
        #[cfg(target_os = "windows")]
        {
            let language_tag = normalize_winpe_language_tag(language);
            info!(
                "Enabling HTA support in WinPE (language: {})...",
                language_tag
            );

            let adk_paths = match self.adk_paths.as_ref() {
                Some(paths) => paths,
                None => {
                    warn!("HTA enablement requires Windows ADK - skipping");
                    return Ok(false);
                }
            };

            let winpe_oc_path = adk_paths.winpe_ocs.clone();
            if !winpe_oc_path.exists() {
                warn!("WinPE OC path not found: {:?}", winpe_oc_path);
                return Ok(false);
            }

            // HTA depends on scripting and NetFX in WinPE.
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-WMI",
                &language_tag,
            )?;
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-NetFX",
                &language_tag,
            )?;
            let _ = self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-Scripting",
                &language_tag,
            )?;
            return self.add_package_with_language_optional(
                mount_dir,
                &winpe_oc_path,
                "WinPE-HTA",
                &language_tag,
            );
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (mount_dir, language);
            warn!("HTA enablement requires Windows DISM - skipping in development mode");
            Ok(false)
        }
    }

    /// Enable additional WinPE components for richer graphics/runtime support.
    pub fn enable_extended_components(&self, mount_dir: &Path) -> BitOSDTResult<()> {
        self.enable_extended_components_for_language(mount_dir, "en-us")
    }

    /// Enable additional WinPE components for richer graphics/runtime support.
    pub fn enable_extended_components_for_language(
        &self,
        mount_dir: &Path,
        language: &str,
    ) -> BitOSDTResult<()> {
        #[cfg(target_os = "windows")]
        {
            let language_tag = normalize_winpe_language_tag(language);
            info!(
                "Enabling extended WinPE components (language: {})...",
                language_tag
            );

            let adk_paths = match self.adk_paths.as_ref() {
                Some(paths) => paths,
                None => {
                    warn!("Extended WinPE components require Windows ADK - skipping");
                    return Ok(());
                }
            };

            let winpe_oc_path = adk_paths.winpe_ocs.clone();
            if !winpe_oc_path.exists() {
                warn!("WinPE OC path not found: {:?}", winpe_oc_path);
                return Ok(());
            }

            for package_name in [
                "WinPE-MDAC",
                "WinPE-WinReTools",
                "WinPE-Fonts-Legacy",
                "WinPE-EnhancedStorage",
            ] {
                let _ = self.add_package_with_language_optional(
                    mount_dir,
                    &winpe_oc_path,
                    package_name,
                    &language_tag,
                )?;
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (mount_dir, language);
            warn!("Extended WinPE components require Windows ADK - skipping in development mode");
        }

        Ok(())
    }

    /// Copy VC runtime DLLs into WinPE System32 from a dependency source directory.
    pub fn inject_vc_runtime_dlls_from_dir(
        &self,
        mount_dir: &Path,
        source_dir: &Path,
    ) -> BitOSDTResult<()> {
        if !source_dir.exists() {
            warn!("VC runtime source directory not found: {:?}", source_dir);
            return Ok(());
        }

        let system32_dir = mount_dir.join("Windows").join("System32");
        std::fs::create_dir_all(&system32_dir)?;

        let runtime_dlls = [
            "vcruntime140.dll",
            "vcruntime140_1.dll",
            "msvcp140.dll",
            "msvcp140_1.dll",
            "msvcp140_2.dll",
        ];

        let mut copied = 0usize;
        for dll_name in runtime_dlls {
            match find_file_case_insensitive(source_dir, dll_name) {
                Some(source_file) => {
                    std::fs::copy(&source_file, system32_dir.join(dll_name))?;
                    copied += 1;
                    info!("Injected VC runtime DLL into WinPE System32: {}", dll_name);
                }
                None => warn!(
                    "VC runtime DLL not found under {:?}: {}",
                    source_dir, dll_name
                ),
            }
        }

        if copied == 0 {
            warn!("No VC runtime DLLs were injected. Add them under your WinPE package directory.");
        }

        Ok(())
    }

    /// Copy custom font files into WinPE Windows\Fonts from a source directory.
    pub fn inject_custom_fonts_from_dir(
        &self,
        mount_dir: &Path,
        source_dir: &Path,
    ) -> BitOSDTResult<usize> {
        if !source_dir.exists() {
            warn!("Custom font source directory not found: {:?}", source_dir);
            return Ok(0);
        }

        let mut font_files = Vec::new();
        collect_font_files_recursive(source_dir, &mut font_files);
        if font_files.is_empty() {
            warn!(
                "No font files found under {:?}. Expected .ttf/.otf/.ttc files.",
                source_dir
            );
            return Ok(0);
        }

        let fonts_dir = mount_dir.join("Windows").join("Fonts");
        std::fs::create_dir_all(&fonts_dir)?;

        let mut copied = 0usize;
        for font_file in font_files {
            if let Some(file_name) = font_file.file_name() {
                std::fs::copy(&font_file, fonts_dir.join(file_name))?;
                copied += 1;
            }
        }

        info!("Injected {} custom font files into WinPE.", copied);
        Ok(copied)
    }

    /// Copy loading.html into BitOSDT\Packages when present next to the packages root.
    pub fn inject_loading_html(
        &self,
        mount_dir: &Path,
        packages_dir: &Path,
    ) -> BitOSDTResult<bool> {
        let mut candidates = vec![packages_dir.join("loading.html")];
        if let Some(parent) = packages_dir.parent() {
            candidates.push(parent.join("loading.html"));
        }

        let source = candidates.into_iter().find(|path| path.is_file());
        let Some(source_path) = source else {
            warn!(
                "loading.html not found near packages directory {:?}.",
                packages_dir
            );
            return Ok(false);
        };

        let destination = mount_dir
            .join("BitOSDT")
            .join("Packages")
            .join("loading.html");
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&source_path, &destination)?;
        info!("Injected loading screen asset: {:?}", destination);
        Ok(true)
    }

    #[cfg(target_os = "windows")]
    fn add_package_with_language_optional(
        &self,
        mount_dir: &Path,
        oc_path: &Path,
        package_name: &str,
        language: &str,
    ) -> BitOSDTResult<bool> {
        let base_added = self.add_package_optional(mount_dir, oc_path, package_name)?;
        if !base_added {
            return Ok(false);
        }

        let language_cab = language_cab_path(oc_path, package_name, language);
        if !language_cab.exists() {
            warn!(
                "Language package not found for {} ({}): {:?}",
                package_name,
                normalize_winpe_language_tag(language),
                language_cab
            );
            return Ok(true);
        }

        let language_label = format!(
            "{}_{}",
            package_name,
            normalize_winpe_language_tag(language)
        );
        let language_added =
            self.add_cab_package_optional(mount_dir, &language_cab, &language_label)?;
        if !language_added {
            warn!(
                "Language package failed to add for {} ({})",
                package_name, language_label
            );
        }

        Ok(true)
    }

    #[cfg(target_os = "windows")]
    fn add_package_optional(
        &self,
        mount_dir: &Path,
        oc_path: &Path,
        package_name: &str,
    ) -> BitOSDTResult<bool> {
        let cab_file = oc_path.join(format!("{}.cab", package_name));

        if !cab_file.exists() {
            warn!("Package not found: {:?}", cab_file);
            return Ok(false);
        }

        self.add_cab_package_optional(mount_dir, &cab_file, package_name)
    }

    #[cfg(target_os = "windows")]
    fn add_cab_package_optional(
        &self,
        mount_dir: &Path,
        cab_file: &Path,
        package_name: &str,
    ) -> BitOSDTResult<bool> {
        let args = vec![
            dism_path_arg("/Image", mount_dir),
            "/Add-Package".to_string(),
            dism_path_arg("/PackagePath", cab_file),
        ];

        let output = run_dism_with_role(&args, self.adk_paths.as_ref(), "winpe-package")?;

        if !output.status.success() {
            let dism_exe = resolve_dism_exe(self.adk_paths.as_ref());
            warn!(
                "Failed to add package {}: {}",
                package_name,
                format_process_failure(&dism_exe, &args, &output)
            );
            Ok(false)
        } else {
            info!("Added package: {}", package_name);
            Ok(true)
        }
    }

    pub fn adk_paths(&self) -> Option<&AdkPaths> {
        self.adk_paths.as_ref()
    }
}

fn normalize_destination_path(destination: &str) -> PathBuf {
    destination
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .fold(PathBuf::new(), |mut path, segment| {
            path.push(segment);
            path
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn normalize_winpe_language_tag_variants() {
        assert_eq!(normalize_winpe_language_tag(""), "en-us");
        assert_eq!(normalize_winpe_language_tag("EN"), "en-us");
        assert_eq!(normalize_winpe_language_tag("EN_US"), "en-us");
        assert_eq!(normalize_winpe_language_tag("engb"), "en-gb");
        assert_eq!(normalize_winpe_language_tag("fr-fr"), "fr-fr");
        assert_eq!(normalize_winpe_language_tag("ptbr"), "pt-br");
    }

    #[test]
    fn language_cab_path_resolves_expected_layout() {
        let oc_root = PathBuf::from("C:/ADK/WinPE_OCs");
        let cab = language_cab_path(&oc_root, "WinPE-HTA", "en-US");
        assert_eq!(cab, oc_root.join("en-us").join("WinPE-HTA_en-us.cab"));
    }

    #[test]
    fn find_file_case_insensitive_finds_nested_match() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("MSVCP140.DLL");
        std::fs::write(&file, b"dll").unwrap();

        let found = find_file_case_insensitive(dir.path(), "msvcp140.dll");
        assert_eq!(found, Some(file));
    }

    #[test]
    fn collect_font_files_recursive_filters_supported_extensions() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("fonts").join("more");
        std::fs::create_dir_all(&nested).unwrap();

        let ttf = nested.join("Inter-Regular.ttf");
        let txt = nested.join("README.txt");
        std::fs::write(&ttf, b"font").unwrap();
        std::fs::write(&txt, b"ignore").unwrap();

        let mut collected = Vec::new();
        collect_font_files_recursive(dir.path(), &mut collected);

        assert_eq!(collected.len(), 1);
        assert!(collected.contains(&ttf));
    }

    #[test]
    fn inject_loading_html_copies_file_from_packages_parent() {
        let dir = tempdir().unwrap();
        let mount_dir = dir.path().join("mount");
        let packages_dir = dir.path().join("Packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        std::fs::write(dir.path().join("loading.html"), "<html></html>").unwrap();

        let builder = WinPEBuilder::new(dir.path().join("workspace"), "amd64".to_string());
        let copied = builder
            .inject_loading_html(&mount_dir, &packages_dir)
            .expect("inject loading html");
        assert!(copied);
        assert!(mount_dir
            .join("BitOSDT")
            .join("Packages")
            .join("loading.html")
            .exists());
    }

    #[test]
    fn clean_existing_copype_destination_removes_directory() {
        let temp = tempdir().expect("temp dir");
        let winpe_dir = temp.path().join("winpe");

        std::fs::create_dir_all(winpe_dir.join("media")).expect("create test structure");
        std::fs::write(winpe_dir.join("media").join("boot.wim"), "test").expect("write file");

        clean_existing_copype_destination(&winpe_dir).expect("cleanup succeeds");
        assert!(!winpe_dir.exists());
    }
}
