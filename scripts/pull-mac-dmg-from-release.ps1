#Requires -Version 5.1
# Pull a Mac .dmg from the latest GitHub Release into website/downloads/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$destDir = Join-Path $root "website\downloads"
New-Item -ItemType Directory -Force -Path $destDir | Out-Null

$api = "https://api.github.com/repos/pauliscoool/maxspeech/releases/latest"
Write-Host "Fetching $api ..."
$release = Invoke-RestMethod -Uri $api -Headers @{ Accept = "application/vnd.github+json" }
$asset = $release.assets | Where-Object { $_.name -match '\.dmg$' } | Select-Object -First 1
if (-not $asset) {
  Write-Error "No .dmg asset on $($release.tag_name). Build on a Mac (./scripts/build-mac.sh) or unlock GitHub Actions and re-run Release."
}

$dest = Join-Path $destDir $asset.name
Write-Host "Downloading $($asset.name) ($([math]::Round($asset.size/1MB,1)) MB)..."
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $dest -UseBasicParsing
Write-Host "Saved: $dest"
Write-Host "Redeploy website/ to publish."
