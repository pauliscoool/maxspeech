# Fast MaxSpeech installer build — reserves headroom for rustc/lld (~8GB Node + parallel cargo).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:CARGO_BUILD_JOBS = "8"
$env:CARGO_TERM_COLOR = "always"
# Vite/tsc + Tauri CLI get a large V8 heap so the frontend step doesn't thrash.
$env:NODE_OPTIONS = "--max-old-space-size=8192"

$conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$ver = $conf.version
Write-Host "Building MaxSpeech $ver installer (CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS, NODE heap 8GB)..." -ForegroundColor Cyan
# Bundles often succeed even when updater signing fails (missing TAURI_SIGNING_PRIVATE_KEY).
npm run tauri -- build
$tauriExit = $LASTEXITCODE

$nsis = Join-Path $root "src-tauri\target\release\bundle\nsis\MaxSpeech_${ver}_x64-setup.exe"
$webDl = Join-Path $root "website\downloads\MaxSpeech_${ver}_x64-setup.exe"
if (-not (Test-Path $nsis)) {
  Write-Error "Installer not found at $nsis (tauri exit=$tauriExit)"
}
New-Item -ItemType Directory -Force -Path (Split-Path $webDl) | Out-Null
Copy-Item -Force $nsis $webDl

$manifest = @{
  version = $ver
  notes   = "MaxSpeech $ver"
  url     = "https://maxspeech.vercel.app/downloads/MaxSpeech_${ver}_x64-setup.exe"
  github  = "https://github.com/pauliscoool/maxspeech/releases/latest"
} | ConvertTo-Json
$manifestDir = Join-Path $root "website\updates"
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
Set-Content -Path (Join-Path $manifestDir "latest.json") -Value $manifest -Encoding utf8

Write-Host ""
Write-Host "Installer ready:" -ForegroundColor Green
Write-Host "  $nsis"
Write-Host "  $webDl"
Write-Host "  website\updates\latest.json"
if ($tauriExit -ne 0) {
  Write-Host "Note: tauri exited $tauriExit (often updater signing). Installer above is still usable." -ForegroundColor Yellow
}
