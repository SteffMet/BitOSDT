param(
    [string]$PreviewRoot = "C:\BitOSDT\ProvisioningPreview",
    [ValidateSet("Basic", "WifiAndDomain", "AppsHeavy", "RaceyFirstLogon")]
    [string]$Scenario = "AppsHeavy",
    [switch]$NoLaunch
)

$ErrorActionPreference = "Stop"

function Resolve-HtaTemplate {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourcePath
    )

    if (-not (Test-Path -LiteralPath $SourcePath)) {
        throw "Could not find provisioning UI source at: $SourcePath"
    }

    $source = Get-Content -LiteralPath $SourcePath -Raw
    $rawStart = $source.IndexOf('r##"<html>')
    if ($rawStart -lt 0) {
        throw "Unable to locate HTA template block in: $SourcePath"
    }

    $htmlStart = $source.IndexOf("<html>", $rawStart)
    $htmlEnd = $source.IndexOf("</html>", $htmlStart)
    if ($htmlStart -lt 0 -or $htmlEnd -lt 0) {
        throw "Unable to extract provisioning HTA template in: $SourcePath"
    }

    return $source.Substring($htmlStart, ($htmlEnd + "</html>".Length) - $htmlStart)
}

function To-JsStringLiteral {
    param([Parameter(Mandatory = $true)][string]$Value)
    return '"' + ($Value -replace '\\', '\\\\') + '"'
}

