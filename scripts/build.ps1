# Builds Earshot in release mode and packages dist\Earshot-v1.0.0-win-x64.zip
# Usage:  powershell -ExecutionPolicy Bypass -File scripts\build.ps1 [-NoTauriCli]
param([switch]$NoTauriCli)
$ErrorActionPreference = 'Stop'

$Root    = Split-Path -Parent $PSScriptRoot
$Tauri   = Join-Path $Root 'src-tauri'
$Dist    = Join-Path $Root 'dist'
$Release = 'Earshot-v1.0.0-win-x64.zip'
$Exe     = 'Earshot.exe'

Push-Location $Tauri
try {
    $hasCli = $false
    if (-not $NoTauriCli) {
        cargo tauri --version 2>$null | Out-Null
        $hasCli = ($LASTEXITCODE -eq 0)
    }
    if ($hasCli) {
        Write-Host '==> cargo tauri build (NSIS installer + exe)'
        cargo tauri build
    } else {
        Write-Host '==> cargo build --release (tauri-cli not found, exe only)'
        cargo build --release
    }
    if ($LASTEXITCODE -ne 0) { throw "build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
}

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Tauri 'target' }
$built = Get-ChildItem (Join-Path $targetDir 'release') -Filter '*.exe' |
    Where-Object { $_.Name -in @($Exe, 'earshot.exe') } | Select-Object -First 1
if (-not $built) { throw "no exe found in $targetDir\release" }

$stage = Join-Path $Dist 'Earshot'
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $built.FullName (Join-Path $stage $Exe)
Copy-Item (Join-Path $Root 'LICENSE') $stage

$zip = Join-Path $Dist $Release
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash
Write-Host "==> $zip"
Write-Host "    SHA256 $hash"
