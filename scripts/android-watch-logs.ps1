<#
.SYNOPSIS
  Filtered MaxSpeech Android logcat (errors, overlay, dictation, crashes).

.EXAMPLE
  .\scripts\android-watch-logs.ps1
  .\scripts\android-watch-logs.ps1 -Clear
  .\scripts\android-watch-logs.ps1 -Dump -Lines 200
#>
param(
    [switch]$Clear,
    [switch]$Dump,
    [int]$Lines = 150
)

$ErrorActionPreference = "Stop"
$adb = Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe"
if (-not (Test-Path $adb)) {
    throw "adb not found at $adb — install Android platform-tools"
}

$devices = & $adb devices | Select-String "`tdevice$"
if (-not $devices) {
    throw "No authorized Android device. Unlock phone and accept USB debugging."
}

# Tags we emit + system crash signals
$filter = @(
    "MaxSpeech:V",
    "FloatingMic:V",
    "OverlayService:V",
    "MaxSpeechA11y:V",
    "Dictation:V",
    "AndroidRuntime:E",
    "ActivityManager:I",
    "*:S"
) -join " "

if ($Clear) {
    & $adb logcat -c | Out-Null
    Write-Host "logcat cleared"
}

if ($Dump) {
    & $adb logcat -d -t $Lines $filter.Split(" ")
    exit $LASTEXITCODE
}

Write-Host "Watching MaxSpeech logs (Ctrl+C to stop)…"
Write-Host "Filter: MaxSpeech / FloatingMic / OverlayService / crashes"
& $adb logcat -v time $filter.Split(" ")
