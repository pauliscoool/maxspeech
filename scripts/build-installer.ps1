# Fast MaxSpeech installer build - reserves headroom for rustc/lld (~8GB Node + parallel cargo).
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
# Stable, version-less filename - windows.html and latest.json always point here so
# every old link (and the update button) resolves to whatever was built last, never
# a stale pinned version.
$webDlStable = Join-Path $root "website\downloads\MaxSpeech_x64-setup.exe"
if (-not (Test-Path $nsis)) {
  Write-Error "Installer not found at $nsis (tauri exit=$tauriExit)"
}
New-Item -ItemType Directory -Force -Path (Split-Path $webDl) | Out-Null
Copy-Item -Force $nsis $webDl
Copy-Item -Force $nsis $webDlStable

$sha256 = (Get-FileHash -Algorithm SHA256 -Path $webDlStable).Hash.ToLowerInvariant()
$sizeBytes = (Get-Item $webDlStable).Length

$stableUrl = "https://maxspeech.vercel.app/downloads/MaxSpeech_x64-setup.exe"
$manifest = @{
  version = $ver
  notes   = "MaxSpeech $ver - full Windows installer with embedded WebView2"
  # Always advertise the stable EXE — versioned URLs 307-redirect and break
  # some browser download= / in-app updater flows.
  url     = $stableUrl
  download = $stableUrl
  filename = "MaxSpeech_${ver}_x64-setup.exe"
  sha256  = $sha256
  size    = $sizeBytes
  github  = "https://github.com/pauliscoool/maxspeech/releases/latest"
  platforms = @{
    windows = $stableUrl
    macos   = "https://maxspeech.vercel.app/mac"
    linux   = "https://github.com/pauliscoool/maxspeech/releases/latest/download/MaxSpeech_0.1.54_amd64.AppImage"
  }
} | ConvertTo-Json
$manifestDir = Join-Path $root "website\updates"
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
# UTF-8 without BOM — PowerShell's Set-Content -Encoding utf8 writes a BOM that
# breaks some JSON consumers (including updater / CDN edge cases).
$latestPath = Join-Path $manifestDir "latest.json"
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText($latestPath, $manifest, $utf8NoBom)

Write-Host ""
Write-Host "Installer ready:" -ForegroundColor Green
Write-Host "  $nsis"
Write-Host "  $webDl (versioned - windows.html download name)"
Write-Host "  $webDlStable (canonical stable - redirects / updater)"
Write-Host "  size=$sizeBytes sha256=$sha256"
Write-Host "  website\updates\latest.json"
Write-Host ""
Write-Host "Next: from website/, run 'vercel deploy --prod -y' then 'vercel alias set <url> maxspeech.vercel.app'." -ForegroundColor Cyan
if ($tauriExit -ne 0) {
  Write-Host "Note: tauri exited $tauriExit (often updater signing). Installer above is still usable." -ForegroundColor Yellow
}
