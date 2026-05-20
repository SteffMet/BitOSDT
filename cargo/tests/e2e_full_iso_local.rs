#![cfg(target_os = "windows")]

use bitosdt::build::{build_full_iso, DiskSelectionPolicy, FullIsoBuildConfig};
use bitosdt::config::{
    NetworkLocation, OobeConfig, ProtectYourPc, UnattendConfig, UserAccountConfig, UserGroup,
};
use bitosdt::core::RuntimeDriverPolicy;
use bitosdt::tasks::{
    AppInstallConfig, CustomInstaller, InstallerSourceType, InstallerType, TaskDefinition,
    TaskSequence, TaskSettings, TaskType, WingetPackage,
};
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

fn remove_dir_all_with_force(path: &Path, description: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    for attempt in 1..=5 {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                let escaped = path.to_string_lossy().replace('\'', "''");
                let script = format!(
                    "if (Test-Path -LiteralPath '{p}') {{ \
                        Get-ChildItem -LiteralPath '{p}' -Recurse -Force -ErrorAction SilentlyContinue | \
                            ForEach-Object {{ $_.Attributes = 'Normal' }}; \
                        Remove-Item -LiteralPath '{p}' -Recurse -Force -ErrorAction SilentlyContinue \
                    }}",
                    p = escaped
                );
                let _ = Command::new("powershell.exe")
                    .args([
                        "-NoProfile",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-Command",
                        &script,
                    ])
                    .status();

                if !path.exists() {
                    return Ok(());
                }

                if attempt == 5 {
                    return Err(format!("{}: {} ({})", description, err, path.display()));
                }

                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }

    Ok(())
}

struct MountGuard {
    mount_dir: PathBuf,
    mounted: bool,
}

impl MountGuard {
    fn new(mount_dir: PathBuf) -> Self {
        Self {
            mount_dir,
            mounted: false,
        }
    }

    fn mount(&mut self, wim_path: &Path) -> Result<(), String> {
        if self.mount_dir.exists() {
            remove_dir_all_with_force(&self.mount_dir, "Failed to remove existing mount dir")?;
        }
        fs::create_dir_all(&self.mount_dir).map_err(|e| {
            format!(
                "Failed to create mount dir {}: {}",
                self.mount_dir.display(),
                e
            )
        })?;

        let args = vec![
            "/Mount-Wim".to_string(),
            format!("/WimFile:{}", wim_path.display()),
            "/Index:1".to_string(),
            format!("/MountDir:{}", self.mount_dir.display()),
        ];

        let output = Command::new("dism")
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to run DISM mount: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "DISM mount failed (exit={:?})\nstdout:\n{}\nstderr:\n{}",
                output.status.code(),
                stdout,
                stderr
            ));
        }

        self.mounted = true;
        Ok(())
    }
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        if !self.mounted {
            return;
        }

        let args = vec![
            "/Unmount-Wim".to_string(),
            format!("/MountDir:{}", self.mount_dir.display()),
            "/Discard".to_string(),
        ];
        let _ = Command::new("dism").args(&args).output();
    }
}

fn resolve_downloads_dir() -> Result<PathBuf, String> {
    bitosdt::core::Config::configured_download_path()
        .map_err(|e| format!("Failed to resolve downloads directory: {}", e))
}

fn pick_esd_source(downloads_dir: &Path) -> Result<(PathBuf, String), String> {
    if !downloads_dir.exists() {
        return Err(format!(
            "Downloads directory not found: {}",
            downloads_dir.display()
        ));
    }

    let mut esds: Vec<PathBuf> = fs::read_dir(downloads_dir)
        .map_err(|e| format!("Failed to list downloads: {}", e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("esd"))
                .unwrap_or(false)
        })
        .collect();

    esds.sort();
    if esds.is_empty() {
        return Err(format!(
            "No .esd files found in {}",
            downloads_dir.display()
        ));
    }

    let en_us = esds.iter().find(|path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_ascii_lowercase().contains("en-us"))
            .unwrap_or(false)
    });
    if let Some(path) = en_us {
        return Ok((path.clone(), "en-US".to_string()));
    }

    let en_gb = esds.iter().find(|path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_ascii_lowercase().contains("en-gb"))
            .unwrap_or(false)
    });
    if let Some(path) = en_gb {
        return Ok((path.clone(), "en-GB".to_string()));
    }

    Err("No en-us or en-gb ESD found in downloads directory".to_string())
}

