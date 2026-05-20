$ErrorActionPreference = "Stop"

$cargoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $cargoRoot
try {

$isoPath = "C:\BitOSDT\test.iso"
$outputDir = Split-Path $isoPath -Parent

if (-not (Test-Path $outputDir)) {
    New-Item -Path $outputDir -ItemType Directory -Force | Out-Null
}

if (Test-Path $isoPath) {
    Remove-Item -Path $isoPath -Force
}

Write-Host "Running local E2E full ISO build test..."
Write-Host "Cleaning stale DISM mount state..."
dism /Cleanup-Wim
if ($LASTEXITCODE -ne 0) {
    throw "DISM cleanup failed with exit code $LASTEXITCODE"
}

Write-Host "Running PowerShell template safety checks first..."
cargo test --test powershell_template_safety -- --nocapture
if ($LASTEXITCODE -ne 0) {
    throw "PowerShell template safety test failed with exit code $LASTEXITCODE"
}

cargo test --test e2e_full_iso_local -- --ignored --nocapture
if ($LASTEXITCODE -ne 0) {
    throw "E2E test failed with exit code $LASTEXITCODE"
}

if (-not (Test-Path $isoPath)) {
    throw "E2E test completed but output ISO was not found at $isoPath"
}

$iso = Get-Item $isoPath
Write-Host "E2E ISO build successful: $($iso.FullName)"
Write-Host "ISO size: $([math]::Round($iso.Length / 1GB, 2)) GB"
} finally {
    Pop-Location
}
