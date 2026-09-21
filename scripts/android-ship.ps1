<#
.SYNOPSIS
  Build → install → launch MaxSpeech Android, then scan device logs for failures.
  Exits non-zero if a crash / overlay attach failure appears so agents can fix + rebuild.

.EXAMPLE
  .\scripts\android-ship.ps1
  .\scripts\android-ship.ps1 -Debug
  .\scripts\android-ship.ps1 -SkipBuild
#>
param(
    [switch]$Debug,
    [switch]$SkipBuild,
    [int]$WatchSeconds = 8
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$androidDir = Join-Path $root "android"
$adb = Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe"
$sdk = Join-Path $env:LOCALAPPDATA "Android\Sdk"
$jdk = "C:\Program Files\Microsoft\jdk-17.0.20.101-hotspot"

if (-not (Test-Path $adb)) { throw "adb missing: $adb" }
if (-not (Test-Path $jdk)) {
    $jdk = $env:JAVA_HOME
    if (-not $jdk -or -not (Test-Path $jdk)) { throw "JDK 17 not found" }
}

$env:ANDROID_HOME = $sdk
$env:JAVA_HOME = $jdk

$devices = & $adb devices | Select-String "`tdevice$"
if (-not $devices) {
    throw "No authorized device. Plug in phone, unlock, accept USB debugging."
}

$sdkEscaped = $sdk -replace '\\', '\\'
Set-Content -Path (Join-Path $androidDir "local.properties") -Value "sdk.dir=$sdkEscaped" -Encoding ASCII

$variant = if ($Debug) { "Debug" } else { "Release" }
$task = ":app:assemble$variant"
$apkRel = if ($Debug) {
    "app\build\outputs\apk\debug\app-debug.apk"
} else {
    "app\build\outputs\apk\release\app-release.apk"
}
$apk = Join-Path $androidDir $apkRel
$pkg = if ($Debug) { "com.maxspeech.android.debug" } else { "com.maxspeech.android" }

function Write-Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

if (-not $SkipBuild) {
    Write-Step "Building $variant"
    Push-Location $androidDir
    try {
        & .\gradlew.bat $task --quiet
        if ($LASTEXITCODE -ne 0) { throw "Gradle failed ($LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
    if (-not (Test-Path $apk)) { throw "APK missing: $apk" }
    Write-Host "APK: $apk ($( [math]::Round((Get-Item $apk).Length / 1MB, 1) ) MB)"
}

Write-Step "Installing $pkg"
& $adb install -r $apk
if ($LASTEXITCODE -ne 0) { throw "adb install failed" }

Write-Step "Clearing logcat + launching"
& $adb logcat -c | Out-Null
& $adb shell am force-stop $pkg 2>$null
& $adb shell am start -n "$pkg/com.maxspeech.android.MainActivity"
if ($LASTEXITCODE -ne 0) { throw "launch failed" }

Write-Step "Watching logs for ${WatchSeconds}s"
Start-Sleep -Seconds $WatchSeconds

$dump = & $adb logcat -d -t 400
$dump | Out-File -FilePath (Join-Path $root "android-last-logcat.txt") -Encoding utf8

$crashPatterns = @(
    "FATAL EXCEPTION",
    "AndroidRuntime: FATAL",
    "Uncaught crash",
    "ViewTreeLifecycleOwner not found",
    "FloatingMic: ensureShown: attach failed",
    "promoteForeground failed",
    "Floating mic: overlay permission missing"
)
$goodPatterns = @(
    "attach: window added",
    "ensureShown: attached ok",
    "ensureShown: already attached",
    "Floating mic: showing",
    "promoteForeground: ok",
    "OverlayService: start requested",
    "start requested"
)

$hits = @()
foreach ($p in $crashPatterns) {
    $m = $dump | Select-String -SimpleMatch $p
    if ($m) { $hits += $m }
}

$goods = @()
foreach ($p in $goodPatterns) {
    $m = $dump | Select-String -SimpleMatch $p
    if ($m) { $goods += $m }
}

Write-Host "`n--- healthy signals ---" -ForegroundColor Green
if ($goods.Count -eq 0) {
    Write-Host "(none - overlay may not have started)" -ForegroundColor Yellow
} else {
    $goods | Select-Object -Last 15 | ForEach-Object { Write-Host $_.Line }
}

Write-Host "`n--- failure signals ---" -ForegroundColor Red
if ($hits.Count -eq 0) {
    Write-Host "(none)" -ForegroundColor Green
} else {
    $hits | Select-Object -Last 25 | ForEach-Object { Write-Host $_.Line }
}

$pidNow = (& $adb shell pidof $pkg 2>$null | Out-String).Trim()
$svc = & $adb shell dumpsys activity services $pkg 2>&1 | Out-String
$fg = ($svc -match "OverlayService") -and ($svc -match "isForeground=true")

Write-Host "`n--- status ---"
if ($pidNow) { Write-Host "pid=$pidNow" } else { Write-Host "pid=DEAD" }
Write-Host "overlayServiceFg=$fg"
Write-Host "Full dump: android-last-logcat.txt"

$failed = ($hits.Count -gt 0) -or (-not $pidNow) -or (-not $fg)
if ($failed) {
    Write-Host "`nSHIP FAILED - fix from logs above, then re-run." -ForegroundColor Red
    exit 1
}

Write-Host "`nSHIP OK" -ForegroundColor Green
exit 0