fn build_unattend(language: &str) -> UnattendConfig {
    let input_locale = match language {
        "en-GB" => "0809:00000809",
        _ => "0409:00000409",
    };

    UnattendConfig {
        language: language.to_string(),
        input_locale: input_locale.to_string(),
        timezone: "Pacific Standard Time".to_string(),
        oobe: OobeConfig {
            skip_machine_oobe: true,
            skip_user_oobe: true,
            hide_eula: true,
            hide_wireless_setup: true,
            hide_local_account_screen: false,
            hide_online_account_screens: true,
            network_location: NetworkLocation::Work,
            protect_your_pc: ProtectYourPc::Recommended,
        },
        users: vec![UserAccountConfig {
            username: "steff".to_string(),
            password: "Steffan1".to_string(),
            display_name: Some("steff".to_string()),
            group: UserGroup::Administrators,
            password_never_expires: true,
            require_password_change: false,
        }],
        administrator_password: None,
        computer_name: None,
        product_key: None,
        domain_join: None,
        wifi_profile: None,
        auto_logon: None,
        first_logon_commands: vec![],
    }
}

fn build_task_sequence(embedded_installer_path: &Path) -> TaskSequence {
    let app_config = AppInstallConfig {
        copied_items: vec![],
        copy_destination: None,
        winget_packages: vec![WingetPackage {
            package_id: "Google.Chrome".to_string(),
            version: None,
            custom_args: None,
            enabled: true,
        }],
        chocolatey_packages: vec![],
        custom_installers: vec![
            CustomInstaller {
                name: "Embedded App".to_string(),
                path: embedded_installer_path.to_string_lossy().to_string(),
                source_type: InstallerSourceType::EmbeddedFile,
                source_file_name: None,
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "/qn /norestart".to_string(),
                installer_type: InstallerType::Msi,
                success_codes: vec![0, 3010],
                enabled: true,
            },
            CustomInstaller {
                name: "Network App".to_string(),
                path: r"\\deploy\apps\custom".to_string(),
                source_type: InstallerSourceType::NetworkDirectory,
                source_file_name: Some("setup.exe".to_string()),
                dependencies: vec![],
                dependency_destination: None,
                silent_args: "/quiet".to_string(),
                installer_type: InstallerType::Exe,
                success_codes: vec![0, 3010],
                enabled: true,
            },
        ],
        auto_install_chocolatey: true,
        continue_on_error: true,
        log_path: "C:\\BitOSDT\\Logs\\app-install.log".to_string(),
        progress_json_path: None,
    };

    TaskSequence {
        id: Uuid::new_v4(),
        name: "E2E Build Sequence".to_string(),
        tasks: vec![TaskDefinition {
            id: Uuid::new_v4(),
            name: "Install Applications".to_string(),
            task_type: TaskType::InstallApps(app_config),
            order: 10,
            enabled: true,
            continue_on_error: true,
            requires_reboot: false,
        }],
        settings: TaskSettings {
            scripts_dir: "C:\\BitOSDT\\Tasks".to_string(),
            logs_dir: "C:\\BitOSDT\\Logs".to_string(),
            continue_on_error: true,
            create_completion_marker: true,
        },
    }
}

fn first_script_executable_line(script: &str) -> Option<&str> {
    for line in script.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            return Some(trimmed);
        }
    }

    None
}

