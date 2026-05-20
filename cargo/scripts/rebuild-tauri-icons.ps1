param(
  [string]$InputPath = "src-tauri/icons/icon-new.png",
  [string]$OutputSource = "src-tauri/icons/app-icon-transparent.png",
  [int]$HardTolerance = 12,
  [int]$SoftTolerance = 38
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($SoftTolerance -le $HardTolerance) {
  throw "SoftTolerance must be greater than HardTolerance."
}

Add-Type -AssemblyName System.Drawing

function Get-MatteColor {
  param([System.Drawing.Bitmap]$Bitmap)

  $maxX = [int]$Bitmap.Width - 1
  $maxY = [int]$Bitmap.Height - 1
  $coords = @(
    @(0, 0),
    @($maxX, 0),
    @(0, $maxY),
    @($maxX, $maxY)
  )

  $r = 0
  $g = 0
  $b = 0
  foreach ($coord in $coords) {
    $pixel = $Bitmap.GetPixel($coord[0], $coord[1])
    $r += $pixel.R
    $g += $pixel.G
    $b += $pixel.B
  }

  return [System.Drawing.Color]::FromArgb(
    [int]($r / $coords.Count),
    [int]($g / $coords.Count),
    [int]($b / $coords.Count)
  )
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$absoluteInput = (Resolve-Path (Join-Path $repoRoot $InputPath)).Path
$absoluteOutput = Join-Path $repoRoot $OutputSource

if (!(Test-Path -LiteralPath $absoluteInput)) {
  throw "Input icon not found: $absoluteInput"
}

$targetSize = 1024
$source = [System.Drawing.Bitmap]::new($absoluteInput)
$prepared = New-Object System.Drawing.Bitmap($targetSize, $targetSize, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$graphics = [System.Drawing.Graphics]::FromImage($prepared)
$graphics.Clear([System.Drawing.Color]::FromArgb(0, 0, 0, 0))
$graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
$graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
$graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$graphics.DrawImage($source, 0, 0, $targetSize, $targetSize)
$graphics.Dispose()
$source.Dispose()

$matteColor = Get-MatteColor -Bitmap $prepared

for ($x = 0; $x -lt $prepared.Width; $x++) {
  for ($y = 0; $y -lt $prepared.Height; $y++) {
    $pixel = $prepared.GetPixel($x, $y)
    $distance = [Math]::Sqrt(
      [Math]::Pow(($pixel.R - $matteColor.R), 2) +
      [Math]::Pow(($pixel.G - $matteColor.G), 2) +
      [Math]::Pow(($pixel.B - $matteColor.B), 2)
    )

    $newAlpha = $pixel.A
    if ($distance -le $HardTolerance) {
      $newAlpha = 0
    } elseif ($distance -lt $SoftTolerance) {
      $fade = ($distance - $HardTolerance) / ($SoftTolerance - $HardTolerance)
      $newAlpha = [int][Math]::Round($pixel.A * $fade)
    }

    if ($newAlpha -ne $pixel.A) {
      if ($newAlpha -le 0) {
        $prepared.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(0, 0, 0, 0))
      } else {
        $prepared.SetPixel($x, $y, [System.Drawing.Color]::FromArgb($newAlpha, $pixel.R, $pixel.G, $pixel.B))
      }
    }
  }
}

$outputDir = Split-Path -Parent $absoluteOutput
if (!(Test-Path -LiteralPath $outputDir)) {
  New-Item -ItemType Directory -Path $outputDir | Out-Null
}

$prepared.Save($absoluteOutput, [System.Drawing.Imaging.ImageFormat]::Png)
$prepared.Dispose()

Write-Host "Transparent icon source written to: $absoluteOutput"

Push-Location $repoRoot
try {
  & cmd.exe /c "npm run tauri -- icon `"$OutputSource`" --output src-tauri/icons"
} finally {
  Pop-Location
}
