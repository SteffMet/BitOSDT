use crate::core::errors::BitOSDTResult;
use crate::tasks::{
    AppInstallConfig, AppInstaller, CustomScript, DomainJoinConfig, DomainJoinGenerator,
    ScriptGenerator, UserCreatorGenerator, UsersConfig, WindowsUpdateConfig,
    WindowsUpdateGenerator,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;
use uuid::Uuid;

/// Task runner that orchestrates all post-deployment tasks
pub struct TaskRunner;

/// Complete task configuration for post-deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSequence {
    /// Unique identifier
    pub id: Uuid,
    /// Sequence name
    pub name: String,
    /// Tasks in execution order
    pub tasks: Vec<TaskDefinition>,
    /// Global settings
    pub settings: TaskSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSettings {
    /// Base directory for scripts
    pub scripts_dir: String,
    /// Base directory for logs
    pub logs_dir: String,
    /// Continue execution on task failure
    pub continue_on_error: bool,
    /// Create completion marker when done
    pub create_completion_marker: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Task ID
    pub id: Uuid,
    /// Task name (display)
    pub name: String,
    /// Task type and configuration
    pub task_type: TaskType,
    /// Execution order (lower = earlier)
    pub order: u32,
    /// Task enabled
    pub enabled: bool,
    /// Continue sequence on this task's failure
    pub continue_on_error: bool,
    /// Reboot required after task
    pub requires_reboot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// Create local user accounts
    CreateUsers(UsersConfig),
    /// Join domain
    JoinDomain(DomainJoinConfig),
    /// Install applications
    InstallApps(AppInstallConfig),
    /// Run Windows Update
    WindowsUpdate(WindowsUpdateConfig),
    /// Run custom script
    CustomScript(CustomScript),
    /// Copy files
    CopyFiles(CopyFilesConfig),
    /// Set registry values
    SetRegistry(RegistryConfig),
    /// Reboot system
    Reboot(RebootConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyFilesConfig {
    /// Source path (can be URL)
    pub source: String,
    /// Destination path
    pub destination: String,
    /// Recursive copy
    pub recursive: bool,
    /// Overwrite existing
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Registry operations
    pub operations: Vec<RegistryOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryOperation {
    /// Key path (e.g., "HKLM:\SOFTWARE\MyApp")
    pub key: String,
    /// Value name
    pub name: String,
    /// Value data
    pub data: String,
    /// Value type
    pub value_type: RegistryValueType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegistryValueType {
    String,
    DWord,
    QWord,
    ExpandString,
    MultiString,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebootConfig {
    /// Delay before reboot (seconds)
    pub delay_seconds: u32,
    /// Message to display
    pub message: Option<String>,
    /// Force close applications
    pub force: bool,
}

impl Default for TaskSettings {
    fn default() -> Self {
        Self {
            scripts_dir: "C:\\BitOSDT\\Tasks".to_string(),
            logs_dir: "C:\\BitOSDT\\Logs".to_string(),
            continue_on_error: true,
            create_completion_marker: true,
        }
    }
}

impl TaskRunner {
    /// Generate all scripts and files for the task sequence
    pub fn generate_task_files(sequence: &TaskSequence) -> BitOSDTResult<HashMap<String, String>> {
        info!("Generating task files for sequence: {}", sequence.name);

        let mut files: HashMap<String, String> = HashMap::new();

        // Sort tasks by order
        let mut sorted_tasks: Vec<_> = sequence.tasks.iter().filter(|t| t.enabled).collect();
        sorted_tasks.sort_by_key(|t| t.order);

        // Generate individual task scripts
        for task in &sorted_tasks {
            let (filename, content) = Self::generate_task_script(task, &sequence.settings)?;
            files.insert(filename, content);
        }

        // Generate main task runner script
        let runner_script = Self::generate_runner_script(&sorted_tasks, &sequence.settings)?;
        files.insert("task-runner.ps1".to_string(), runner_script);

        // Generate SetupComplete.cmd
        let setup_complete = ScriptGenerator::generate_setup_complete_cmd(
            &sequence.settings.scripts_dir,
            &format!("{}\\setup-complete.log", sequence.settings.logs_dir),
        );
        files.insert("SetupComplete.cmd".to_string(), setup_complete);

        info!("Generated {} task files", files.len());
        Ok(files)
    }

    fn generate_task_script(
        task: &TaskDefinition,
        _settings: &TaskSettings,
    ) -> BitOSDTResult<(String, String)> {
        let filename = format!("{:02}_{}.ps1", task.order, sanitize_name(&task.name));

        let content = match &task.task_type {
            TaskType::CreateUsers(config) => UserCreatorGenerator::generate_script(config)?,
            TaskType::JoinDomain(config) => DomainJoinGenerator::generate_script(config)?,
            TaskType::InstallApps(config) => AppInstaller::generate_install_script(config)?,
            TaskType::WindowsUpdate(config) => WindowsUpdateGenerator::generate_script(config)?,
            TaskType::CustomScript(script) => {
                ScriptGenerator::wrap_powershell_script(&script.content, script.run_as_admin)
            }
            TaskType::CopyFiles(config) => Self::generate_copy_files_script(config),
            TaskType::SetRegistry(config) => Self::generate_registry_script(config),
            TaskType::Reboot(config) => Self::generate_reboot_script(config),
        };

        Ok((filename, content))
    }

    fn generate_runner_script(
        tasks: &[&TaskDefinition],
        settings: &TaskSettings,
    ) -> BitOSDTResult<String> {
        let mut script = format!(
            r#"# BitOSDT Task Runner
# Generated by BitOSDT 2.0
# ================================================

$ErrorActionPreference = "Continue"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$LogDir = "{logs_dir}"
$LogPath = Join-Path $LogDir "task-runner.log"

# Create directories
New-Item -Path $LogDir -ItemType Directory -Force | Out-Null

function Write-Log {{
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logLine = "$timestamp [$Level] $Message"
    $logLine | Out-File -Append $LogPath
    
    switch ($Level) {{
        "ERROR" {{ Write-Host $logLine -ForegroundColor Red }}
        "WARNING" {{ Write-Host $logLine -ForegroundColor Yellow }}
        "SUCCESS" {{ Write-Host $logLine -ForegroundColor Green }}
        default {{ Write-Host $logLine }}
    }}
}}

function Invoke-Task {{
    param(
        [string]$Name,
        [string]$ScriptPath,
        [bool]$ContinueOnError = $true
    )
    
    Write-Log "========================================"
    Write-Log "Starting task: $Name"
    Write-Log "Script: $ScriptPath"
    
    if (-not (Test-Path $ScriptPath)) {{
        Write-Log "Script not found: $ScriptPath" "ERROR"
        return $ContinueOnError
    }}
    
    try {{
        $startTime = Get-Date
        & $ScriptPath
        $exitCode = $LASTEXITCODE
        $duration = (Get-Date) - $startTime
        
        if ($exitCode -eq 0 -or $exitCode -eq 3010) {{
            Write-Log "Task completed successfully (Exit: $exitCode, Duration: $($duration.TotalSeconds)s)" "SUCCESS"
            return $true
        }} else {{
            Write-Log "Task completed with warnings (Exit: $exitCode, Duration: $($duration.TotalSeconds)s)" "WARNING"
            return $ContinueOnError
        }}
    }} catch {{
        Write-Log "Task failed with error: $_" "ERROR"
        return $ContinueOnError
    }}
}}

Write-Log "========================================"
Write-Log "BitOSDT Task Runner Starting"
Write-Log "Total tasks: {task_count}"
Write-Log "========================================"

$tasksCompleted = 0
$tasksFailed = 0
$rebootRequired = $false

"#,
            logs_dir = settings.logs_dir,
            task_count = tasks.len()
        );

        // Add task executions
        for task in tasks {
            let script_filename = format!("{:02}_{}.ps1", task.order, sanitize_name(&task.name));

            script.push_str(&format!(
                r#"
# Task {order}: {name}
$taskScript = Join-Path $ScriptDir "{filename}"
$success = Invoke-Task -Name "{name}" -ScriptPath $taskScript -ContinueOnError ${continue_on_error}

if ($success) {{
    $tasksCompleted++
}} else {{
    $tasksFailed++
    {failure_handling}
}}

{reboot_check}
"#,
                order = task.order,
                name = task.name,
                filename = script_filename,
                continue_on_error = if task.continue_on_error {
                    "true"
                } else {
                    "false"
                },
                failure_handling = if task.continue_on_error {
                    "# Continue on error"
                } else {
                    "Write-Log \"Stopping due to task failure\" \"ERROR\"\nbreak"
                },
                reboot_check = if task.requires_reboot {
                    "$rebootRequired = $true"
                } else {
                    ""
                }
            ));
        }

        // Add summary and completion
        script.push_str(&format!(
            r#"
# Summary
Write-Log "========================================"
Write-Log "Task Runner Complete"
Write-Log "Tasks completed: $tasksCompleted"
Write-Log "Tasks failed: $tasksFailed"
Write-Log "========================================"

{completion_marker}

if ($rebootRequired) {{
    Write-Log "A reboot is required to complete setup" "WARNING"
    # Uncomment to auto-reboot:
    # shutdown /r /t 60 /c "BitOSDT: Rebooting to complete setup"
}}

if ($tasksFailed -gt 0) {{
    exit 1
}} else {{
    exit 0
}}
"#,
            completion_marker = if settings.create_completion_marker {
                "# Create completion marker\n\"COMPLETE\" | Out-File -FilePath (Join-Path $ScriptDir \"bitosdt-complete.flag\") -Force".to_string()
            } else {
                String::new()
            }
        ));

        Ok(script)
    }

    fn generate_copy_files_script(config: &CopyFilesConfig) -> String {
        format!(
            r#"# BitOSDT Copy Files Task
$Source = "{source}"
$Destination = "{destination}"
$Recursive = ${recursive}
$Overwrite = ${overwrite}

Write-Host "Copying files from $Source to $Destination"

try {{
    # Check if source is URL
    if ($Source -match "^https?://") {{
        Write-Host "Downloading from URL..."
        Invoke-WebRequest -Uri $Source -OutFile $Destination -UseBasicParsing
    }} else {{
        # Local copy
        $copyParams = @{{
            Path = $Source
            Destination = $Destination
            Force = $Overwrite
        }}
        
        if ($Recursive) {{
            $copyParams['Recurse'] = $true
        }}
        
        Copy-Item @copyParams
    }}
    
    Write-Host "Copy completed successfully"
    exit 0
}} catch {{
    Write-Error "Copy failed: $_"
    exit 1
}}
"#,
            source = config.source,
            destination = config.destination,
            recursive = if config.recursive { "true" } else { "false" },
            overwrite = if config.overwrite { "true" } else { "false" }
        )
    }

    fn generate_registry_script(config: &RegistryConfig) -> String {
        let mut script = r#"# BitOSDT Registry Task
$ErrorActionPreference = "Stop"

Write-Host "Applying registry modifications..."

function Convert-BitOSDTRegistryValue {
    param(
        [string]$RawValue,
        [string]$ValueType
    )

    switch ($ValueType) {
        "String" { return [string]$RawValue }
        "ExpandString" { return [string]$RawValue }
        "DWord" {
            return [uint32]::Parse(
                $RawValue,
                [System.Globalization.NumberStyles]::Integer,
                [System.Globalization.CultureInfo]::InvariantCulture
            )
        }
        "QWord" {
            return [uint64]::Parse(
                $RawValue,
                [System.Globalization.NumberStyles]::Integer,
                [System.Globalization.CultureInfo]::InvariantCulture
            )
        }
        "MultiString" {
            $trimmed = if ($null -eq $RawValue) { "" } else { $RawValue.Trim() }
            if ($trimmed.StartsWith("[")) {
                try {
                    $jsonValue = $RawValue | ConvertFrom-Json -ErrorAction Stop
                    if ($jsonValue -is [System.Array]) {
                        return @($jsonValue | ForEach-Object { [string]$_ })
                    }
                } catch {
                }
            }

            if ([string]::IsNullOrWhiteSpace($RawValue)) {
                return @()
            }

            return @($RawValue -split "`r?`n" | Where-Object { $_ -ne "" })
        }
        "Binary" {
            $clean = (($RawValue -replace "0x", "") -replace "[^0-9A-Fa-f]", "")
            if ([string]::IsNullOrWhiteSpace($clean)) {
                return [byte[]]@()
            }
            if (($clean.Length % 2) -ne 0) {
                throw "Binary registry data must contain an even number of hex characters."
            }

            $bytes = New-Object byte[] ($clean.Length / 2)
            for ($index = 0; $index -lt $clean.Length; $index += 2) {
                $bytes[$index / 2] = [Convert]::ToByte($clean.Substring($index, 2), 16)
            }
            return $bytes
        }
        default {
            throw "Unsupported registry value type: $ValueType"
        }
    }
}

function Set-BitOSDTRegistryValue {
    param(
        [string]$KeyPath,
        [string]$Name,
        [string]$RawValue,
        [string]$ValueType
    )

    if (-not (Test-Path $KeyPath)) {
        New-Item -Path $KeyPath -Force | Out-Null
    }

    $convertedValue = Convert-BitOSDTRegistryValue -RawValue $RawValue -ValueType $ValueType
    $existingValue = Get-ItemProperty -Path $KeyPath -Name $Name -ErrorAction SilentlyContinue

    if ($null -eq $existingValue) {
        New-ItemProperty -Path $KeyPath -Name $Name -Value $convertedValue -PropertyType $ValueType -Force | Out-Null
    } else {
        Set-ItemProperty -Path $KeyPath -Name $Name -Value $convertedValue -Force
    }
}

"#
        .to_string();

        for op in &config.operations {
            let value_type = match op.value_type {
                RegistryValueType::String => "String",
                RegistryValueType::DWord => "DWord",
                RegistryValueType::QWord => "QWord",
                RegistryValueType::ExpandString => "ExpandString",
                RegistryValueType::MultiString => "MultiString",
                RegistryValueType::Binary => "Binary",
            };

            script.push_str(&format!(
                r#"
# Set: {key}\{name}
try {{
    Set-BitOSDTRegistryValue -KeyPath '{key}' -Name '{name}' -RawValue '{data}' -ValueType '{value_type}'
    Write-Host "Set: {key}\{name}"
}} catch {{
    Write-Error "Failed to set {key}\{name}: $_"
}}
"#,
                key = escape_powershell_single_quoted_string(&op.key),
                name = escape_powershell_single_quoted_string(&op.name),
                data = escape_powershell_single_quoted_string(&op.data),
                value_type = value_type
            ));
        }

        script.push_str(
            r#"
Write-Host "Registry modifications completed"
exit 0
"#,
        );

        script
    }

    fn generate_reboot_script(config: &RebootConfig) -> String {
        let message = config
            .message
            .as_ref()
            .map(|m| format!(" /c \"{}\"", m))
            .unwrap_or_default();

        let force = if config.force { " /f" } else { "" };

        format!(
            r#"# BitOSDT Reboot Task
Write-Host "System will reboot in {} seconds..."
shutdown /r /t {}{}{}"#,
            config.delay_seconds, config.delay_seconds, force, message
        )
    }

    /// Write task files to disk
    pub fn write_task_files(
        sequence: &TaskSequence,
        output_dir: &Path,
    ) -> BitOSDTResult<Vec<PathBuf>> {
        let files = Self::generate_task_files(sequence)?;
        let mut written_files = Vec::new();

        fs::create_dir_all(output_dir)?;

        for (filename, content) in files {
            let file_path = output_dir.join(&filename);
            fs::write(&file_path, &content)?;
            info!("Wrote task file: {:?}", &file_path);
            written_files.push(file_path);
        }

        Ok(written_files)
    }
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn escape_powershell_single_quoted_string(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{ScriptType, UserAccountConfig};

    #[test]
    fn test_generate_task_files() {
        let sequence = TaskSequence {
            id: Uuid::new_v4(),
            name: "Test Sequence".to_string(),
            tasks: vec![TaskDefinition {
                id: Uuid::new_v4(),
                name: "Create Admin User".to_string(),
                task_type: TaskType::CreateUsers(UsersConfig {
                    users: vec![UserAccountConfig {
                        username: "Admin".to_string(),
                        password: "P@ssw0rd".to_string(),
                        is_admin: true,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                order: 1,
                enabled: true,
                continue_on_error: true,
                requires_reboot: false,
            }],
            settings: TaskSettings::default(),
        };

        let files = TaskRunner::generate_task_files(&sequence).unwrap();

        assert!(files.contains_key("task-runner.ps1"));
        assert!(files.contains_key("SetupComplete.cmd"));
        assert!(files.len() >= 3); // runner, setup complete, and at least one task

        let setup_complete = files.get("SetupComplete.cmd").unwrap();
        assert!(setup_complete.contains("%~dp0task-runner.ps1"));

        let runner = files.get("task-runner.ps1").unwrap();
        assert!(runner.contains("Split-Path -Parent $MyInvocation.MyCommand.Path"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("My Task Name!"), "my_task_name_");
        assert_eq!(sanitize_name("install-apps"), "install-apps");
    }

    #[test]
    fn test_generate_task_files_includes_custom_script_tasks() {
        let sequence = TaskSequence {
            id: Uuid::new_v4(),
            name: "Custom Script Sequence".to_string(),
            tasks: vec![TaskDefinition {
                id: Uuid::new_v4(),
                name: "Run Hardening Script".to_string(),
                task_type: TaskType::CustomScript(CustomScript {
                    name: "Run Hardening Script".to_string(),
                    content: "Write-Host 'Hardening'".to_string(),
                    script_type: ScriptType::PowerShell,
                    run_as_admin: true,
                    continue_on_error: false,
                    timeout_seconds: 0,
                }),
                order: 30,
                enabled: true,
                continue_on_error: false,
                requires_reboot: false,
            }],
            settings: TaskSettings::default(),
        };

        let files = TaskRunner::generate_task_files(&sequence).unwrap();

        let custom_script = files
            .get("30_run_hardening_script.ps1")
            .expect("custom script file should be generated");
        assert!(custom_script.contains("Write-Host 'Hardening'"));
        assert!(custom_script.contains("administrator"));

        let runner = files
            .get("task-runner.ps1")
            .expect("task runner should be generated");
        assert!(runner.contains("Run Hardening Script"));
        assert!(runner.contains("30_run_hardening_script.ps1"));
    }

    #[test]
    fn test_generate_registry_script_uses_typed_registry_helpers() {
        let script = TaskRunner::generate_registry_script(&RegistryConfig {
            operations: vec![
                RegistryOperation {
                    key: r"HKLM:\SOFTWARE\Policies\Test".to_string(),
                    name: "EnableFeature".to_string(),
                    data: "1".to_string(),
                    value_type: RegistryValueType::DWord,
                },
                RegistryOperation {
                    key: r"HKLM:\SOFTWARE\Policies\Test".to_string(),
                    name: "AllowedHosts".to_string(),
                    data: "[\"a\",\"b\"]".to_string(),
                    value_type: RegistryValueType::MultiString,
                },
                RegistryOperation {
                    key: r"HKLM:\SOFTWARE\Policies\Test".to_string(),
                    name: "Blob".to_string(),
                    data: "DE AD BE EF".to_string(),
                    value_type: RegistryValueType::Binary,
                },
            ],
        });

        assert!(script.contains("function Convert-BitOSDTRegistryValue"));
        assert!(script.contains("New-ItemProperty -Path $KeyPath"));
        assert!(script
            .contains("Set-ItemProperty -Path $KeyPath -Name $Name -Value $convertedValue -Force"));
        assert!(script.contains("-ValueType 'DWord'"));
        assert!(script.contains("-ValueType 'MultiString'"));
        assert!(script.contains("-ValueType 'Binary'"));
        assert!(script.contains("ConvertFrom-Json"));
        assert!(
            script.contains("Binary registry data must contain an even number of hex characters.")
        );
    }
}