fn assert_powershell_script_parses(script_path: &Path) -> Result<(), String> {
    let escaped_path = script_path.to_string_lossy().replace('\'', "''");
    let parse_command = format!(
        "$tokens = $null; \
         $errors = $null; \
         [void][System.Management.Automation.Language.Parser]::ParseFile('{escaped_path}', [ref]$tokens, [ref]$errors); \
         if ($errors.Count -gt 0) {{ \
            $errors | ForEach-Object {{ \"Line $($_.Extent.StartLineNumber): $($_.Message)\" }}; \
            exit 1; \
         }}"
    );

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &parse_command,
        ])
        .output()
        .map_err(|e| format!("Failed to run PowerShell parser: {}", e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "PowerShell parser errors in {} (exit={:?})\nstdout:\n{}\nstderr:\n{}",
            script_path.display(),
            output.status.code(),
            stdout,
            stderr
        ));
    }

    Ok(())
}

fn cleanup_stale_dism_mount_state() -> Result<(), String> {
    let output = Command::new("dism")
        .arg("/Cleanup-Wim")
        .output()
        .map_err(|e| format!("Failed to run DISM cleanup: {}", e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "DISM cleanup failed (exit={:?})\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            stdout,
            stderr
        ));
    }

    Ok(())
}

fn unique_mount_dir(prefix: &str) -> PathBuf {
    PathBuf::from(format!(r"C:\BitOSDT\{}-{}", prefix, Uuid::new_v4()))
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| {
        format!(
            "failed to create destination directory {}: {}",
            destination.display(),
            e
        )
    })?;

    for entry in fs::read_dir(source).map_err(|e| {
        format!(
            "failed to list source directory {}: {}",
            source.display(),
            e
        )
    })? {
        let entry = entry.map_err(|e| format!("failed to read source directory entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = destination.join(entry.file_name());
        let metadata = entry
            .file_type()
            .map_err(|e| format!("failed to read file type for {}: {}", src_path.display(), e))?;

        if metadata.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

#[test]
#[ignore]
#[serial]
fn e2e_builds_full_iso_with_expected_customizations() {
    cleanup_stale_dism_mount_state().expect("failed to cleanup stale DISM mount state");

    let downloads_dir = resolve_downloads_dir().expect("failed to resolve downloads directory");
    let (source_esd, language) =
        pick_esd_source(&downloads_dir).expect("failed to pick source ESD");

    let output_iso = PathBuf::from(r"C:\BitOSDT\test.iso");
    let canonical_workspace = PathBuf::from(r"C:\BitOSDT\e2e-workspace");
    let mut workspace = canonical_workspace.clone();
    let mount_dir = unique_mount_dir("e2e-verify-mount");

    if output_iso.exists() {
        fs::remove_file(&output_iso).expect("failed to remove previous output ISO");
    }
    if canonical_workspace.exists() {
        if let Err(err) = remove_dir_all_with_force(
            &canonical_workspace,
            "failed to remove previous canonical workspace",
        ) {
            eprintln!(
                "Warning: {}. Falling back to isolated workspace for this run.",
                err
            );
            workspace = unique_mount_dir("e2e-workspace-fallback");
            if workspace.exists() {
                remove_dir_all_with_force(&workspace, "failed to remove fallback workspace")
                    .expect("failed to remove fallback workspace");
            }
        }
    }
    fs::create_dir_all(r"C:\BitOSDT").expect("failed to create C:\\BitOSDT");
    let embedded_installer_dir = PathBuf::from(r"C:\BitOSDT\e2e-installers");
    fs::create_dir_all(&embedded_installer_dir).expect("failed to create embedded installer dir");
    let embedded_installer = embedded_installer_dir.join("embedded-test.msi");
    fs::write(&embedded_installer, b"dummy msi payload")
        .expect("failed to create embedded installer payload");
    let winpe_packages_dir = PathBuf::from(r"C:\BitOSDT\e2e-winpe-packages");
    fs::create_dir_all(&winpe_packages_dir).expect("failed to create e2e winpe packages dir");

    let config = FullIsoBuildConfig {
        source_path: source_esd.clone(),
        output_path: output_iso.clone(),
        volume_label: "BITOSDT".to_string(),
        windows_version: "Windows 11".to_string(),
        windows_build: "25H2".to_string(),
        windows_edition: "Enterprise".to_string(),
        language: language.clone(),
        architecture: "amd64".to_string(),
        wim_index: 1,
        target_disk: None,
        disk_selection_policy: DiskSelectionPolicy::ConfigFirstSafeFallback,
        unattend: build_unattend(&language),
        autopilot: None,
        task_sequence: Some(build_task_sequence(&embedded_installer)),
        runtime_domain_join: None,
        workspace: Some(workspace.clone()),
        download_dir: Some(downloads_dir),
        adk_paths: None,
        winpe_assets_dir: None,
        winpe_packages_dir: Some(winpe_packages_dir),
        ui_dir: None,
        native_executable: None,
        common_boot_driver_dir: None,
        runtime_driver_catalog: Vec::new(),
        runtime_driver_cache_source: None,
        driver_paths: vec![],
        apply_drivers_to_offline_windows: true,
        runtime_driver_policy: RuntimeDriverPolicy::default(),
        unc_image_path: None,
        unc_auth_username: None,
        unc_auth_password: None,
        http_image_url: None,
        prompt_unc_credentials_at_runtime: None,
    };

    let result = build_full_iso(&config, |_| {}).expect("full ISO build failed");

    if workspace != canonical_workspace {
        let canonical_winpe = canonical_workspace.join("winpe");
        if canonical_winpe.exists() {
            let _ = remove_dir_all_with_force(
                &canonical_winpe,
                "failed to remove canonical winpe directory before publish",
            );
        }
        if fs::create_dir_all(&canonical_workspace).is_ok() {
            let _ = copy_dir_recursive(&result.winpe_dir, &canonical_winpe);
        }
    }

    assert!(result.output_path.exists(), "output ISO was not created");
    let iso_size = fs::metadata(&result.output_path)
        .expect("failed to stat output ISO")
        .len();
    assert!(iso_size > 0, "output ISO is empty");

    let mut mounted = MountGuard::new(mount_dir.clone());
    mounted
        .mount(&result.prepared_wim_path)
        .expect("failed to mount prepared WIM for verification");

    let unattend_path = mount_dir
        .join("Windows")
        .join("Panther")
        .join("unattend.xml");
    let unattend = fs::read_to_string(&unattend_path)
        .unwrap_or_else(|_| panic!("failed to read {}", unattend_path.display()));
    assert!(
        unattend.contains("<Name>steff</Name>"),
        "steff user missing"
    );
    assert!(
        unattend.contains("<Group>Administrators</Group>"),
        "steff is not in Administrators group"
    );
    assert!(
        unattend.contains("<UILanguage>en-US</UILanguage>")
            || unattend.contains("<UILanguage>en-GB</UILanguage>"),
        "unattend language is not en-US/en-GB"
    );

    let app_script_path = mount_dir
        .join("Windows")
        .join("Setup")
        .join("Scripts")
        .join("10_install_applications.ps1");
    let app_script = fs::read_to_string(&app_script_path)
        .unwrap_or_else(|_| panic!("failed to read {}", app_script_path.display()));
    assert!(
        app_script.contains("winget install --id \"Google.Chrome\""),
        "Google Chrome winget command not found"
    );
    assert!(
        app_script.contains(r"C:\BitOSDT\Installers\embedded-app-1.msi"),
        "embedded installer path not rewritten to staged runtime path"
    );
    assert!(
        app_script.contains("Install-NetworkApps.ps1"),
        "deferred network installer script was not generated"
    );
    assert!(
        app_script.contains("BitOSDTNetworkInstallers"),
        "RunOnce registration for deferred installers missing"
    );

    let staged_payload = mount_dir
        .join("BitOSDT")
        .join("Installers")
        .join("embedded-app-1.msi");
    assert!(
        staged_payload.exists(),
        "embedded installer payload not injected into mounted image at {}",
        staged_payload.display()
    );

    let scripts_dir = mount_dir.join("Windows").join("Setup").join("Scripts");
    for entry in fs::read_dir(&scripts_dir).expect("failed to list Scripts directory") {
        let entry = entry.expect("failed to read Scripts entry");
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        assert!(
            !name.contains("windows_update"),
            "Windows Update script should not be present when updates are disabled"
        );
    }

    let setup_complete_path = scripts_dir.join("SetupComplete.cmd");
    let setup_complete = fs::read_to_string(&setup_complete_path)
        .unwrap_or_else(|_| panic!("failed to read {}", setup_complete_path.display()));
    assert!(
        setup_complete.contains("%~dp0task-runner.ps1"),
        "SetupComplete should execute task-runner from its own directory"
    );
    assert!(
        !setup_complete.to_ascii_lowercase().contains("post-deploy"),
        "SetupComplete should not reference build workspace paths"
    );

    drop(mounted);

    let boot_mount_dir = unique_mount_dir("e2e-verify-boot-mount");
    let boot_wim = result
        .winpe_dir
        .join("media")
        .join("sources")
        .join("boot.wim");
    assert!(boot_wim.exists(), "boot.wim missing from WinPE media");

    let mut boot_mounted = MountGuard::new(boot_mount_dir.clone());
    boot_mounted
        .mount(&boot_wim)
        .expect("failed to mount boot.wim for verification");

    let startnet_path = boot_mount_dir
        .join("Windows")
        .join("System32")
        .join("startnet.cmd");
    let startnet = fs::read_to_string(&startnet_path)
        .unwrap_or_else(|_| panic!("failed to read {}", startnet_path.display()));
    assert!(
        startnet.contains("Deploy-FullIso.ps1"),
        "startnet.cmd does not invoke Deploy-FullIso.ps1"
    );
    assert!(
        startnet.contains("Launch-BitOSDT-WinPE.cmd"),
        "startnet.cmd does not hand off through Launch-BitOSDT-WinPE.cmd"
    );
    assert!(
        startnet.contains("set STARTNET_LOG=X:\\BitOSDT\\Logs\\startnet.log"),
        "startnet.cmd does not configure startnet diagnostic logging"
    );
    assert!(
        startnet.contains("Invoking shell wrapper \"%WRAPPER%\""),
        "startnet.cmd does not emit wrapper invocation breadcrumbs"
    );
    assert!(
        startnet.contains("Shell wrapper missing at"),
        "startnet.cmd does not emit shell wrapper missing diagnostics"
    );

    let hta_path = boot_mount_dir
        .join("BitOSDT")
        .join("UI")
        .join("BitOSDT-Deploy.hta");
    assert!(
        hta_path.exists(),
        "WinPE HTA shell missing at {}",
        hta_path.display()
    );
    let hta = fs::read_to_string(&hta_path)
        .unwrap_or_else(|_| panic!("failed to read {}", hta_path.display()));
    assert!(
        hta.contains("<HTA:APPLICATION"),
        "BitOSDT-Deploy.hta missing HTA application markup"
    );

    let winpeshl_path = boot_mount_dir
        .join("Windows")
        .join("System32")
        .join("winpeshl.ini");
    assert!(
        winpeshl_path.exists(),
        "winpeshl.ini missing at {}",
        winpeshl_path.display()
    );
    let winpeshl = fs::read_to_string(&winpeshl_path)
        .unwrap_or_else(|_| panic!("failed to read {}", winpeshl_path.display()));
    assert!(
        winpeshl.contains("startnet.cmd"),
        "winpeshl.ini should launch startnet.cmd"
    );
    assert!(
        !winpeshl.contains("Launch-BitOSDT-WinPE.cmd"),
        "winpeshl.ini should not directly launch Launch-BitOSDT-WinPE.cmd"
    );

    let shell_wrapper_path = boot_mount_dir
        .join("BitOSDT")
        .join("Scripts")
        .join("Launch-BitOSDT-WinPE.cmd");
    let shell_wrapper = fs::read_to_string(&shell_wrapper_path)
        .unwrap_or_else(|_| panic!("failed to read {}", shell_wrapper_path.display()));
    assert!(
        shell_wrapper.contains("start \"\" \"%MSHTA_EXE%\" \"%HTA%\""),
        "Launch-BitOSDT-WinPE.cmd should launch mshta with HTA path"
    );
    assert!(
        shell_wrapper.contains(
            "echo Failed to launch HTA shell. Exit=!HTA_EXIT!. Keeping console fallback visible."
        ),
        "Launch-BitOSDT-WinPE.cmd missing safe HTA failure message"
    );
    assert!(
        !shell_wrapper.contains(
            "echo Failed to launch HTA shell (exit=%HTA_EXIT%). Keeping console fallback visible."
        ),
        "Launch-BitOSDT-WinPE.cmd contains unsafe HTA failure message that breaks cmd parser"
    );

    let deploy_script_path = boot_mount_dir
        .join("BitOSDT")
        .join("Scripts")
        .join("Deploy-FullIso.ps1");
    assert!(
        deploy_script_path.exists(),
        "WinPE deploy script missing at {}",
        deploy_script_path.display()
    );
    let deploy_script = fs::read_to_string(&deploy_script_path)
        .unwrap_or_else(|_| panic!("failed to read {}", deploy_script_path.display()));
    let invalid_exe_interpolation = ["$Ex", "e:"].concat();
    assert_powershell_script_parses(&deploy_script_path)
        .unwrap_or_else(|e| panic!("Deploy-FullIso.ps1 parse validation failed: {}", e));
    assert_eq!(
        first_script_executable_line(&deploy_script),
        Some("param("),
        "Deploy-FullIso.ps1 should start with a top-level param block"
    );
    assert!(
        deploy_script.contains("Resolve-WindowsImagePath"),
        "Deploy-FullIso.ps1 should resolve image path dynamically"
    );
    assert!(
        deploy_script.contains("\\sources\\install.wim"),
        "Deploy-FullIso.ps1 should include install.wim candidate path"
    );
    assert!(
        deploy_script.contains("\\sources\\install.esd"),
        "Deploy-FullIso.ps1 should include install.esd candidate path"
    );
    assert!(
        deploy_script.contains(
            "Invoke-Logged -Exe \"diskpart.exe\" -ArgumentList @(\"/s\", $diskpartScriptPath) -TimeoutSeconds 300"
        ),
        "Deploy-FullIso.ps1 should run diskpart with explicit timeout"
    );
    assert!(
        deploy_script.contains("function Write-ScriptLinesToLog"),
        "Deploy-FullIso.ps1 should include script-line diagnostic logging helper"
    );
    assert!(
        deploy_script.contains("Diskpart script contents:"),
        "Deploy-FullIso.ps1 should log diskpart script contents for troubleshooting"
    );
    assert!(
        deploy_script.contains("runtime-drivers"),
        "Deploy-FullIso.ps1 should invoke native runtime driver staging"
    );
    assert!(
        deploy_script.contains("runtime-drivers.json"),
        "Deploy-FullIso.ps1 should reference runtime driver config"
    );
    assert!(
        deploy_script.contains("Unable to stop timed-out process ${Exe}: $_"),
        "Deploy-FullIso.ps1 should delimit Exe variable before colon"
    );
    assert!(
        !deploy_script.contains(&invalid_exe_interpolation),
        "Deploy-FullIso.ps1 should not contain invalid '$Exe:' interpolation"
    );

    let deploy_config_path = boot_mount_dir
        .join("BitOSDT")
        .join("Config")
        .join("deploy.json");
    let deploy_config = fs::read_to_string(&deploy_config_path)
        .unwrap_or_else(|_| panic!("failed to read {}", deploy_config_path.display()));
    assert!(
        deploy_config.contains("\"wim_index\": 1"),
        "deploy config did not contain expected wim_index"
    );
    assert!(
        deploy_config.contains("config_first_safe_fallback"),
        "deploy config did not contain expected disk selection policy"
    );

    let runtime_driver_config_path = boot_mount_dir
        .join("BitOSDT")
        .join("Config")
        .join("runtime-drivers.json");
    let runtime_driver_config = fs::read_to_string(&runtime_driver_config_path)
        .unwrap_or_else(|_| panic!("failed to read {}", runtime_driver_config_path.display()));
    assert!(
        runtime_driver_config.contains("\"runtime_driver_policy\""),
        "runtime driver config missing runtime_driver_policy"
    );
    assert!(
        runtime_driver_config.contains("\"os_version\": \"25H2\""),
        "runtime driver config missing expected os_version"
    );
}