function New-ScenarioData {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    switch ($Name) {
        "Basic" {
            return @{
                profile = [ordered]@{
                    schemaVersion = 1
                    name = "Preview Basic"
                    description = "Computer naming only"
                    language = "en-US"
                    inputLocale = "0409:00000409"
                    timezone = "Pacific Standard Time"
                    skipMachineOobe = $true
                    skipUserOobe = $true
                    hideEula = $true
                    hidePrivacySettings = $true
                    hideWirelessSetup = $true
                    hideOnlineAccountScreens = $true
                    defaultUserEnabled = $false
                    promptForComputerName = $true
                    explicitComputerName = ""
                    wifiEnabled = $false
                    wifiSsid = ""
                    domainJoinEnabled = $false
                    domainName = ""
                    appItemCount = 0
                    debloatEnabled = $false
                    customScriptCount = 0
                }
                state = [ordered]@{
                    schemaVersion = 1
                    currentStepId = "computerName"
                    completedStepIds = @()
                    restartChoices = [ordered]@{ computerName = $true }
                    computerName = "ENG-LT-001"
                    inProgress = $false
                    rebootPending = $false
                    errorMessage = $null
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                status = [ordered]@{
                    schemaVersion = 1
                    terminalStatus = "idle"
                    percentComplete = 0
                    bannerMessage = ""
                    errorMessage = $null
                    tasks = @(
                        [ordered]@{ id = "computerName"; title = "Computer Name"; status = "active"; detail = "Waiting for operator input" }
                    )
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                appProgress = [ordered]@{
                    schemaVersion = 1
                    currentItem = ""
                    state = "idle"
                    completedCount = 0
                    totalCount = 0
                    message = ""
                    updatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
            }
        }
        "WifiAndDomain" {
            return @{
                profile = [ordered]@{
                    schemaVersion = 1
                    name = "Preview Branch Office"
                    description = "Rename, Wi-Fi, and domain join"
                    language = "en-GB"
                    inputLocale = "0809:00000809"
                    timezone = "GMT Standard Time"
                    skipMachineOobe = $true
                    skipUserOobe = $true
                    hideEula = $true
                    hidePrivacySettings = $true
                    hideWirelessSetup = $false
                    hideOnlineAccountScreens = $true
                    defaultUserEnabled = $true
                    promptForComputerName = $true
                    explicitComputerName = ""
                    wifiEnabled = $true
                    wifiSsid = "BranchOffice"
                    domainJoinEnabled = $true
                    domainName = "contoso.local"
                    appItemCount = 0
                    debloatEnabled = $false
                    customScriptCount = 0
                }
                state = [ordered]@{
                    schemaVersion = 1
                    currentStepId = "domainJoin"
                    completedStepIds = @("computerName", "wifi")
                    restartChoices = [ordered]@{ computerName = $true; wifi = $false; domainJoin = $true }
                    computerName = "BRNCH-WS-042"
                    inProgress = $false
                    rebootPending = $false
                    errorMessage = $null
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                status = [ordered]@{
                    schemaVersion = 1
                    terminalStatus = "idle"
                    percentComplete = 66
                    bannerMessage = ""
                    errorMessage = $null
                    tasks = @(
                        [ordered]@{ id = "computerName"; title = "Computer Name"; status = "complete"; detail = "Renamed to BRNCH-WS-042" }
                        [ordered]@{ id = "wifi"; title = "Wi-Fi Settings"; status = "complete"; detail = "Connected to BranchOffice" }
                        [ordered]@{ id = "domainJoin"; title = "Domain Join"; status = "active"; detail = "Ready to join contoso.local" }
                    )
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                appProgress = [ordered]@{
                    schemaVersion = 1
                    currentItem = ""
                    state = "idle"
                    completedCount = 0
                    totalCount = 0
                    message = ""
                    updatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
            }
        }
        "RaceyFirstLogon" {
            return @{
                profile = [ordered]@{
                    schemaVersion = 1
                    name = "Preview Racey First Logon"
                    description = "Simulates missing files and delayed controller activity"
                    language = "en-US"
                    inputLocale = "0409:00000409"
                    timezone = "Pacific Standard Time"
                    skipMachineOobe = $true
                    skipUserOobe = $true
                    hideEula = $true
                    hidePrivacySettings = $true
                    hideWirelessSetup = $true
                    hideOnlineAccountScreens = $true
                    defaultUserEnabled = $true
                    promptForComputerName = $true
                    explicitComputerName = ""
                    wifiEnabled = $true
                    wifiSsid = "CorpWiFi"
                    domainJoinEnabled = $true
                    domainName = "corp.contoso.local"
                    appItemCount = 4
                    debloatEnabled = $true
                    customScriptCount = 2
                }
                state = [ordered]@{
                    schemaVersion = 1
                    currentStepId = "computerName"
                    completedStepIds = @()
                    restartChoices = [ordered]@{ computerName = $true; wifi = $false; domainJoin = $true; apps = $false; optionalScripts = $false }
                    computerName = "ENG-LT-RACE"
                    inProgress = $false
                    rebootPending = $false
                    errorMessage = $null
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                status = [ordered]@{
                    schemaVersion = 1
                    terminalStatus = "idle"
                    percentComplete = 0
                    bannerMessage = "Preview race simulator will disturb the local files."
                    errorMessage = $null
                    tasks = @(
                        [ordered]@{ id = "computerName"; title = "Computer Name"; status = "active"; detail = "Waiting for operator input" }
                        [ordered]@{ id = "wifi"; title = "Wi-Fi Settings"; status = "pending"; detail = "Waiting" }
                        [ordered]@{ id = "domainJoin"; title = "Domain Join"; status = "pending"; detail = "Waiting" }
                        [ordered]@{ id = "apps"; title = "Applications"; status = "pending"; detail = "Waiting" }
                        [ordered]@{ id = "optionalScripts"; title = "Custom Actions"; status = "pending"; detail = "Waiting" }
                    )
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                appProgress = [ordered]@{
                    schemaVersion = 1
                    currentItem = ""
                    state = "idle"
                    completedCount = 0
                    totalCount = 4
                    message = ""
                    updatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
            }
        }
        default {
            return @{
                profile = [ordered]@{
                    schemaVersion = 1
                    name = "Preview Apps Heavy"
                    description = "Full provisioning flow with app progress"
                    language = "en-US"
                    inputLocale = "0409:00000409"
                    timezone = "Pacific Standard Time"
                    skipMachineOobe = $true
                    skipUserOobe = $true
                    hideEula = $true
                    hidePrivacySettings = $true
                    hideWirelessSetup = $true
                    hideOnlineAccountScreens = $true
                    defaultUserEnabled = $true
                    promptForComputerName = $true
                    explicitComputerName = ""
                    wifiEnabled = $true
                    wifiSsid = "CorpWiFi"
                    domainJoinEnabled = $true
                    domainName = "corp.contoso.local"
                    appItemCount = 4
                    debloatEnabled = $true
                    customScriptCount = 2
                }
                state = [ordered]@{
                    schemaVersion = 1
                    currentStepId = "apps"
                    completedStepIds = @("computerName", "wifi", "domainJoin")
                    restartChoices = [ordered]@{ computerName = $true; wifi = $false; domainJoin = $true; apps = $false; optionalScripts = $false }
                    computerName = "ENG-LT-104"
                    inProgress = $false
                    rebootPending = $false
                    errorMessage = $null
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                status = [ordered]@{
                    schemaVersion = 1
                    terminalStatus = "idle"
                    percentComplete = 60
                    bannerMessage = ""
                    errorMessage = $null
                    tasks = @(
                        [ordered]@{ id = "computerName"; title = "Computer Name"; status = "complete"; detail = "Renamed to ENG-LT-104" }
                        [ordered]@{ id = "wifi"; title = "Wi-Fi Settings"; status = "complete"; detail = "Connected to CorpWiFi" }
                        [ordered]@{ id = "domainJoin"; title = "Domain Join"; status = "complete"; detail = "Joined corp.contoso.local" }
                        [ordered]@{ id = "apps"; title = "Applications"; status = "active"; detail = "Installing packaged software" }
                        [ordered]@{ id = "optionalScripts"; title = "Custom Actions"; status = "pending"; detail = "Waiting" }
                    )
                    lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
                appProgress = [ordered]@{
                    schemaVersion = 1
                    currentItem = "Microsoft.VisualStudioCode"
                    state = "active"
                    completedCount = 1
                    totalCount = 4
                    message = "Installing Microsoft.VisualStudioCode"
                    updatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
                }
            }
        }
    }
}

function Write-ScenarioFiles {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable]$ScenarioData,
        [Parameter(Mandatory = $true)]
        [string]$StateDir
    )

    New-Item -Path $StateDir -ItemType Directory -Force | Out-Null
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)

    [System.IO.File]::WriteAllText((Join-Path $StateDir "profile.json"), ($ScenarioData.profile | ConvertTo-Json -Depth 8), $utf8NoBom)
    [System.IO.File]::WriteAllText((Join-Path $StateDir "ui-state.json"), ($ScenarioData.state | ConvertTo-Json -Depth 8), $utf8NoBom)
    [System.IO.File]::WriteAllText((Join-Path $StateDir "task-status.json"), ($ScenarioData.status | ConvertTo-Json -Depth 8), $utf8NoBom)
    [System.IO.File]::WriteAllText((Join-Path $StateDir "app-progress.json"), ($ScenarioData.appProgress | ConvertTo-Json -Depth 8), $utf8NoBom)
}

function Write-PreviewController {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ControllerPath,
        [Parameter(Mandatory = $true)]
        [string]$StateDir,
        [Parameter(Mandatory = $true)]
        [string]$Scenario
    )

    $controller = @"
param([string]`$Action = 'ProcessCommand')
`$ErrorActionPreference = 'Stop'
`$stateDir = '$($StateDir.Replace("'", "''"))'
`$statePath = Join-Path `$stateDir 'ui-state.json'
`$statusPath = Join-Path `$stateDir 'task-status.json'
`$commandPath = Join-Path `$stateDir 'command.json'
`$appProgressPath = Join-Path `$stateDir 'app-progress.json'
`$scenario = '$($Scenario.Replace("'", "''"))'

function Read-Json([string]`$path) {
    Get-Content -LiteralPath `$path -Raw | ConvertFrom-Json
}

function Normalize-RestartChoices(`$value) {
    if (`$null -eq `$value) {
        return [ordered]@{}
    }
    if (`$value -is [System.Collections.IDictionary]) {
        return `$value
    }

    `$normalized = [ordered]@{}
    foreach (`$property in `$value.PSObject.Properties) {
        `$normalized[[string]`$property.Name] = [bool]`$property.Value
    }
    return `$normalized
}

function Normalize-PreviewState(`$state, `$status) {
    if (`$null -ne `$state) {
        `$state.completedStepIds = @(`$state.completedStepIds)
        `$state.restartChoices = Normalize-RestartChoices `$state.restartChoices
    }
    if (`$null -ne `$status) {
        `$status.tasks = @(`$status.tasks)
    }
}

function Write-Json([string]`$path, `$value) {
    `$value.lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    `$value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath `$path -Encoding UTF8
}

function Set-Task([object]`$status, [string]`$taskId, [string]`$taskState, [string]`$detail) {
    foreach (`$task in `$status.tasks) {
        if (`$task.id -eq `$taskId) {
            `$task.status = `$taskState
            `$task.detail = `$detail
        } elseif (`$taskState -eq 'active' -and `$task.status -eq 'active') {
            `$task.status = 'pending'
        }
    }
}

if (-not (Test-Path -LiteralPath `$commandPath)) { exit 0 }
`$command = Read-Json `$commandPath
Remove-Item -LiteralPath `$commandPath -Force -ErrorAction SilentlyContinue
`$state = Read-Json `$statePath
`$status = Read-Json `$statusPath
Normalize-PreviewState `$state `$status

if (`$scenario -eq 'RaceyFirstLogon') {
    Start-Sleep -Milliseconds 1800
}

`$taskId = [string]`$command.stepId
`$state.inProgress = `$true
Write-Json `$statePath `$state
Set-Task `$status `$taskId 'active' 'Preview applying step'
`$status.terminalStatus = 'running'
Write-Json `$statusPath `$status

switch (`$taskId) {
    'computerName' {
        Start-Sleep -Milliseconds 350
        `$state.computerName = if ([string]::IsNullOrWhiteSpace(`$command.computerName)) { 'PREVIEW-PC' } else { `$command.computerName }
        if (`$state.completedStepIds -notcontains 'computerName') { `$state.completedStepIds += 'computerName' }
        Set-Task `$status 'computerName' 'complete' ("Renamed to " + `$state.computerName)
        `$state.currentStepId = if (`$status.tasks | Where-Object { `$_.id -eq 'wifi' }) { 'wifi' } elseif (`$status.tasks | Where-Object { `$_.id -eq 'apps' }) { 'apps' } else { 'complete' }
    }
    'wifi' {
        Start-Sleep -Milliseconds 350
        if (`$state.completedStepIds -notcontains 'wifi') { `$state.completedStepIds += 'wifi' }
        Set-Task `$status 'wifi' 'complete' 'Wi-Fi profile applied'
        `$state.currentStepId = if (`$status.tasks | Where-Object { `$_.id -eq 'domainJoin' }) { 'domainJoin' } else { 'complete' }
    }
    'domainJoin' {
        Start-Sleep -Milliseconds 350
        if (`$state.completedStepIds -notcontains 'domainJoin') { `$state.completedStepIds += 'domainJoin' }
        Set-Task `$status 'domainJoin' 'complete' 'Joined preview domain'
        `$state.currentStepId = if (`$status.tasks | Where-Object { `$_.id -eq 'apps' }) { 'apps' } elseif (`$status.tasks | Where-Object { `$_.id -eq 'optionalScripts' }) { 'optionalScripts' } else { 'complete' }
    }
    'apps' {
        `$items = @('Microsoft.VisualStudioCode', 'Google.Chrome', '7zip.7zip', 'Contoso VPN')
        for (`$i = 0; `$i -lt `$items.Count; `$i++) {
            `$progress = [ordered]@{
                schemaVersion = 1
                currentItem = `$items[`$i]
                state = 'active'
                completedCount = `$i
                totalCount = `$items.Count
                message = ('Installing ' + `$items[`$i])
                updatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
            }
            `$progress | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath `$appProgressPath -Encoding UTF8
            Start-Sleep -Milliseconds 350
            `$progress.completedCount = `$i + 1
            `$progress.state = 'complete'
            `$progress.message = ('Installed ' + `$items[`$i])
            `$progress | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath `$appProgressPath -Encoding UTF8
        }
        if (`$state.completedStepIds -notcontains 'apps') { `$state.completedStepIds += 'apps' }
        Set-Task `$status 'apps' 'complete' 'Preview app installation finished'
        `$state.currentStepId = if (`$status.tasks | Where-Object { `$_.id -eq 'optionalScripts' }) { 'optionalScripts' } else { 'complete' }
    }
    'optionalScripts' {
        Start-Sleep -Milliseconds 350
        if (`$state.completedStepIds -notcontains 'optionalScripts') { `$state.completedStepIds += 'optionalScripts' }
        Set-Task `$status 'optionalScripts' 'complete' 'Debloat and custom scripts complete'
        `$state.currentStepId = 'complete'
    }
}

`$state.inProgress = `$false
`$state.rebootPending = [bool]`$command.restartNow
`$state.errorMessage = `$null

if (`$state.currentStepId -eq 'complete') {
    `$status.terminalStatus = 'complete'
    `$status.percentComplete = 100
    `$status.bannerMessage = 'Preview finished successfully.'
} else {
    `$status.terminalStatus = 'idle'
    `$status.bannerMessage = if (`$command.restartNow) { 'Preview restart selected. The real provisioning flow would relaunch after sign-in.' } else { '' }
    `$done = @(`$status.tasks | Where-Object { `$_.status -eq 'complete' -or `$_.status -eq 'reboot_pending' }).Count
    `$status.percentComplete = [Math]::Round((`$done / [Math]::Max(@(`$status.tasks).Count, 1)) * 100)
    Set-Task `$status `$state.currentStepId 'active' 'Ready for preview step'
}

Write-Json `$statePath `$state
Write-Json `$statusPath `$status
"@

    Set-Content -LiteralPath $ControllerPath -Value $controller -Encoding UTF8
}

function Write-RaceConditionSimulator {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SimulatorPath,
        [Parameter(Mandatory = $true)]
        [string]$StateDir,
        [Parameter(Mandatory = $true)]
        [string]$LogPath
    )

    $simulator = @"
param()
`$ErrorActionPreference = 'Stop'
`$stateDir = '$($StateDir.Replace("'", "''"))'
`$logPath = '$($LogPath.Replace("'", "''"))'
`$profilePath = Join-Path `$stateDir 'profile.json'
`$statePath = Join-Path `$stateDir 'ui-state.json'
`$statusPath = Join-Path `$stateDir 'task-status.json'
`$appProgressPath = Join-Path `$stateDir 'app-progress.json'

function Write-PreviewLog([string]`$message) {
    `$line = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') [SIM] `$message"
    `$line | Out-File -FilePath `$logPath -Encoding utf8 -Append
}

function Read-Json([string]`$path) {
    Get-Content -LiteralPath `$path -Raw | ConvertFrom-Json
}

function Normalize-PreviewStatus(`$status) {
    if (`$null -ne `$status) {
        `$status.tasks = @(`$status.tasks)
    }
}

function Write-Json([string]`$path, `$value) {
    `$value.lastUpdatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    `$value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath `$path -Encoding UTF8
}

function Write-Raw([string]`$path, [string]`$content) {
    Set-Content -LiteralPath `$path -Value `$content -Encoding UTF8
}

Write-PreviewLog 'Race simulator started.'
Start-Sleep -Milliseconds 900

if (Test-Path -LiteralPath `$statusPath) {
    `$statusBackup = Get-Content -LiteralPath `$statusPath -Raw
    Remove-Item -LiteralPath `$statusPath -Force
    Write-PreviewLog 'Removed task-status.json temporarily.'
    Start-Sleep -Milliseconds 1300
    Write-Raw `$statusPath `$statusBackup
    Write-PreviewLog 'Restored task-status.json.'
}

Start-Sleep -Milliseconds 650

if (Test-Path -LiteralPath `$statePath) {
    `$stateBackup = Get-Content -LiteralPath `$statePath -Raw
    Write-Raw `$statePath "{`"schemaVersion`":1,"
    Write-PreviewLog 'Wrote partial ui-state.json.'
    Start-Sleep -Milliseconds 850
    Write-Raw `$statePath `$stateBackup
    Write-PreviewLog 'Restored ui-state.json.'
}

for (`$i = 1; `$i -le 4; `$i++) {
    if (Test-Path -LiteralPath `$statusPath) {
        `$status = Read-Json `$statusPath
        Normalize-PreviewStatus `$status
        `$status.bannerMessage = "Preview churn pulse `$i"
        if (`$status.tasks.Count -gt 0) {
            `$status.tasks[0].detail = "Waiting for operator input (pulse `$i)"
        }
        Write-Json `$statusPath `$status
    }

    `$progress = [ordered]@{
        schemaVersion = 1
        currentItem = "Background pulse `$i"
        state = 'active'
        completedCount = [Math]::Min(`$i - 1, 3)
        totalCount = 4
        message = "Simulated background update `$i"
        updatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    }
    `$progress | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath `$appProgressPath -Encoding UTF8
    Start-Sleep -Milliseconds 500
}

Write-PreviewLog 'Race simulator finished.'
"@

    Set-Content -LiteralPath $SimulatorPath -Value $simulator -Encoding UTF8
}

$previewRootResolved = [System.IO.Path]::GetFullPath($PreviewRoot)
$stateDir = Join-Path $previewRootResolved "State"
$uiDir = Join-Path $previewRootResolved "UI"
$htaPath = Join-Path $uiDir "BitOSDT-Provisioning-Preview.hta"
$controllerPath = Join-Path $previewRootResolved "PreviewController.ps1"
$simulatorPath = Join-Path $previewRootResolved "PreviewRaceSimulator.ps1"
$heartbeatPath = Join-Path $stateDir "ui-heartbeat.json"
$logPath = Join-Path $previewRootResolved "preview-shell.log"

New-Item -Path $previewRootResolved -ItemType Directory -Force | Out-Null
New-Item -Path $uiDir -ItemType Directory -Force | Out-Null

$template = Resolve-HtaTemplate -SourcePath (Join-Path $PSScriptRoot "..\src\build\provisioning_ui.rs")
$template = $template.Replace("__PROFILE_PATH__", (To-JsStringLiteral (Join-Path $stateDir "profile.json")))
$template = $template.Replace("__STATE_PATH__", (To-JsStringLiteral (Join-Path $stateDir "ui-state.json")))
$template = $template.Replace("__STATUS_PATH__", (To-JsStringLiteral (Join-Path $stateDir "task-status.json")))
$template = $template.Replace("__APP_PROGRESS_PATH__", (To-JsStringLiteral (Join-Path $stateDir "app-progress.json")))
$template = $template.Replace("__COMMAND_PATH__", (To-JsStringLiteral (Join-Path $stateDir "command.json")))
$template = $template.Replace("__CONTROLLER_PATH__", (To-JsStringLiteral $controllerPath))
$template = $template.Replace("__HEARTBEAT_PATH__", (To-JsStringLiteral $heartbeatPath))
$template = $template.Replace("__SHELL_LOG_PATH__", (To-JsStringLiteral $logPath))

$scenarioData = New-ScenarioData -Name $Scenario
Write-ScenarioFiles -ScenarioData $scenarioData -StateDir $stateDir
Write-PreviewController -ControllerPath $controllerPath -StateDir $stateDir -Scenario $Scenario
if ($Scenario -eq "RaceyFirstLogon") {
    Write-RaceConditionSimulator -SimulatorPath $simulatorPath -StateDir $stateDir -LogPath $logPath
}
Set-Content -LiteralPath $htaPath -Value $template -Encoding UTF8

Write-Host "Preview assets written to: $previewRootResolved"
Write-Host "HTA: $htaPath"
Write-Host "Scenario: $Scenario"

if (-not $NoLaunch) {
    if ($Scenario -eq "RaceyFirstLogon") {
        Start-Process -FilePath "powershell.exe" -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File", $simulatorPath) -WindowStyle Hidden
    }
    Start-Process -FilePath "mshta.exe" -ArgumentList "`"$htaPath`""
}
